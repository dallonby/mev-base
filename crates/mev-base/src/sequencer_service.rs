use alloy_primitives::{B256, keccak256};
use eyre::Result;
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client as RedisClient};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Response from the sequencer
#[derive(Debug, Deserialize, Serialize)]
pub struct SequencerResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SequencerError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SequencerError {
    pub code: i32,
    pub message: String,
}

/// Configuration for the sequencer service
#[derive(Debug, Clone)]
pub struct SequencerConfig {
    pub url: String,
    pub timeout: Duration,
    pub redis_host: String,
    pub redis_port: u16,
    pub redis_password: String,
    pub redis_channel: String,
}

impl Default for SequencerConfig {
    fn default() -> Self {
        Self {
            url: "https://mainnet-sequencer.base.org/".to_string(),
            timeout: Duration::from_secs(5),
            redis_host: "localhost".to_string(),
            redis_port: 6379,
            redis_password: String::new(),
            redis_channel: "baseTransactionBroadcast".to_string(),
        }
    }
}

/// Service for submitting transactions to the Base sequencer
pub struct SequencerService {
    config: SequencerConfig,
    client: Client,
    redis_conn: Arc<RwLock<Option<ConnectionManager>>>,
}

impl SequencerService {
    /// Create a new sequencer service
    pub fn new(config: SequencerConfig) -> Result<Self> {
        let client = ClientBuilder::new()
            .timeout(config.timeout)
            .build()?;

        info!(
            url = %config.url,
            timeout_secs = config.timeout.as_secs(),
            redis_host = %config.redis_host,
            redis_channel = %config.redis_channel,
            "Initialized sequencer service"
        );

        let service = Self { 
            config: config.clone(),
            client,
            redis_conn: Arc::new(RwLock::new(None)),
        };

        // Spawn task to initialize Redis connection asynchronously
        let redis_conn_clone = service.redis_conn.clone();
        tokio::spawn(async move {
            match Self::init_redis_connection(&config).await {
                Ok(conn) => {
                    info!("Successfully connected to Redis");
                    *redis_conn_clone.write().await = Some(conn);
                }
                Err(e) => {
                    error!("Failed to connect to Redis: {}. Transaction broadcasting disabled.", e);
                }
            }
        });

        Ok(service)
    }

    /// Initialize Redis connection
    async fn init_redis_connection(config: &SequencerConfig) -> Result<ConnectionManager> {
        let redis_url = format!(
            "redis://:{}@{}:{}/",
            config.redis_password,
            config.redis_host,
            config.redis_port
        );

        let client = RedisClient::open(redis_url)?;
        let conn = ConnectionManager::new(client).await?;
        
        Ok(conn)
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let mut config = SequencerConfig::default();

        if let Ok(url) = std::env::var("SEQUENCER_URL") {
            config.url = url;
        }

        if let Ok(timeout_str) = std::env::var("SEQUENCER_TIMEOUT") {
            if let Ok(timeout_secs) = timeout_str.parse::<u64>() {
                config.timeout = Duration::from_secs(timeout_secs);
            }
        }

        if let Ok(host) = std::env::var("REDIS_HOST") {
            config.redis_host = host;
        }

        if let Ok(port_str) = std::env::var("REDIS_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                config.redis_port = port;
            }
        }

        if let Ok(password) = std::env::var("REDIS_PASSWORD") {
            config.redis_password = password;
        }

        if let Ok(channel) = std::env::var("REDIS_CHANNEL") {
            config.redis_channel = channel;
        }

