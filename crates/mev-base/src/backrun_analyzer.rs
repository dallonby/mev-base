use alloy_primitives::{Address, U256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::flashblock_state::FlashblockStateSnapshot;
use crate::gradient_descent::GradientOptimizer;
use alloy_consensus::Transaction;

/// Token pair processor configuration
#[derive(Clone, Debug)]
pub struct TokenPairProcessorConfig {
    pub name: String,
    pub tokens: Vec<Address>,
    pub accounts: Vec<Address>,
    pub contract_address: Address,
    pub default_value: U256,
    pub data_format: String, // "short" or "long"
}

/// Backrun analyzer for monitoring token pair processors
pub struct BackrunAnalyzer {
    configs: HashMap<String, TokenPairProcessorConfig>,
    gradient_optimizer: Arc<GradientOptimizer>,
    min_profit_threshold: U256,
}

impl BackrunAnalyzer {
    pub fn new(min_profit_threshold: U256) -> Self {
        let mut analyzer = Self {
            configs: HashMap::new(),
            gradient_optimizer: Arc::new(GradientOptimizer::new()),
            min_profit_threshold,
        };
        
        // Initialize all processor configs
        analyzer.initialize_configs();
        analyzer
    }
    
    /// Convert an address string (e.g., "0xe5C17Deb99f15033451b63d2Acf34d840211b3bB") 
    /// to the byte array format needed for TokenPairProcessorConfig
    pub fn address_to_bytes(address_str: &str) -> Result<[u8; 20], String> {
        // Remove "0x" prefix if present
        let hex_str = address_str.strip_prefix("0x").unwrap_or(address_str);
        
        // Check length
        if hex_str.len() != 40 {
            return Err(format!("Invalid address length: expected 40 hex chars, got {}", hex_str.len()));
        }
        
        // Parse hex string to bytes
        let bytes = hex::decode(hex_str)
            .map_err(|e| format!("Invalid hex string: {}", e))?;
        
        // Convert to fixed array
        let mut array = [0u8; 20];
        array.copy_from_slice(&bytes);
        Ok(array)
    }
    
    /// Helper method to format address bytes for display in code
    pub fn format_address_bytes(bytes: &[u8; 20]) -> String {
        let hex_parts: Vec<String> = bytes.iter()
            .map(|b| format!("0x{:02x}", b))
            .collect();
        
        format!("[{}]", hex_parts.join(", "))
    }

    /*
    Example: Converting an address to the required format:
    
    let addr_str = "0xe5C17Deb99f15033451b63d2Acf34d840211b3bB";
    let bytes = BackrunAnalyzer::address_to_bytes(addr_str).unwrap();
    println!("contract_address: Address::from({}),", BackrunAnalyzer::format_address_bytes(&bytes));
    
    // This would output:
    // contract_address: Address::from([0xe5, 0xc1, 0x7d, 0xeb, 0x99, 0xf1, 0x50, 0x33, 0x45, 0x1b, 0x63, 0xd2, 0xac, 0xf3, 0x4d, 0x84, 0x02, 0x11, 0xb3, 0xbb]),
    
    // Example creating a new config:
            TokenPairProcessorConfig {
                name: "NewPair".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x72AB388E2E2F6FaceF59E3C3FA2C4E29011c2D38").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x72AB388E2E2F6FaceF59E3C3FA2C4E29011c2D38").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x72AB388E2E2F6FaceF59E3C3FA2C4E29011c2D38").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xe5C17Deb99f15033451b63d2Acf34d840211b3bB").unwrap()),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
    */

    // Initialize processor configurations (ported from TypeScript)
    fn initialize_configs(&mut self) {
        // Port all the configs from processorConfigs.ts
        let configs = vec![
            TokenPairProcessorConfig {
                name: "WethAeroSpectre".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x64fcc3a02eeeba05ef701b7eed066c6ebd5d4e51").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x295dc5279B8df362DF8B848276D0A9264512b09F").unwrap()),
                default_value: U256::from(1000),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethUsdcAero".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xdB7FD121983aDD932Afc73a73d869d8096810529").unwrap()),
                default_value: U256::from(3000),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethWgcDegen".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4ed4e862860bed51a9570b96d89af5e1b0efefed").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xfb18511f1590a494360069f3640c27d55c2b5290").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x3AfD01d840b36C0cA9Ee4AB75B503f60fE8E7458").unwrap()),
                default_value: U256::from(1000),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethUsdcCbxrp".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x41e357ea17eed8e3ee32451f8e5cba824af58dbf").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xe5C17Deb99f15033451b63d2Acf34d840211b3bB").unwrap()),
                default_value: U256::from(3000),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethUsdc".to_string(),
                tokens: vec![],
                accounts: vec![
                    Address::from([0x72, 0xAB, 0x38, 0x8E, 0x2E, 0x2F, 0x6F, 0xac, 0xeF, 0x59, 0xE3, 0xC3, 0xFA, 0x2C, 0x4E, 0x29, 0x01, 0x1c, 0x2D, 0x38]),
                    Address::from([0x88, 0x3e, 0x4A, 0xE0, 0xA8, 0x17, 0xf2, 0x90, 0x15, 0x00, 0x97, 0x1B, 0x35, 0x3b, 0x5d, 0xD8, 0x9A, 0xa5, 0x21, 0x84]),
                ],
                contract_address: Address::from([0x38, 0xce, 0xf6, 0x27, 0x79, 0x42, 0xfa, 0xF6, 0x6B, 0x9c, 0xD9, 0xf1, 0xb5, 0x13, 0x2d, 0x68, 0xBA, 0x17, 0x5b, 0x32]),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcKtaWeth".to_string(),
                tokens: vec![
                    Address::from([0xc0, 0x63, 0x40, 0x90, 0xf2, 0xfe, 0x6c, 0x6d, 0x75, 0xe6, 0x1b, 0xe2, 0xb9, 0x49, 0x46, 0x4a, 0xbb, 0x49, 0x89, 0x73]),
                ],
                accounts: vec![],
                contract_address: Address::from([0xFe, 0x1f, 0x37, 0xaB, 0x84, 0xBb, 0x04, 0x30, 0x0C, 0xB2, 0x6F, 0x8E, 0xf7, 0xe8, 0x88, 0x70, 0xc2, 0x56, 0x1B, 0x94]),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcSpartansWeth".to_string(),
                tokens: vec![
                    Address::from([0x11, 0x4e, 0xee, 0x49, 0x3a, 0x90, 0x9a, 0x4e, 0xba, 0x20, 0xbd, 0x2b, 0xd8, 0x6e, 0xdd, 0x4f, 0x29, 0x34, 0x2c, 0x88]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x18, 0x1e, 0xa6, 0x89, 0x74, 0xC1, 0x9b, 0x79, 0x3d, 0x80, 0x59, 0x13, 0x6b, 0x2F, 0xE2, 0x0B, 0x04, 0x41, 0xFd, 0x0d]),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcUsdtWeth".to_string(),
                tokens: vec![
                    Address::from([0xfd, 0xe4, 0xc9, 0x6c, 0x85, 0x93, 0x53, 0x6e, 0x31, 0xf2, 0x29, 0xea, 0x8f, 0x37, 0xb2, 0xad, 0xa2, 0x69, 0x9b, 0xb2]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x5A, 0x16, 0x36, 0x93, 0x4B, 0xA3, 0x43, 0x97, 0xa7, 0x3C, 0x8f, 0x8A, 0xFd, 0xF4, 0xF9, 0x6A, 0xEe, 0x77, 0x80, 0x01]),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "EurcUsdcWeth".to_string(),
                tokens: vec![
                    Address::from([0x60, 0xa3, 0xe3, 0x5c, 0xc3, 0x02, 0xbf, 0xa4, 0x4c, 0xb2, 0x88, 0xbc, 0x5a, 0x4f, 0x31, 0x6f, 0xdb, 0x1a, 0xdb, 0x42]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x7e, 0x59, 0x8D, 0xe3, 0xCd, 0x20, 0xc3, 0x1B, 0x28, 0x94, 0xe4, 0x6b, 0xB1, 0x37, 0x03, 0x33, 0x89, 0x82, 0xD6, 0xdb]),
                default_value: U256::from(1300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "AeroWeth".to_string(),
                tokens: vec![
                    Address::from([0x94, 0x01, 0x81, 0xa9, 0x4a, 0x35, 0xa4, 0x56, 0x9e, 0x45, 0x29, 0xa3, 0xcd, 0xfb, 0x74, 0xe3, 0x8f, 0xd9, 0x86, 0x31]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x4D, 0xf3, 0xAf, 0xe2, 0x1b, 0x52, 0x8d, 0x01, 0x0b, 0x8d, 0xCd, 0xe6, 0x5E, 0xD2, 0x51, 0x75, 0x6d, 0xeA, 0x34, 0x65]),
                default_value: U256::from(988),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcXccxWeth".to_string(),
                tokens: vec![
                    Address::from([0x6f, 0x8c, 0x1d, 0xe0, 0x7c, 0x9e, 0x59, 0xa8, 0x28, 0x97, 0x05, 0xb1, 0x03, 0x3a, 0xf3, 0x83, 0xdc, 0x36, 0x81, 0xb1]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x62, 0x4E, 0xeB, 0xAf, 0xD2, 0x55, 0x32, 0xF2, 0xd3, 0x23, 0xC5, 0xB4, 0xa6, 0x97, 0x49, 0x56, 0x41, 0xb0, 0x4c, 0xF7]),
                default_value: U256::from(900),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdbcWethUsdc".to_string(),
                tokens: vec![
                    Address::from([0xd9, 0xaa, 0xec, 0x86, 0xb6, 0x5d, 0x86, 0xf6, 0xa7, 0xb5, 0xb1, 0xb0, 0xc4, 0x2f, 0xfa, 0x53, 0x17, 0x10, 0xb6, 0xca]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x49, 0xcE, 0x99, 0x10, 0xC1, 0xD8, 0xDD, 0xAC, 0x57, 0xD1, 0x00, 0x1d, 0xfd, 0xc8, 0xa2, 0x57, 0x76, 0xF7, 0x9a, 0x8f]),
                default_value: U256::from(900),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcPfusdcPbasedpepePepe".to_string(),
                tokens: vec![
                    Address::from([0x9b, 0xbb, 0xd7, 0xa3, 0x6a, 0x28, 0x7d, 0xf7, 0x8a, 0x11, 0x81, 0x34, 0x06, 0xbe, 0xac, 0xb0, 0x36, 0xba, 0x2b, 0xb6]),
                    Address::from([0xce, 0x18, 0xd9, 0xc3, 0x07, 0x9f, 0x25, 0x6e, 0xe4, 0xc7, 0xa4, 0x44, 0x83, 0x6e, 0x40, 0x84, 0x7c, 0x57, 0x87, 0x76]),
                    Address::from([0x52, 0xb4, 0x92, 0xa3, 0x3e, 0x44, 0x7c, 0xdb, 0x85, 0x4c, 0x7f, 0xc1, 0x9f, 0x1e, 0x57, 0xe8, 0xbf, 0xa1, 0x77, 0x7d]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x9e, 0xcE, 0x02, 0x97, 0x13, 0x5d, 0xE9, 0xa2, 0x96, 0xa2, 0xeE, 0xd1, 0x78, 0x7E, 0xBE, 0x98, 0x72, 0x88, 0x36, 0x83]),
                default_value: U256::from(3200),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethPfwethPbrianBrian".to_string(),
                tokens: vec![
                    Address::from([0x23, 0xbd, 0x2f, 0xe4, 0x4c, 0xdb, 0xf6, 0x69, 0x5e, 0xa8, 0x9f, 0x08, 0x6b, 0xe1, 0x5f, 0xeb, 0x83, 0xe6, 0x9b, 0x7c]),
                    Address::from([0x5a, 0xfa, 0x72, 0x0d, 0x50, 0x09, 0x3a, 0x36, 0xe9, 0x4f, 0x53, 0x8a, 0xc0, 0xcb, 0x72, 0xff, 0xc4, 0xe3, 0x7c, 0x42]),
                    Address::from([0x3e, 0xcc, 0xed, 0x5b, 0x41, 0x6e, 0x58, 0x66, 0x4f, 0x04, 0xa3, 0x9d, 0xd1, 0x89, 0x35, 0xeb, 0x71, 0xd3, 0x3b, 0x15]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x5D, 0x49, 0xc3, 0xEc, 0x92, 0x6F, 0x78, 0x8a, 0x55, 0x93, 0x6B, 0xf9, 0x64, 0xc8, 0xFf, 0x48, 0x39, 0x89, 0x4b, 0xc8]),
                default_value: U256::from(1200),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethBrianPbrianPfweth".to_string(),
                tokens: vec![
                    Address::from([0x3e, 0xcc, 0xed, 0x5b, 0x41, 0x6e, 0x58, 0x66, 0x4f, 0x04, 0xa3, 0x9d, 0xd1, 0x89, 0x35, 0xeb, 0x71, 0xd3, 0x3b, 0x15]),
                    Address::from([0x5a, 0xfa, 0x72, 0x0d, 0x50, 0x09, 0x3a, 0x36, 0xe9, 0x4f, 0x53, 0x8a, 0xc0, 0xcb, 0x72, 0xff, 0xc4, 0xe3, 0x7c, 0x42]),
                    Address::from([0x23, 0xbd, 0x2f, 0xe4, 0x4c, 0xdb, 0xf6, 0x69, 0x5e, 0xa8, 0x9f, 0x08, 0x6b, 0xe1, 0x5f, 0xeb, 0x83, 0xe6, 0x9b, 0x7c]),
                ],
                accounts: vec![],
                contract_address: Address::from([0xAD, 0x2c, 0x7c, 0xCF, 0x6C, 0x87, 0xA8, 0x19, 0xb7, 0x41, 0x9d, 0x23, 0x42, 0xC8, 0x1d, 0x91, 0xb4, 0xb4, 0x09, 0x8d]),
                default_value: U256::from(1500),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcPfusdcPusdpPeasUsdcWeth".to_string(),
                tokens: vec![
                    Address::from([0x39, 0x24, 0x25, 0x17, 0xde, 0xa0, 0x58, 0x9b, 0x72, 0x94, 0xa5, 0xd8, 0xd1, 0x09, 0xfa, 0xbf, 0x6d, 0xe2, 0x2c, 0x41]),
                    Address::from([0x6b, 0x93, 0x80, 0x67, 0x6d, 0x2c, 0x53, 0x1c, 0xe6, 0xb1, 0x29, 0xc9, 0x1d, 0x4c, 0x9b, 0x7f, 0x76, 0xb2, 0xb2, 0x99]),
                    Address::from([0x02, 0xf9, 0x28, 0x00, 0xf5, 0x7b, 0xcd, 0x74, 0x06, 0x6f, 0x57, 0x09, 0xf1, 0xda, 0xa1, 0xa4, 0x30, 0x2d, 0xf8, 0x75]),
                ],
                accounts: vec![],
                contract_address: Address::from([0xcd, 0x70, 0xf4, 0x51, 0x25, 0x2c, 0x74, 0x06, 0x42, 0x4f, 0xdA, 0x55, 0x01, 0x6c, 0x8b, 0x05, 0x44, 0x3D, 0x6B, 0x49]),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "UsdcPfusdcPtibbirTibbirWeth".to_string(),
                tokens: vec![
                    Address::from([0xf0, 0xde, 0x99, 0x62, 0x92, 0xa1, 0x95, 0xdb, 0xb5, 0xfc, 0x94, 0xff, 0x18, 0x99, 0x78, 0x1c, 0x87, 0x4a, 0x97, 0x50]),
                    Address::from([0x2d, 0xad, 0x8b, 0x75, 0x1a, 0xd1, 0x5c, 0x41, 0x86, 0xea, 0x95, 0x5d, 0x6a, 0x47, 0xb7, 0x51, 0xc6, 0x68, 0x27, 0xd7]),
                    Address::from([0xa4, 0xa2, 0xe2, 0xca, 0x3f, 0xbf, 0xe2, 0x1a, 0xed, 0x83, 0x47, 0x1d, 0x28, 0xb6, 0xf6, 0x5a, 0x23, 0x3c, 0x6e, 0x00]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x64, 0xaa, 0xFD, 0x7d, 0x91, 0x67, 0xde, 0x08, 0x0C, 0x6f, 0x58, 0x84, 0x9f, 0xbc, 0x11, 0xF1, 0x6E, 0x6c, 0xa3, 0x2C]),
                default_value: U256::from(245),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "WethPfwethPpeasPeasUsdc".to_string(),
                tokens: vec![
                    Address::from([0x23, 0x57, 0x41, 0x54, 0x84, 0x2b, 0xc8, 0x6c, 0xf5, 0xbb, 0xc5, 0xf9, 0x93, 0x76, 0xcf, 0xa1, 0xe2, 0xf8, 0x24, 0x97]),
                    Address::from([0x9d, 0xbf, 0x56, 0x2a, 0x65, 0x59, 0x0c, 0x03, 0xe5, 0x40, 0x08, 0x54, 0xd4, 0x98, 0x84, 0x49, 0x15, 0xc2, 0x59, 0x44]),
                    Address::from([0x02, 0xf9, 0x28, 0x00, 0xf5, 0x7b, 0xcd, 0x74, 0x06, 0x6f, 0x57, 0x09, 0xf1, 0xda, 0xa1, 0xa4, 0x30, 0x2d, 0xf8, 0x75]),
                ],
                accounts: vec![],
                contract_address: Address::from([0xFD, 0x5d, 0x7d, 0x50, 0xA1, 0x1A, 0x7B, 0xC3, 0xB3, 0x5b, 0xb3, 0x0B, 0xB6, 0x20, 0x8d, 0x37, 0x66, 0xca, 0x95, 0x32]),
                default_value: U256::from(300),
                data_format: "short".to_string(),
            },
            TokenPairProcessorConfig {
                name: "TybgPtybgPfwethWeth".to_string(),
                tokens: vec![
                    Address::from([0x0d, 0x97, 0xf2, 0x61, 0xb1, 0xe8, 0x88, 0x45, 0x18, 0x4f, 0x67, 0x8e, 0x2d, 0x1e, 0x7a, 0x98, 0xd9, 0xfd, 0x38, 0xde]),
                    Address::from([0xf1, 0x69, 0xd8, 0x1d, 0xd5, 0xc4, 0x82, 0xd6, 0x24, 0xbf, 0xd3, 0xa9, 0xe6, 0x4a, 0x5f, 0xcb, 0x11, 0xf2, 0xa1, 0x72]),
                    Address::from([0x37, 0x5f, 0xed, 0xbf, 0xd5, 0x1f, 0xd3, 0x61, 0x74, 0xf0, 0x7c, 0x7c, 0x67, 0x37, 0x26, 0x52, 0x20, 0x03, 0xc9, 0x67]),
                ],
                accounts: vec![],
                contract_address: Address::from([0x2c, 0x66, 0x51, 0xA0, 0x2b, 0x19, 0xE3, 0x1e, 0x46, 0xC0, 0x6F, 0xd6, 0x49, 0xDF, 0xD1, 0x39, 0xcc, 0x14, 0xFC, 0x2F]),
                default_value: U256::from(9200),
                data_format: "short".to_string(),
            },
        ];
        
        for config in configs {
            self.configs.insert(config.name.clone(), config);
        }
    }
    
    /// Analyze state for backrun opportunities
    pub fn analyze_state_for_backrun(&self, state: &FlashblockStateSnapshot) -> Vec<String> {
        let mut triggered_configs = Vec::new();
        
        // Get all affected addresses from state
        let mut affected_addresses = HashSet::new();
        for addr in state.account_changes.keys() {
            affected_addresses.insert(addr.to_string().to_lowercase());
        }
        for addr in state.storage_changes.keys() {
            affected_addresses.insert(addr.to_string().to_lowercase());
        }
        
        // Check each config
        for (name, config) in &self.configs {
            // Check if any monitored accounts were touched
            let touches_accounts = config.accounts.iter().any(|account| {
                affected_addresses.contains(&format!("0x{}", hex::encode(account.as_slice())).to_lowercase())
            });
            
            // Check if any monitored tokens were touched
            let touches_tokens = config.tokens.iter().any(|token| {
                affected_addresses.contains(&format!("0x{}", hex::encode(token.as_slice())).to_lowercase())
            });
            
            if touches_accounts || touches_tokens {
                triggered_configs.push(name.clone());
            }
        }
        
        // Also check for oracle updates in transactions
        if self.has_oracle_updates(state) {
            // Oracle updates can trigger all configs
            return self.configs.keys().cloned().collect();
        }
        
        triggered_configs
    }
    
    /// Check if state contains oracle updates (Chainlink, etc)
    fn has_oracle_updates(&self, state: &FlashblockStateSnapshot) -> bool {
        // Chainlink oracle function selectors
        const ORACLE_SELECTORS: &[&[u8]] = &[
            &[0x50, 0xd2, 0x5b, 0xcd], // latestAnswer()
            &[0x9a, 0x6f, 0xc8, 0xf5], // transmit()
            &[0xc9, 0x80, 0x75, 0x39], // submit()
            &[0x6f, 0xad, 0xcf, 0x72], // forward()
        ];
        
        // Check transactions in the state for oracle function selectors
        for tx in &state.transactions {
            let calldata = tx.input();
            if calldata.len() >= 4 {
                let selector = &calldata[0..4];
                if ORACLE_SELECTORS.iter().any(|&s| s == selector) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Get a reference to the configs (for worker access)
    pub fn get_configs(&self) -> &HashMap<String, TokenPairProcessorConfig> {
        &self.configs
    }
    
    /// Get the gradient optimizer
    pub fn get_optimizer(&self) -> Arc<GradientOptimizer> {
        self.gradient_optimizer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_backrun_analyzer_creation() {
        let analyzer = BackrunAnalyzer::new(U256::from(10_000_000_000_000u64)); // 0.00001 ETH (10 microether)
        assert!(!analyzer.configs.is_empty());
        assert_eq!(analyzer.configs.len(), 16); // Should have 16 configs
    }
    
    #[test]
    fn test_config_lookup() {
        let analyzer = BackrunAnalyzer::new(U256::from(10_000_000_000_000u64)); // 0.00001 ETH (10 microether)
        assert!(analyzer.configs.contains_key("WethUsdc"));
        assert!(analyzer.configs.contains_key("AeroWeth"));
    }
}