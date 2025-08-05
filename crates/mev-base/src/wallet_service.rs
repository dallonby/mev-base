use alloy_signer_local::PrivateKeySigner;
use eyre::Result;
use rand::Rng;
use tracing::{debug, info};

/// Service for managing wallets and signing transactions
pub struct WalletService {
    wallets: Vec<PrivateKeySigner>,
}

impl WalletService {
    /// Create a new wallet service from private keys
    pub fn new(private_keys: Vec<String>) -> Result<Self> {
        if private_keys.is_empty() {
            return Err(eyre::eyre!("No private keys provided"));
        }

        let mut wallets = Vec::new();
        
        for (index, key) in private_keys.iter().enumerate() {
            // Remove 0x prefix if present
            let clean_key = if key.starts_with("0x") {
                &key[2..]
            } else {
                key
            };

            match clean_key.parse::<PrivateKeySigner>() {
                Ok(wallet) => {
                    info!(
                        index = index,
                        address = %wallet.address(),
                        "Initialized wallet"
                    );
                    wallets.push(wallet);
                }
                Err(e) => {
                    return Err(eyre::eyre!("Failed to parse private key at index {}: {}", index, e));
                }
            }
        }

        info!(count = wallets.len(), "Initialized wallets");
        
        Ok(Self { wallets })
    }

    /// Initialize from environment variables
    pub fn from_env() -> Result<Self> {
        let private_keys_env = std::env::var("WALLET_PRIVATE_KEYS")
            .map_err(|_| eyre::eyre!("WALLET_PRIVATE_KEYS environment variable is required"))?;

        // Parse comma-separated private keys
        let private_keys: Vec<String> = private_keys_env
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if private_keys.is_empty() {
            return Err(eyre::eyre!("No valid private keys found in WALLET_PRIVATE_KEYS"));
        }

        Self::new(private_keys)
    }

    /// Get a wallet by index
    pub fn get_wallet(&self, index: usize) -> Result<PrivateKeySigner> {
        self.wallets
            .get(index)
            .cloned()
            .ok_or_else(|| eyre::eyre!("Invalid wallet index: {}. Available: 0-{}", index, self.wallets.len() - 1))
    }

    /// Get a random wallet
    pub fn get_random_wallet(&self) -> Result<PrivateKeySigner> {
        if self.wallets.is_empty() {
            return Err(eyre::eyre!("No wallets available"));
        }

        let mut rng = rand::rng();
        let index = rng.random_range(0..self.wallets.len());
        
        debug!(index = index, "Selected random wallet");
        Ok(self.wallets[index].clone())
    }

    /// Get the number of wallets
    pub fn wallet_count(&self) -> usize {
        self.wallets.len()
    }

    /// Get all wallet addresses
    pub fn get_addresses(&self) -> Vec<alloy_primitives::Address> {
        self.wallets.iter().map(|w| w.address()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_service_creation() {
        // Test with valid private key (test key, do not use in production)
        let test_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let keys = vec![test_key.to_string()];
        
        let service = WalletService::new(keys).unwrap();
        assert_eq!(service.wallet_count(), 1);
        
        let wallet = service.get_wallet(0).unwrap();
        assert_eq!(
            wallet.address().to_string(),
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        );
    }

    #[test]
    fn test_random_wallet() {
        let test_keys = vec![
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string(),
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".to_string(),
        ];
        
        let service = WalletService::new(test_keys).unwrap();
        assert_eq!(service.wallet_count(), 2);
        
        // Get random wallet should work
        let wallet = service.get_random_wallet().unwrap();
        assert!(service.get_addresses().contains(&wallet.address()));
    }
}