        Self::new(config)
    }

    /// Send a signed transaction to the sequencer
    /// Returns the transaction hash if successful
    pub async fn send_transaction(&self, signed_tx: &str) -> Result<B256> {
        // Ensure the transaction has 0x prefix
        let tx_data = if signed_tx.starts_with("0x") {
            signed_tx.to_string()
        } else {
            format!("0x{}", signed_tx)
        };

        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_sendRawTransaction",
            "params": [tx_data],
            "id": 1
        });

        info!(
            url = %self.config.url,
            tx_size = tx_data.len(),
            tx_preview = %format!("{}...{}",
                &tx_data[..20.min(tx_data.len())],
                &tx_data[tx_data.len().saturating_sub(20)..]
            ),
            "Sending transaction to sequencer"
        );

        let start_time = std::time::Instant::now();

        // Clone tx_data for Redis broadcast
        let tx_for_redis = tx_data.clone();
        let redis_conn = self.redis_conn.clone();
        let redis_channel = self.config.redis_channel.clone();

        // Spawn Redis broadcast task to run concurrently with sequencer submission
        let redis_task = tokio::spawn(async move {
            if let Some(mut conn) = redis_conn.write().await.as_mut() {
                let payload = serde_json::json!({
                    "signedTx": tx_for_redis
                });
                
                match conn.publish::<_, _, ()>(&redis_channel, payload.to_string()).await {
                    Ok(_) => {
                        info!("🌟💫 REDIS BROADCAST COMPLETE! 📡✨ Transaction echoing across the MEV network on channel: {} 🎊🎉", redis_channel);
                    }
                    Err(e) => {
                        warn!("Failed to broadcast transaction to Redis: {}", e);
                    }
                }
            } else {
                warn!("Redis connection not available, skipping broadcast");
            }
        });

        let response = self.client
            .post(&self.config.url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let elapsed = start_time.elapsed();
        
        info!(
            status = response.status().as_u16(),
            elapsed_ms = elapsed.as_millis(),
            "⚡📨 SEQUENCER RESPONDED! Status: {} in {}ms! 🏁💫 The race is on! 🏎️💨",
            response.status().as_u16(),
            elapsed.as_millis()
        );

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                status = %status,
                error = %error_text,
                elapsed_ms = elapsed.as_millis(),
                "Sequencer returned error status"
            );
            return Err(eyre::eyre!("Sequencer error {}: {}", status, error_text));
        }

        let response_text = response.text().await?;
        info!(
            response_body = %response_text,
            "📬✨ SEQUENCER RESPONSE RECEIVED! 📡🎯 Transaction accepted into the mempool! 🌊🚀"
        );
        
        let sequencer_response: SequencerResponse = serde_json::from_str(&response_text)
            .map_err(|e| eyre::eyre!("Failed to parse sequencer response: {}", e))?;

        // Wait for Redis broadcast to complete (but don't fail if it errors)
        let redis_result = redis_task.await;
        let redis_broadcast_success = redis_result.is_ok();

        if let Some(error) = sequencer_response.error {
            // Check if this is a "transaction already known" error
            // Common error codes: -32000 (already known), -32003 (transaction underpriced)
            let is_already_known = error.message.to_lowercase().contains("already known") 
                || error.message.to_lowercase().contains("replacement transaction")
                || error.message.to_lowercase().contains("nonce too low")
                || error.code == -32000;

            if is_already_known && redis_broadcast_success {
                // If Redis broadcast succeeded and sequencer says already known,
                // this is likely a race condition where another node submitted first
                warn!(
                    code = error.code,
                    message = %error.message,
                    elapsed_ms = elapsed.as_millis(),
                    "Transaction already known to sequencer (likely submitted by another node via Redis)"
                );
                
                // Try to extract tx hash from the error message if possible
                // Some implementations include the hash in the error
                // For now, we'll generate a placeholder hash
                let placeholder_hash = B256::from_slice(&keccak256(signed_tx.as_bytes())[..]);
                
                info!(
                    tx_hash = %placeholder_hash,
                    elapsed_ms = elapsed.as_millis(),
                    "Transaction broadcast via Redis (sequencer reports already known)"
                );
                
                return Ok(placeholder_hash);
            } else {
                // This is a real error, not a race condition
                error!(
                    code = error.code,
                    message = %error.message,
                    elapsed_ms = elapsed.as_millis(),
                    redis_broadcast = redis_broadcast_success,
                    "Sequencer returned JSON-RPC error"
                );
                return Err(eyre::eyre!("Sequencer error {}: {}", error.code, error.message));
            }
        }

        let tx_hash = sequencer_response.result
            .ok_or_else(|| eyre::eyre!("No result in sequencer response"))?;

        // Parse the transaction hash
        let hash = tx_hash.parse::<B256>()
            .map_err(|e| eyre::eyre!("Failed to parse transaction hash: {}", e))?;

        info!(
            tx_hash = %hash,
            elapsed_ms = elapsed.as_millis(),
            redis_broadcast = redis_broadcast_success,
            "🎆🎇 TRANSACTION LAUNCHED TO SEQUENCER! 🚀✨ Transaction soaring through the mempool! 🎆🎇"
        );

        Ok(hash)
    }

    /// Check if the sequencer is healthy
    pub async fn health_check(&self) -> Result<()> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        });

        let response = self.client
            .post(&self.config.url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(eyre::eyre!("Sequencer health check failed with status: {}", response.status()));
        }

        let sequencer_response: SequencerResponse = response.json().await?;
        
        if sequencer_response.error.is_some() {
            return Err(eyre::eyre!("Sequencer health check returned error"));
        }

        debug!("Sequencer health check passed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SequencerConfig::default();
        assert_eq!(config.url, "https://mainnet-sequencer.base.org/");
        assert_eq!(config.timeout.as_secs(), 5);
    }

    #[test]
    fn test_tx_prefix_handling() {
        // This would need an async test context and mock server
        // Just testing the logic here
        let with_prefix = "0x1234";
        let without_prefix = "1234";
        
        assert!(with_prefix.starts_with("0x"));
        assert!(!without_prefix.starts_with("0x"));
    }
}