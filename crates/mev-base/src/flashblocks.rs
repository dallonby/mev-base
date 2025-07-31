use std::{io::Read, time::Duration};
use alloy_primitives::B256;
use alloy_consensus::TxEnvelope;
use alloy_rlp::Decodable;
use futures_util::StreamExt;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};
use url::Url;
use rollup_boost::FlashblocksPayloadV1;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata from flashblocks payload
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Metadata {
    pub receipts: HashMap<String, serde_json::Value>,
    pub new_account_balances: HashMap<String, String>,
    pub block_number: u64,
}

/// A processed flashblocks event
#[derive(Debug, Clone)]
pub struct FlashblocksEvent {
    pub block_number: u64,
    pub index: u32,
    pub transactions: Vec<TxEnvelope>,
    pub state_root: B256,
    pub receipts_root: B256,
    #[allow(dead_code)]
    pub metadata: Metadata,
    pub received_at: std::time::Instant,
}

/// Actor messages for internal communication
#[derive(Debug)]
enum ActorMessage {
    BestPayload { payload: FlashblocksPayloadV1 },
}

/// Main flashblocks client
pub struct FlashblocksClient {
    sender: mpsc::Sender<ActorMessage>,
    event_sender: broadcast::Sender<FlashblocksEvent>,
    ws_url: String,
}

impl FlashblocksClient {
    pub fn new(ws_url: String, event_buffer_size: usize) -> Self {
        let (sender, _mailbox) = mpsc::channel(100);
        let (event_sender, _) = broadcast::channel(event_buffer_size);
        
        Self {
            sender,
            event_sender,
            ws_url,
        }
    }
    
    /// Subscribe to flashblocks events
    pub fn subscribe(&self) -> broadcast::Receiver<FlashblocksEvent> {
        self.event_sender.subscribe()
    }
    
    /// Start the websocket connection and event processing
    pub async fn start(&mut self) -> eyre::Result<()> {
        let url = Url::parse(&self.ws_url)?;
        info!("Connecting to Flashblocks WebSocket at {}", url);
        
        let _sender = self.sender.clone();
        let event_sender_clone = self.event_sender.clone();
        
        // Create a channel for the actor loop
        let (actor_sender, mut actor_mailbox) = mpsc::channel(100);
        
        // Replace our sender with the actor sender
        self.sender = actor_sender.clone();
        
        // Spawn WebSocket handler
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            const MAX_BACKOFF: Duration = Duration::from_secs(30);
            
            loop {
                match connect_async(url.as_str()).await {
                    Ok((ws_stream, _)) => {
                        info!("WebSocket connected successfully");
                        backoff = Duration::from_secs(1); // Reset backoff on success
                        
                        let (_write, mut read) = ws_stream.split();
                        
                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(Message::Binary(bytes)) => {
                                    match try_parse_message(&bytes) {
                                        Ok(text) => {
                                            match serde_json::from_str::<FlashblocksPayloadV1>(&text) {
                                                Ok(payload) => {
                                                    let _ = actor_sender.send(ActorMessage::BestPayload { payload }).await;
                                                }
                                                Err(e) => {
                                                    error!("Failed to parse flashblocks message: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to decode message: {}", e);
                                        }
                                    }
                                }
                                Ok(Message::Close(_)) => {
                                    warn!("WebSocket closed by server");
                                    break;
                                }
                                Ok(Message::Text(text)) => {
                                    // Try to parse text messages as well
                                    match serde_json::from_str::<FlashblocksPayloadV1>(&text) {
                                        Ok(payload) => {
                                            let _ = actor_sender.send(ActorMessage::BestPayload { payload }).await;
                                        }
                                        Err(e) => {
                                            debug!("Received non-flashblocks text message: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("WebSocket error: {}", e);
                                    break;
                                }
                                _ => {} // Ignore ping/pong
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect to WebSocket: {}, retrying in {:?}", e, backoff);
                    }
                }
                
                tokio::time::sleep(backoff).await;
                backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
            }
        });
        
        // Spawn message processor
        tokio::spawn(async move {
            while let Some(message) = actor_mailbox.recv().await {
                match message {
                    ActorMessage::BestPayload { payload } => {
                        process_payload(payload, &event_sender_clone).await;
                    }
                }
            }
        });
        
        Ok(())
    }
}

/// Try to parse message, handling brotli compression
fn try_parse_message(bytes: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // First try as plain text
    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        if text.trim_start().starts_with("{") {
            return Ok(text);
        }
    }
    
    // Try brotli decompression
    let mut decompressor = brotli::Decompressor::new(bytes, 4096);
    let mut decompressed = Vec::new();
    decompressor.read_to_end(&mut decompressed)?;
    
    let text = String::from_utf8(decompressed)?;
    Ok(text)
}

/// Process a flashblocks payload and emit events
async fn process_payload(
    payload: FlashblocksPayloadV1,
    event_sender: &broadcast::Sender<FlashblocksEvent>,
) {
    // Parse metadata
    let metadata: Metadata = match serde_json::from_value(payload.metadata.clone()) {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to deserialize metadata: {}", e);
            return;
        }
    };
    
    let block_number = metadata.block_number;
    let diff = payload.diff;
    
    // Convert transactions
    let mut transactions = Vec::new();
    for tx_bytes in diff.transactions {
        // Parse transaction bytes using RLP decoding
        match TxEnvelope::decode(&mut tx_bytes.as_ref()) {
            Ok(tx) => transactions.push(tx),
            Err(e) => {
                warn!("Failed to decode transaction: {}", e);
            }
        }
    }
    
    let event = FlashblocksEvent {
        block_number,
        index: payload.index as u32,
        transactions,
        state_root: diff.state_root,
        receipts_root: diff.receipts_root,
        metadata,
        received_at: std::time::Instant::now(),
    };
    
    // Send event to subscribers
    match event_sender.send(event) {
        Ok(count) => {
            debug!("Sent flashblocks event to {} subscribers", count);
        }
        Err(_) => {
            // No subscribers, that's ok
        }
    }
}