use alloy_primitives::{Address, U256, Bytes, TxKind};
use revm::{
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
    Database,
    database::{DbAccount, AccountState},
    state::AccountInfo,
};
use reth_revm::db::CacheDB;
use reth_optimism_evm::OpEvmConfig;
use reth_evm::{ConfigureEvm, Evm};
use crate::flashblock_state::FlashblockStateSnapshot;
use alloy_consensus::{TxEip1559, TxEnvelope, Signed};
use alloy_eips::eip2718::Encodable2718;
use rayon::prelude::*;
use std::sync::{Arc, Mutex};

// Re-export types from the main gradient descent module
pub use crate::gradient_descent::{GradientParams, OptimizeOutput};

/// Test case for parallel execution
#[derive(Clone)]
struct TestCase {
    qty: U256,
    iteration: usize,
}

/// Parallel gradient descent optimizer
pub struct ParallelGradientOptimizer {
    /// Maximum iterations for optimization
    max_iterations: usize,
    /// Number of parallel workers
    num_workers: usize,
}

impl ParallelGradientOptimizer {
    pub fn new() -> Self {
        Self {
            max_iterations: 250,
            num_workers: rayon::current_num_threads(),
        }
    }

    /// Optimize quantity using parallel gradient descent
    pub fn optimize_quantity<DB>(
        &self,
        params: GradientParams,
        state: &FlashblockStateSnapshot,
        cache_db: &CacheDB<DB>,
        evm_config: &OpEvmConfig,
    ) -> eyre::Result<OptimizeOutput> 
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug + Clone + Send + Sync,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        let start_time = std::time::Instant::now();
        
        // Thread-safe best output
        let best_output = Arc::new(Mutex::new(OptimizeOutput {
            qty_in: params.initial_qty,
            delta: 0,
            calldata_used: params.calldata_template.clone(),
            gas_used: 0,
            filtered_gas: None,
        }));
        
        let hotspots = Arc::new(Mutex::new(Vec::<U256>::new()));
        
        // Phase 1: Parallel coarse grid search (40% of iterations)
        let range = params.upper_bound.saturating_sub(params.lower_bound) + U256::from(1);
        let grid_iterations = (self.max_iterations * 2) / 5;
        let grid_step = range / U256::from(grid_iterations);
        let grid_step = if grid_step.is_zero() { U256::from(1) } else { grid_step };
        
        println!("      🚀 Starting parallel gradient optimization with {} workers", self.num_workers);
        println!("      📊 Grid search: {} iterations, step size: {}", grid_iterations, grid_step);
        
        // Prepare test cases for parallel execution
        let mut test_cases = Vec::new();
        for i in 0..grid_iterations {
            let random_offset = self.random(params.seed + U256::from(i)) % grid_step;
            let test_value = params.lower_bound + random_offset + (U256::from(i) * grid_step);
            
            if test_value <= params.upper_bound {
                test_cases.push(TestCase {
                    qty: test_value,
                    iteration: i + 1,
                });
            }
        }
        
        // Execute grid search in parallel batches
        let batch_size = 8; // Process 8 simulations at a time
        let chunks: Vec<_> = test_cases.chunks(batch_size).collect();
        
        println!("      🔄 Processing {} batches of {} simulations each", chunks.len(), batch_size);
        
