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
    pub check_balance_of: Option<(Address, Address)>, // (erc20_token, address_to_check)
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
                default_value: U256::from(6),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x8c54143b62cca30b0718ef8167ad5bc25881e554").unwrap()), // Address to check balance of
                )),
            },
    */

    // Initialize processor configurations (ported from TypeScript)
    fn initialize_configs(&mut self) {
        // Port all the configs from processorConfigs.ts
        let configs = vec![
            TokenPairProcessorConfig {
                name: "WethCbbtcPrompt".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x30c7235866872213f68cb1f08c37cb9eccb93452").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xA61C2ce328C2346233c801C2BD02E46Be36F74Dd").unwrap()),
                default_value: U256::from(328),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xdF4D571e55eFdc25CDD010dA9Cb35b21064DEd49").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethBenjiAero".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xbc45647ea894030a4e9801ec03479739fa2485f0").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x0aEafdAaB2a0e05335c18EbB2E9D04b183928eDa").unwrap()),
                default_value: U256::from(913),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x16905890A1D02b6F824387419319Bf4188B961b0").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethUsdcZora".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x1111111111166b7fe7bd91427724b487980afc69").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x11a40A6c3d44FF301732CB3695429b739feB4570").unwrap()),
                default_value: U256::from(823),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x1111111111166b7FE7bd91427724B487980aFc69").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xEdc625B74537eE3a10874f53D170E9c17A906B9c").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethTibbirVirtual".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xa4a2e2ca3fbfe21aed83471d28b6f65a233c6e00").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x0b3e328455c4059eeb9e3f84b5543f74e24e7e1b").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xd81A85c18DB29452De36a0f56fFC0e60FBE62366").unwrap()),
                default_value: U256::from(370),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x9c087Eb773291e50CF6c6a90ef0F4500e349B903").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "AeroWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xA7fe2a8D76D7e729105D52e15e3daC612B382eAA").unwrap()),
                default_value: U256::from(2258),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xF4DFb8647C3Ef75c5A71b7B0ee9240BdccCe8697").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "AnonPanonPfwethWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x79bbf4508b1391af3a0f4b30bb5fc4aa9ab0e07c").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xaa779272360e79193e88dd0ba96e2b1bc9da3d4e").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xf135f3a72f87ebb721ce6adfd0f5d35661056065").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xE4737263cC53D7b0a3dE4002d2C42a521280F495").unwrap()),
                default_value: U256::from(401),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x1807af3897aA6419E770D4642dF7B8b06E542C02").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "MigglesPmigglesPeasUsdc".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xb1a03eda10342529bbf8eb700a06c60441fef25d").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xdbca4ba3cf9126f4eb3ace8679221c7db42d47d9").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x02f92800f57bcd74066f5709f1daa1a4302df875").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xb0a463f5E5132Df8eEe74A2b3a2F55A650DD9330").unwrap()),
                default_value: U256::from(80),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xC16F5d5C0a2C0784EfaFEDf28B934a9F0bA21CD7").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethPfwethPtybgpTybg".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x375fedbfd51fd36174f07c7c673726522003c967").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xf169d81dd5c482d624bfd3a9e64a5fcb11f2a172").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x0d97f261b1e88845184f678e2d1e7a98d9fd38de").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xdd9723cE865B4d9A20774A8695c43121ADC38711").unwrap()),
                default_value: U256::from(30),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x8d628d22d298b4a6E3DC9171d4b7aa5229e2353c").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcPzfiZfiWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xfe9a0da6dbe7b3167a5908e7e032c4fd7fc51194").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x196bb23d5d05f3b8d28921833a2d3d7feb7d6aaf").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xd080ed3c74a20250a2c9821885203034acd2d5ae").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xDf0F538cD6472fb82f5db3e980901C379e9633a4").unwrap()),
                default_value: U256::from(50),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xeF32a6e5B1D363deD63e35af03fc53A637926DE0").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethRwaxPearwaxUsdc".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xe0023e73aab4fe9a22f059a9d27e857e027ee3dc").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x808b82194ae30418ca5eb37a10c43435f065ac5e").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x69d7d544348C52C1904D2BA5Ec3324b607983C47").unwrap()),
                default_value: U256::from(160),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xd448670823ff9667848C821BeE829c642F67E064").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethFuegoPfuegoPeasUsdc".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x36912b5cf63e509f18e53ac98b3012fa79e77bf5").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xd0b57b784ada47365ab3bda65fdf438b88252360").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x02f92800f57bcd74066f5709f1daa1a4302df875").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x9ab787aa734a0BbFb12EcbE891b8eeb0E1CdCD75").unwrap()),
                default_value: U256::from(800),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x2bbFb5A2496f405d4094D4b854DAeb9CE70D0029").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcAeroWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x0E95FB13f090B309d822C1074b95D749b42e6aFe").unwrap()),
                default_value: U256::from(526),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x20CB8f872ae894F7c9e32e621C186e5AFCe82Fd0").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcPwbltPeas".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x3dd79d6bd927615787cc95f2c7a77c9ac1af26f4").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x02f92800f57bcd74066f5709f1daa1a4302df875").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x0802a4408795639F10f829936E0e080F672eb6fE").unwrap()),
                default_value: U256::from(360),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x3Dd79d6BD927615787Cc95F2c7A77C9aC1AF26F4").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x9b0025d10E824E7E2b148953009A40B0C0792F30").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcBusdBltFblpFsblpWbltBlt".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xe974a88385935cb8846482f3ab01b6c0f70fa5f3").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xe771b4e273df31b85d7a7ae0efd22fb44bdd0633").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xa2242d0a8b0b5c1a487abfc03cd9fef6262badca").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4e74d4db6c0726ccded4656d0bce448876bb4c7a").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x3dd79d6bd927615787cc95f2c7a77c9ac1af26f4").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x1Ed147c2275CF72EA364DF7Ab88ADfcC0921bdD6").unwrap()),
                default_value: U256::from(400),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x3Dd79d6BD927615787Cc95F2c7A77C9aC1AF26F4").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x9b0025d10E824E7E2b148953009A40B0C0792F30").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcPeasPmigglesMigglesWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x02f92800f57bcd74066f5709f1daa1a4302df875").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xdbca4ba3cf9126f4eb3ace8679221c7db42d47d9").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xb1a03eda10342529bbf8eb700a06c60441fef25d").unwrap()),
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xBfDC66fc27370b2998ef1b73d3Fc6A6042100fc3").unwrap()),
                default_value: U256::from(60),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x02f92800F57BCD74066F5709F1Daa1A4302Df875").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xb0a65b3A6F9DA0e5EB057e0D5327DEDDbe17309E").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcVirtualTibbirPtibbir".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x0b3e328455c4059eeb9e3f84b5543f74e24e7e1b").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xa4a2e2ca3fbfe21aed83471d28b6f65a233c6e00").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x2dad8b751ad15c4186ea955d6a47b751c66827d7").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xf0de996292a195dbb5fc94ff1899781c874a9750").unwrap()),
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xD1cbA7BE955fAc0cc458aCe3A2a61E271b41053D").unwrap()),
                default_value: U256::from(1191),
                data_format: "short".to_string(),
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x0b3e328455c4059EEb9e3f84b5543F74E24e7E1b").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x0c3b466104545efa096b8f944c1e524E1d0D4888").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcBmxPbmxPeas".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x548f93779fbc992010c07467cbaf329dd5f059b7").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xf3e25c1512bef952f01252f4d5f6415f408c0d23").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x02f92800f57bcd74066f5709f1daa1a4302df875").unwrap()),
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xAdEcFf8F5890572D352DE8A5Ec997766e9dCAF0D").unwrap()),
                default_value: U256::from(248),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x548f93779fBC992010C07467cBaf329DD5F059B7").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x2F48C208d7Bd2b4Ff6Da005A9427eF38F035b2d8").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "KudaiPkudaiUsdcWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x288f4eb27400fa220d14b864259ad1b7f77c1594").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x7a48fc98673d7109b2a92aabbb807af5bd2f9b25").unwrap()),
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xaFC4814646FBf06b84761fdF1264c4Dc22fAAa5c").unwrap()),
                default_value: U256::from(600),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xaDE5DA9C31b77a2b95c8Dd88676AFFD2c9482139").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "UsdcWethTibbirPtibbir".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xa4a2e2ca3fbfe21aed83471d28b6f65a233c6e00").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x2dad8b751ad15c4186ea955d6a47b751c66827d7").unwrap()),
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x5D30142a0F8527fEfbdda7E427e7b66325Fa4189").unwrap()),
                default_value: U256::from(974),
                data_format: "short".to_string(),
                // check_balance_of: None,
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0x0f664afFB82f074937D5cFCD61b97F3F32d5dC50").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "ZfiPzfiUsdcWeth".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0xd080ed3c74a20250a2c9821885203034acd2d5ae").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x196bb23d5d05f3b8d28921833a2d3d7feb7d6aaf").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xfe9a0da6dbe7b3167a5908e7e032c4fd7fc51194").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x7065f0E05fDF46Ab98522F5930F38BAa04469B8a").unwrap()),
                default_value: U256::from(600),
                data_format: "short".to_string(),
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x4200000000000000000000000000000000000006").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xeF32a6e5B1D363deD63e35af03fc53A637926DE0").unwrap()), // Address to check balance of
                )),
            },
            TokenPairProcessorConfig {
                name: "WethUsdcUsdbc".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xC1043c9c8003AFdE9127192d6bFa15A19E6017fB").unwrap()),
                default_value: U256::from(90),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "WethAeroSpectre".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x64fcc3a02eeeba05ef701b7eed066c6ebd5d4e51").unwrap()),
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x8C1e725EdE2301AF3Ff0Bf23c76200C0eEd4445f").unwrap()),
                default_value: U256::from(73),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "WethUsdcAero".to_string(),
                tokens: vec![],
                accounts: vec![
                    // Convert token addresses
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()),
                    // Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
                ],
                // Convert contract address
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xEb1763F0d5F712a3f155FfF96b5Ac2433122B7DB").unwrap()),
                default_value: U256::from(200),
                data_format: "short".to_string(),
                check_balance_of: Some((
                    Address::from(BackrunAnalyzer::address_to_bytes("0x940181a94A35A4569E4529A3CDfB74e38FD98631").unwrap()), // ERC20 token (USDC)
                    Address::from(BackrunAnalyzer::address_to_bytes("0xE5B5f522E98B5a2baAe212d4dA66b865B781DB97").unwrap()), // Address to check balance of
                )),
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
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x81A0c588Bd8f7aC26884E6c46558e9323f284277").unwrap()),
                default_value: U256::from(42),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            // TokenPairProcessorConfig {
            //     name: "WethUsdcCbxrp".to_string(),
            //     tokens: vec![],
            //     accounts: vec![
            //         // Convert token addresses
            //         Address::from(BackrunAnalyzer::address_to_bytes("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()),
            //         Address::from(BackrunAnalyzer::address_to_bytes("0x41e357ea17eed8e3ee32451f8e5cba824af58dbf").unwrap()),
            //     ],
            //     // Convert contract address
            //     contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x22B2158d0F07974052B48Fe2438da071b1b18518").unwrap()),
            //     default_value: U256::from(60),
            //     data_format: "short".to_string(),
            //     check_balance_of: Some((
            //         Address::from(BackrunAnalyzer::address_to_bytes("0x41e357ea17eed8e3ee32451f8e5cba824af58dbf").unwrap()), // ERC20 token (USDC)
            //         Address::from(BackrunAnalyzer::address_to_bytes("0x8c54143b62cca30b0718ef8167ad5bc25881e554").unwrap()), // Address to check balance of
            //     )),
            // },
            TokenPairProcessorConfig {
                name: "WethUsdc".to_string(),
                tokens: vec![],
                accounts: vec![
                    Address::from([0x72, 0xAB, 0x38, 0x8E, 0x2E, 0x2F, 0x6F, 0xac, 0xeF, 0x59, 0xE3, 0xC3, 0xFA, 0x2C, 0x4E, 0x29, 0x01, 0x1c, 0x2D, 0x38]),
                    Address::from([0x88, 0x3e, 0x4A, 0xE0, 0xA8, 0x17, 0xf2, 0x90, 0x15, 0x00, 0x97, 0x1B, 0x35, 0x3b, 0x5d, 0xD8, 0x9A, 0xa5, 0x21, 0x84]),
                ],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x62633312C6a9dd1DbB8a073c96AD898c6b062F0F").unwrap()),
                default_value: U256::from(162),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcKtaWeth".to_string(),
                tokens: vec![
                    Address::from([0xc0, 0x63, 0x40, 0x90, 0xf2, 0xfe, 0x6c, 0x6d, 0x75, 0xe6, 0x1b, 0xe2, 0xb9, 0x49, 0x46, 0x4a, 0xbb, 0x49, 0x89, 0x73]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xc9E3e959Ec5a1c1dFB34854C438208d13860193c").unwrap()),
                default_value: U256::from(174),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcSpartansWeth".to_string(),
                tokens: vec![
                    Address::from([0x11, 0x4e, 0xee, 0x49, 0x3a, 0x90, 0x9a, 0x4e, 0xba, 0x20, 0xbd, 0x2b, 0xd8, 0x6e, 0xdd, 0x4f, 0x29, 0x34, 0x2c, 0x88]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x4eF2620cbC6eC0bBd7D2872ce390AB4C951feDEd").unwrap()),
                default_value: U256::from(94),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcUsdtWeth".to_string(),
                tokens: vec![
                    Address::from([0xfd, 0xe4, 0xc9, 0x6c, 0x85, 0x93, 0x53, 0x6e, 0x31, 0xf2, 0x29, 0xea, 0x8f, 0x37, 0xb2, 0xad, 0xa2, 0x69, 0x9b, 0xb2]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x91b56E95A682494A98ca36DF68d1F209D6C4B5d9").unwrap()),
                default_value: U256::from(597),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "EurcUsdcWeth".to_string(),
                tokens: vec![
                    Address::from([0x60, 0xa3, 0xe3, 0x5c, 0xc3, 0x02, 0xbf, 0xa4, 0x4c, 0xb2, 0x88, 0xbc, 0x5a, 0x4f, 0x31, 0x6f, 0xdb, 0x1a, 0xdb, 0x42]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xf0Ca4Df73823A9F587d00Ab6bb4eee9B218c5Af1").unwrap()),
                default_value: U256::from(644),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcXccxWeth".to_string(),
                tokens: vec![
                    Address::from([0x6f, 0x8c, 0x1d, 0xe0, 0x7c, 0x9e, 0x59, 0xa8, 0x28, 0x97, 0x05, 0xb1, 0x03, 0x3a, 0xf3, 0x83, 0xdc, 0x36, 0x81, 0xb1]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x9C4d9ba17FA4C0Ba2494Fdcc4EBAD9a87d428131").unwrap()),
                default_value: U256::from(634),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdbcWethUsdc".to_string(),
                tokens: vec![
                    Address::from([0xd9, 0xaa, 0xec, 0x86, 0xb6, 0x5d, 0x86, 0xf6, 0xa7, 0xb5, 0xb1, 0xb0, 0xc4, 0x2f, 0xfa, 0x53, 0x17, 0x10, 0xb6, 0xca]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x0d5f0Cbf138352FEA45E82F8b4966e2a50Fe8d5c").unwrap()),
                default_value: U256::from(381),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcPfusdcPbasedpepePepe".to_string(),
                tokens: vec![
                    Address::from([0x9b, 0xbb, 0xd7, 0xa3, 0x6a, 0x28, 0x7d, 0xf7, 0x8a, 0x11, 0x81, 0x34, 0x06, 0xbe, 0xac, 0xb0, 0x36, 0xba, 0x2b, 0xb6]),
                    Address::from([0xce, 0x18, 0xd9, 0xc3, 0x07, 0x9f, 0x25, 0x6e, 0xe4, 0xc7, 0xa4, 0x44, 0x83, 0x6e, 0x40, 0x84, 0x7c, 0x57, 0x87, 0x76]),
                    Address::from([0x52, 0xb4, 0x92, 0xa3, 0x3e, 0x44, 0x7c, 0xdb, 0x85, 0x4c, 0x7f, 0xc1, 0x9f, 0x1e, 0x57, 0xe8, 0xbf, 0xa1, 0x77, 0x7d]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xAED9aFe16c8c03876413FA1cCBb17646A3B24266").unwrap()),
                default_value: U256::from(1019),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "WethPfwethPbrianBrian".to_string(),
                tokens: vec![
                    Address::from([0x23, 0xbd, 0x2f, 0xe4, 0x4c, 0xdb, 0xf6, 0x69, 0x5e, 0xa8, 0x9f, 0x08, 0x6b, 0xe1, 0x5f, 0xeb, 0x83, 0xe6, 0x9b, 0x7c]),
                    Address::from([0x5a, 0xfa, 0x72, 0x0d, 0x50, 0x09, 0x3a, 0x36, 0xe9, 0x4f, 0x53, 0x8a, 0xc0, 0xcb, 0x72, 0xff, 0xc4, 0xe3, 0x7c, 0x42]),
                    Address::from([0x3e, 0xcc, 0xed, 0x5b, 0x41, 0x6e, 0x58, 0x66, 0x4f, 0x04, 0xa3, 0x9d, 0xd1, 0x89, 0x35, 0xeb, 0x71, 0xd3, 0x3b, 0x15]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x1c8dDf0e958111eD1EC98dAeA2110ABCCB5a265F").unwrap()),
                default_value: U256::from(277),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "WethBrianPbrianPfweth".to_string(),
                tokens: vec![
                    Address::from([0x3e, 0xcc, 0xed, 0x5b, 0x41, 0x6e, 0x58, 0x66, 0x4f, 0x04, 0xa3, 0x9d, 0xd1, 0x89, 0x35, 0xeb, 0x71, 0xd3, 0x3b, 0x15]),
                    Address::from([0x5a, 0xfa, 0x72, 0x0d, 0x50, 0x09, 0x3a, 0x36, 0xe9, 0x4f, 0x53, 0x8a, 0xc0, 0xcb, 0x72, 0xff, 0xc4, 0xe3, 0x7c, 0x42]),
                    Address::from([0x23, 0xbd, 0x2f, 0xe4, 0x4c, 0xdb, 0xf6, 0x69, 0x5e, 0xa8, 0x9f, 0x08, 0x6b, 0xe1, 0x5f, 0xeb, 0x83, 0xe6, 0x9b, 0x7c]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x212d0CcF7368859Bf5e1A54B9E225874c591C4a6").unwrap()),
                default_value: U256::from(692),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcPfusdcPusdpPeasUsdcWeth".to_string(),
                tokens: vec![
                    Address::from([0x39, 0x24, 0x25, 0x17, 0xde, 0xa0, 0x58, 0x9b, 0x72, 0x94, 0xa5, 0xd8, 0xd1, 0x09, 0xfa, 0xbf, 0x6d, 0xe2, 0x2c, 0x41]),
                    Address::from([0x6b, 0x93, 0x80, 0x67, 0x6d, 0x2c, 0x53, 0x1c, 0xe6, 0xb1, 0x29, 0xc9, 0x1d, 0x4c, 0x9b, 0x7f, 0x76, 0xb2, 0xb2, 0x99]),
                    Address::from([0x02, 0xf9, 0x28, 0x00, 0xf5, 0x7b, 0xcd, 0x74, 0x06, 0x6f, 0x57, 0x09, 0xf1, 0xda, 0xa1, 0xa4, 0x30, 0x2d, 0xf8, 0x75]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xd01B9d8CD74F0E245158Dd5d6A8d5B3873505C83").unwrap()),
                default_value: U256::from(157),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "UsdcPfusdcPtibbirTibbirWeth".to_string(),
                tokens: vec![
                    Address::from([0xf0, 0xde, 0x99, 0x62, 0x92, 0xa1, 0x95, 0xdb, 0xb5, 0xfc, 0x94, 0xff, 0x18, 0x99, 0x78, 0x1c, 0x87, 0x4a, 0x97, 0x50]),
                    Address::from([0x2d, 0xad, 0x8b, 0x75, 0x1a, 0xd1, 0x5c, 0x41, 0x86, 0xea, 0x95, 0x5d, 0x6a, 0x47, 0xb7, 0x51, 0xc6, 0x68, 0x27, 0xd7]),
                    Address::from([0xa4, 0xa2, 0xe2, 0xca, 0x3f, 0xbf, 0xe2, 0x1a, 0xed, 0x83, 0x47, 0x1d, 0x28, 0xb6, 0xf6, 0x5a, 0x23, 0x3c, 0x6e, 0x00]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x5ecAD03BA53E092db9c98E038E005f8597645F84").unwrap()),
                default_value: U256::from(84),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "WethPfwethPpeasPeasUsdc".to_string(),
                tokens: vec![
                    Address::from([0x23, 0x57, 0x41, 0x54, 0x84, 0x2b, 0xc8, 0x6c, 0xf5, 0xbb, 0xc5, 0xf9, 0x93, 0x76, 0xcf, 0xa1, 0xe2, 0xf8, 0x24, 0x97]),
                    Address::from([0x9d, 0xbf, 0x56, 0x2a, 0x65, 0x59, 0x0c, 0x03, 0xe5, 0x40, 0x08, 0x54, 0xd4, 0x98, 0x84, 0x49, 0x15, 0xc2, 0x59, 0x44]),
                    Address::from([0x02, 0xf9, 0x28, 0x00, 0xf5, 0x7b, 0xcd, 0x74, 0x06, 0x6f, 0x57, 0x09, 0xf1, 0xda, 0xa1, 0xa4, 0x30, 0x2d, 0xf8, 0x75]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0x5817C0E2bDdDcdB3ca24f60Fe05efD74bdA0a0E6").unwrap()),
                default_value: U256::from(6),
                data_format: "short".to_string(),
                check_balance_of: None,
            },
            TokenPairProcessorConfig {
                name: "TybgPtybgPfwethWeth".to_string(),
                tokens: vec![
                    Address::from([0x0d, 0x97, 0xf2, 0x61, 0xb1, 0xe8, 0x88, 0x45, 0x18, 0x4f, 0x67, 0x8e, 0x2d, 0x1e, 0x7a, 0x98, 0xd9, 0xfd, 0x38, 0xde]),
                    Address::from([0xf1, 0x69, 0xd8, 0x1d, 0xd5, 0xc4, 0x82, 0xd6, 0x24, 0xbf, 0xd3, 0xa9, 0xe6, 0x4a, 0x5f, 0xcb, 0x11, 0xf2, 0xa1, 0x72]),
                    Address::from([0x37, 0x5f, 0xed, 0xbf, 0xd5, 0x1f, 0xd3, 0x61, 0x74, 0xf0, 0x7c, 0x7c, 0x67, 0x37, 0x26, 0x52, 0x20, 0x03, 0xc9, 0x67]),
                ],
                accounts: vec![],
                contract_address: Address::from(BackrunAnalyzer::address_to_bytes("0xCCBF08344B87ECE24D0563C932f1A5Ca3A2Cd79A").unwrap()),
                default_value: U256::from(1329),
                data_format: "short".to_string(),
                check_balance_of: None,
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