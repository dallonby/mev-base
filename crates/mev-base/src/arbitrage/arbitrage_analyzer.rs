use alloy_primitives::{Address, U256};
use reth_revm::db::CacheDB;
use revm::DatabaseRef;
use tracing::{debug, info};

use crate::arbitrage::{
    ArbitrageConfig, ArbitragePath, DexProtocol, PathFinder,
    dex::{DexPool, SwapRoute},
};
use crate::flashblock_state::FlashblockStateSnapshot;

pub struct ArbitrageAnalyzer {
    config: ArbitrageConfig,
    path_finder: PathFinder,
}

impl ArbitrageAnalyzer {
    pub fn new(config: ArbitrageConfig) -> Self {
        let path_finder = PathFinder::new(config.max_hops as usize);
        Self {
            config,
            path_finder,
        }
    }
    
    pub fn analyze_transaction<DB: DatabaseRef, T: alloy_consensus::Transaction>(
        &mut self,
        tx: &T,
        state: &FlashblockStateSnapshot,
        cache_db: &mut CacheDB<DB>,
    ) -> Vec<ArbitragePath> {
        // Extract tokens involved in the transaction
        let tokens = self.extract_tokens_from_tx(tx);
        if tokens.is_empty() {
            return Vec::new();
        }
        
        debug!(
            "Analyzing arbitrage for tx with {} tokens involved",
            tokens.len()
        );
        
        // Update pool states for relevant tokens
        self.update_pool_states(&tokens, cache_db);
        
        // Find arbitrage opportunities
        let mut all_paths = Vec::new();
        
        for token in &tokens {
            // Try different input amounts
            let test_amounts = vec![
                U256::from(1_000_000_000_000_000_000u128), // 1 ETH worth
                U256::from(5_000_000_000_000_000_000u128), // 5 ETH worth
                U256::from(10_000_000_000_000_000_000u128), // 10 ETH worth
            ];
            
            for amount in test_amounts {
                let paths = self.path_finder.find_arbitrage_paths(
                    *token,
                    amount,
                    U256::from(state.base_fee),
                );
                
                for path in paths {
                    if path.net_profit > self.config.min_profit_threshold {
                        info!(
                            "Found profitable arbitrage: {} -> profit: {} ETH, gas: {} ETH",
                            self.format_path(&path.route),
                            format_ether(path.profit),
                            format_ether(path.gas_cost)
                        );
                        all_paths.push(path);
                    }
                }
            }
        }
        
        // Return top opportunities
        all_paths.sort_by(|a, b| b.net_profit.cmp(&a.net_profit));
        all_paths.truncate(5);
        all_paths
    }
    
    fn extract_tokens_from_tx<T: alloy_consensus::Transaction>(&self, tx: &T) -> Vec<Address> {
        let mut tokens = Vec::new();
        
        // Extract from calldata (simplified - would need proper decoding)
        let data = tx.input();
        if data.len() >= 4 {
                // Check if it's a swap transaction
                let selector = &data[0..4];
                
                // Common swap selectors
                const SWAP_SELECTORS: &[[u8; 4]] = &[
                    [0x38, 0xed, 0x17, 0x39], // swapExactTokensForTokens
                    [0x7f, 0xf3, 0x6a, 0xb5], // swapExactETHForTokens
                    [0x18, 0xcb, 0xaf, 0xe5], // swapExactTokensForETH
                    [0x04, 0xe4, 0x5a, 0xaf], // exactInputSingle (V3)
                    [0x41, 0x4b, 0xf3, 0x89], // exactInput (V3)
                ];
                
                for swap_selector in SWAP_SELECTORS {
                    if selector == swap_selector {
                        // Extract token addresses from calldata
                        // This is simplified - would need proper ABI decoding
                        if data.len() >= 68 {
                            // Try to extract addresses at common positions
                            tokens.push(Address::from_slice(&data[16..36]));
                            tokens.push(Address::from_slice(&data[48..68]));
                        }
                        break;
                    }
                }
            }
        
        // Add WETH as it's commonly involved
        tokens.push(Address::from([
            0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x06
        ])); // WETH on Base
        
        tokens.dedup();
        tokens
    }
    
    fn update_pool_states<DB: DatabaseRef>(&mut self, tokens: &[Address], _cache_db: &mut CacheDB<DB>) {
        // This would fetch actual pool states from the blockchain
        // For now, adding placeholder pools
        
        // Common Base mainnet tokens
        let weth = Address::from([0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06]);
        let usdc = Address::from([0x83, 0x3e, 0x89, 0xa3, 0x4b, 0x4c, 0x64, 0x22, 0xfc, 0xb8, 0x88, 0x43, 0x46, 0xea, 0xb7, 0xe4, 0xfd, 0x3e, 0xf3, 0xbf]);
        
        // Add some example pools
        for token in tokens {
            if *token != weth {
                // Add WETH pair
                let pool = DexPool {
                    protocol: DexProtocol::UniswapV2,
                    address: Address::ZERO, // Would be calculated
                    token0: if *token < weth { *token } else { weth },
                    token1: if *token < weth { weth } else { *token },
                    reserve0: U256::from(1_000_000_000_000_000_000_000u128),
                    reserve1: U256::from(500_000_000_000_000_000_000u128),
                    fee: 30,
                    tick: None,
                    liquidity: None,
                };
                self.path_finder.add_pool(pool);
            }
            
            if *token != usdc && *token != weth {
                // Add USDC pair
                let pool = DexPool {
                    protocol: DexProtocol::UniswapV3,
                    address: Address::ZERO,
                    token0: if *token < usdc { *token } else { usdc },
                    token1: if *token < usdc { usdc } else { *token },
                    reserve0: U256::from(2_000_000_000_000u128),
                    reserve1: U256::from(1_000_000_000_000u128),
                    fee: 5,
                    tick: Some(0),
                    liquidity: Some(U256::from(1_000_000_000_000_000u128)),
                };
                self.path_finder.add_pool(pool);
            }
        }
    }
    
    fn format_path(&self, route: &SwapRoute) -> String {
        let mut path = String::new();
        for (i, token) in route.token_path.iter().enumerate() {
            if i > 0 {
                path.push_str(" -> ");
            }
            path.push_str(&format!("{:?}", &token.to_string()[0..8]));
        }
        path
    }
}

fn format_ether(wei: U256) -> String {
    let eth = wei.to_string();
    if eth.len() <= 18 {
        format!("0.{:0>18}", eth)
    } else {
        let (whole, decimal) = eth.split_at(eth.len() - 18);
        format!("{}.{}", whole, decimal)
    }
}