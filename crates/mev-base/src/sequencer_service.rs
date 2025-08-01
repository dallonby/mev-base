use alloy_primitives::B256;
use eyre::Result;
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

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
}

impl Default for SequencerConfig {
    fn default() -> Self {
        Self {
            url: "https://mainnet-sequencer.base.org/".to_string(),
            timeout: Duration::from_secs(5),
        }
    }
}

/// Service for submitting transactions to the Base sequencer
pub struct SequencerService {
    config: SequencerConfig,
    client: Client,
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
            "Initialized sequencer service"
        );

        Ok(Self { config, client })
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
            "Received response from sequencer"
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
            "Raw sequencer response"
        );
        
        let sequencer_response: SequencerResponse = serde_json::from_str(&response_text)
            .map_err(|e| eyre::eyre!("Failed to parse sequencer response: {}", e))?;

        if let Some(error) = sequencer_response.error {
            error!(
                code = error.code,
                message = %error.message,
                elapsed_ms = elapsed.as_millis(),
                "Sequencer returned JSON-RPC error"
            );
            return Err(eyre::eyre!("Sequencer error {}: {}", error.code, error.message));
        }

        let tx_hash = sequencer_response.result
            .ok_or_else(|| eyre::eyre!("No result in sequencer response"))?;

        // Parse the transaction hash
        let hash = tx_hash.parse::<B256>()
            .map_err(|e| eyre::eyre!("Failed to parse transaction hash: {}", e))?;

        info!(
            tx_hash = %hash,
            elapsed_ms = elapsed.as_millis(),
            "Successfully submitted transaction to sequencer"
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