        for (batch_idx, chunk) in chunks.iter().enumerate() {
            let batch_start = std::time::Instant::now();
            
            // Process batch in parallel
            let results: Vec<_> = chunk.par_iter()
                .map(|test_case| {
                    // Clone CacheDB for this thread
                    let mut local_cache_db = cache_db.clone();
                    
                    // Test the quantity
                    self.test_quantity_fast(
                        test_case.qty,
                        &params,
                        &mut local_cache_db,
                        evm_config,
                        state.base_fee,
                        test_case.iteration,
                        batch_idx == 0 && test_case.iteration == 1, // Only log first
                    )
                })
                .collect();
            
            // Update best output and collect hotspots
            for result in results {
                if let Ok(output) = result {
                    if output.delta > 0 {
                        let mut best = best_output.lock().unwrap();
                        if output.delta > best.delta && output.delta < i128::MAX / 2 {
                            *best = output.clone();
                            println!("      💰 New best found: qty={}, profit={} wei", output.qty_in, output.delta);
                        }
                        drop(best);
                        
                        // Store hotspot
                        let mut spots = hotspots.lock().unwrap();
                        if spots.len() < 5 {
                            spots.push(output.qty_in);
                        }
                    }
                }
            }
            
            if batch_idx % 5 == 0 {
                println!("      ⏱️  Batch {}/{} completed in {:.1}ms", 
                    batch_idx + 1, chunks.len(), batch_start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        
        // Phase 2: Exploit hotspots (serial for now, as they depend on each other)
        let spots = hotspots.lock().unwrap().clone();
        if !spots.is_empty() {
            println!("      🎯 Exploiting {} hotspots", spots.len());
            
            for hotspot in spots {
                let mut local_cache_db = cache_db.clone();
                
                // Quick binary search around hotspot
                let mut start = if hotspot > grid_step * U256::from(2) {
                    hotspot - grid_step * U256::from(2)
                } else {
                    params.lower_bound
                };
                
                let mut end = if hotspot + grid_step * U256::from(2) < params.upper_bound {
                    hotspot + grid_step * U256::from(2)
                } else {
                    params.upper_bound
                };
                
                // Just do 5 iterations per hotspot for speed
                for _ in 0..5 {
                    if end - start <= U256::from(1) {
                        break;
                    }
                    
                    let mid = (start + end) / U256::from(2);
                    
                    let output = self.test_quantity_fast(
                        mid,
                        &params,
                        &mut local_cache_db,
                        evm_config,
                        state.base_fee,
                        0,
                        false,
                    )?;
                    
                    let mut best = best_output.lock().unwrap();
                    if output.delta > best.delta && output.delta < i128::MAX / 2 {
                        *best = output.clone();
                        println!("      💰 Hotspot improvement: qty={}, profit={} wei", output.qty_in, output.delta);
                    }
                    drop(best);
                    
                    if output.delta > 0 {
                        // Focus on this region
                        start = if mid > U256::from(10) { mid - U256::from(10) } else { start };
                        end = if mid + U256::from(10) < end { mid + U256::from(10) } else { end };
                    } else {
                        break;
                    }
                }
            }
        }
        
        let total_time = start_time.elapsed().as_secs_f64() * 1000.0;
        let final_output = best_output.lock().unwrap().clone();
        
        println!("      📈 Parallel optimization complete in {:.1}ms", total_time);
        println!("         - Best quantity: {}", final_output.qty_in);
        println!("         - Best profit: {} wei", final_output.delta);
        println!("         - Speedup: {:.1}x", 900.0 / total_time);
        
        Ok(final_output)
    }

    /// Fast version of test_quantity with minimal overhead
    fn test_quantity_fast<DB>(
        &self,
        qty_in: U256,
        params: &GradientParams,
        cache_db: &mut CacheDB<DB>,
        evm_config: &OpEvmConfig,
        _base_fee: u128,
        _iteration: usize,
        should_log: bool,
    ) -> eyre::Result<OptimizeOutput> 
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // Format calldata efficiently
        let qty_bytes = qty_in.to_be_bytes::<32>();
        let mut calldata = vec![0x00];
        calldata.extend_from_slice(&qty_bytes[29..32]);
        
        // Use constant bot address
        let bot_address = Address::from([
            0x3a, 0x3f, 0x76, 0x93, 0x11, 0x08, 0xc7, 0x96,
            0x58, 0xa9, 0x0f, 0x34, 0x0b, 0x4c, 0xbe, 0xc8,
            0x60, 0x34, 0x6b, 0x2b
        ]);
        
        // Fund the bot address efficiently (only if not already funded)
        if !cache_db.cache.accounts.contains_key(&bot_address) {
            let bot_account_info = AccountInfo {
                balance: U256::from(1_000_000_000_000_000_000u64),
                nonce: 0,
                code_hash: alloy_primitives::KECCAK256_EMPTY,
                code: None,
            };
            
            cache_db.cache.accounts.insert(bot_address, DbAccount {
                info: bot_account_info,
                account_state: AccountState::Touched,
                storage: Default::default(),
            });
        }
        
        // Create minimal transaction
        let mut tx_env = TxEnv::default();
        tx_env.caller = bot_address;
        tx_env.nonce = 0;
        tx_env.kind = TxKind::Call(params.target_address);
        tx_env.data = calldata.clone().into();
        tx_env.gas_limit = 4_000_000;
        tx_env.gas_price = 0;
        tx_env.value = U256::ZERO;
        
        // Create EVM environment once
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let mut evm_env = evm_config.evm_env(&alloy_consensus::Header {
            base_fee_per_gas: Some(0),
            gas_limit: 2_000_000_000,
            number: 33_634_688,
            timestamp: current_timestamp,
            ..Default::default()
        });
        
        evm_env.block_env.gas_limit = 2_000_000_000;
        evm_env.block_env.basefee = 0;
        
        // Create EVM
        let mut evm = evm_config.evm_with_env(&mut *cache_db, evm_env);
        
        if should_log {
            println!("      🔬 Starting parallel gradient optimizer on {}", params.target_address);
        }
        
        // Create minimal transaction for Optimism
        let tx_eip1559 = TxEip1559 {
            chain_id: 8453,
            nonce: tx_env.nonce,
            gas_limit: tx_env.gas_limit,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: tx_env.kind,
            value: tx_env.value,
            access_list: Default::default(),
            input: tx_env.data.clone(),
        };
        
        let signature = alloy_primitives::Signature::new(
            U256::from(1),
            U256::from(1), 
            false
        );
        
        let signed_tx = Signed::new_unchecked(tx_eip1559, signature, Default::default());
        let tx_envelope = TxEnvelope::Eip1559(signed_tx);
        let enveloped_bytes = tx_envelope.encoded_2718();
        
        let mut op_tx = op_revm::OpTransaction::new(tx_env);
        op_tx.enveloped_tx = Some(enveloped_bytes.into());
        
        // Execute transaction
        let result = evm.transact(op_tx);
        
        match result {
            Ok(exec_result) => {
                let gas_used = exec_result.result.gas_used();
                
                match exec_result.result {
                    ExecutionResult::Success { .. } => {
                        // Contract succeeded but we expect revert with profit data
                        Ok(OptimizeOutput {
                            qty_in,
                            delta: 0,
                            calldata_used: calldata.into(),
                            gas_used,
                            filtered_gas: None,
                        })
                    }
                    ExecutionResult::Revert { output, gas_used: revert_gas_used } => {
                        // Extract profit from revert data
                        let delta = if output.len() >= 32 {
                            let delta_u256 = U256::from_be_bytes::<32>(output[0..32].try_into()?);
                            
                            if delta_u256 > U256::from(i128::MAX) {
                                // Handle two's complement negative
                                let as_i256 = delta_u256.as_limbs();
                                if as_i256[3] & 0x8000_0000_0000_0000 != 0 {
                                    let neg = (!delta_u256).wrapping_add(U256::from(1));
                                    let neg_i128: i128 = neg.try_into().unwrap_or(i128::MIN);
                                    -neg_i128
                                } else {
                                    0
                                }
                            } else {
                                delta_u256.try_into().unwrap_or(0)
                            }
                        } else {
                            0
                        };
                        
                        Ok(OptimizeOutput {
                            qty_in,
                            delta,
                            calldata_used: calldata.into(),
                            gas_used: revert_gas_used,
                            filtered_gas: None,
                        })
                    }
                    ExecutionResult::Halt { .. } => {
                        Ok(OptimizeOutput {
                            qty_in,
                            delta: 0,
                            calldata_used: calldata.into(),
                            gas_used,
                            filtered_gas: None,
                        })
                    }
                }
            }
            Err(_) => {
                Ok(OptimizeOutput {
                    qty_in,
                    delta: 0,
                    calldata_used: calldata.into(),
                    gas_used: 0,
            filtered_gas: None,
                })
            }
        }
    }

    /// Simple random number generator
    fn random(&self, seed: U256) -> U256 {
        use sha3::{Keccak256, Digest};
        
        let mut hasher = Keccak256::new();
        hasher.update(seed.to_be_bytes::<32>());
        hasher.update(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_be_bytes());
        
        let result = hasher.finalize();
        U256::from_be_bytes(result.into())
    }
}