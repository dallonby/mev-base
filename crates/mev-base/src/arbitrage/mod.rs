pub mod dex;
pub mod path_finder;
pub mod arbitrage_analyzer;
pub mod pool_discovery;
pub mod pool_fetcher;
pub mod dynamic_pricing;
pub mod atomic_executor;
pub mod mev_integration;

pub use dex::{DexProtocol, DexPool, SwapRoute};
pub use path_finder::{PathFinder, ArbitragePath};
pub use arbitrage_analyzer::ArbitrageAnalyzer;
pub use pool_discovery::{PoolDiscoveryStrategy, PoolMonitor, PoolInfo, TokenInfo};
pub use pool_fetcher::PoolFetcher;
pub use dynamic_pricing::{DynamicPricingEngine, TokenBehavior, HoneypotDetector};
pub use atomic_executor::AtomicArbitrageExecutor;
pub use mev_integration::{ArbitrageMevIntegration, run_arbitrage_worker};

use alloy_primitives::{Address, U256};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ArbitrageConfig {
    pub enabled_dexes: Vec<DexProtocol>,
    pub max_hops: u8,
    pub min_profit_threshold: U256,
    pub max_gas_price: U256,
    pub router_addresses: HashMap<DexProtocol, Address>,
    pub factory_addresses: HashMap<DexProtocol, Address>,
}

impl Default for ArbitrageConfig {
    fn default() -> Self {
        let mut router_addresses = HashMap::new();
        let mut factory_addresses = HashMap::new();
        
        // Base mainnet addresses (verified 2024)
        
        // UniswapV2
        router_addresses.insert(
            DexProtocol::UniswapV2,
            // 0x4752ba5dbc23f44d87826276bf6fd6b1c372ad24
            Address::from([0x47, 0x52, 0xba, 0x5d, 0xbc, 0x23, 0xf4, 0x4d, 0x87, 0x82, 0x62, 0x76, 0xbf, 0x6f, 0xd6, 0xb1, 0xc3, 0x72, 0xad, 0x24])
        );
        
        factory_addresses.insert(
            DexProtocol::UniswapV2,
            // 0x8909dc15e40173ff4699343b6eb8132c65e18ec6
            Address::from([0x89, 0x09, 0xdc, 0x15, 0xe4, 0x01, 0x73, 0xff, 0x46, 0x99, 0x34, 0x3b, 0x6e, 0xb8, 0x13, 0x2c, 0x65, 0xe1, 0x8e, 0xc6])
        );
        
        // UniswapV3
        router_addresses.insert(
            DexProtocol::UniswapV3,
            // 0x198ef79f1f515f02dfe9e3115ed9fc07183f02fc (Universal Router V2)
            Address::from([0x19, 0x8e, 0xf7, 0x9f, 0x1f, 0x51, 0x5f, 0x02, 0xdf, 0xe9, 0xe3, 0x11, 0x5e, 0xd9, 0xfc, 0x07, 0x18, 0x3f, 0x02, 0xfc])
        );
        
        factory_addresses.insert(
            DexProtocol::UniswapV3,
            // 0x33128a8fC17869897dcE68Ed026d694621f6FDfD
            Address::from([0x33, 0x12, 0x8a, 0x8f, 0xC1, 0x78, 0x69, 0x89, 0x7d, 0xcE, 0x68, 0xEd, 0x02, 0x6d, 0x69, 0x46, 0x21, 0xf6, 0xFD, 0xfD])
        );
        
        // Aerodrome
        router_addresses.insert(
            DexProtocol::Aerodrome,
            // 0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43
            Address::from([0xcf, 0x77, 0xa3, 0xba, 0x9a, 0x5c, 0xa3, 0x99, 0xb7, 0xc9, 0x7c, 0x74, 0xd5, 0x4e, 0x5b, 0x1b, 0xeb, 0x87, 0x4e, 0x43])
        );
        
        factory_addresses.insert(
            DexProtocol::Aerodrome,
            // 0x420DD381dA112a368EC4086Bb2E089AabcbBBF8F (PoolFactory)
            Address::from([0x42, 0x0D, 0xD3, 0x81, 0xdA, 0x11, 0x2a, 0x36, 0x8E, 0xC4, 0x08, 0x6B, 0xb2, 0xE0, 0x89, 0xAa, 0xbc, 0xbB, 0xBF, 0x8F])
        );
        
        Self {
            enabled_dexes: vec![
                DexProtocol::UniswapV2,
                DexProtocol::UniswapV3,
                DexProtocol::Aerodrome,
            ],
            max_hops: 3,
            min_profit_threshold: U256::from(10_000_000_000_000u64), // 0.00001 ETH
            max_gas_price: U256::from(1_000_000_000u64), // 1 gwei
            router_addresses,
            factory_addresses,
        }
    }
}