use alloy_primitives::{Address, U256, TxKind};
use revm::{
    context::TxEnv,
    context_interface::result::ExecutionResult,
    database::{DbAccount, AccountState},
    state::AccountInfo,
};
use reth_revm::db::CacheDB;
use reth_optimism_evm::OpEvmConfig;
use reth_evm::{ConfigureEvm, Evm};
use crate::flashblock_state::FlashblockStateSnapshot;
use alloy_consensus::{TxEip1559, TxEnvelope, Signed};
use alloy_eips::eip2718::Encodable2718;
use tracing::trace;

// Re-export types from the main gradient descent module
pub use crate::gradient_descent::{GradientParams, OptimizeOutput};

/// Fast gradient descent optimizer with reduced iterations and optimized execution
pub struct FastGradientOptimizer {
    /// Maximum iterations for optimization
    max_iterations: usize,
}

impl FastGradientOptimizer {
    pub fn new() -> Self {
        Self {
            max_iterations: 50, // Reduced from 250 for 5x speedup
        }
    }

    /// Optimize quantity using fast gradient descent
    pub fn optimize_quantity<DB>(
        &self,
        params: GradientParams,
        state: &FlashblockStateSnapshot,
        cache_db: &mut CacheDB<DB>,
        evm_config: &OpEvmConfig,
    ) -> eyre::Result<OptimizeOutput> 
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        let start_time = std::time::Instant::now();
        
        let mut best_output = OptimizeOutput {
            qty_in: params.initial_qty,
            delta: 0,
            calldata_used: params.calldata_template.clone(),
            gas_used: 0,
            filtered_gas: None,
            actual_multiplier: None,
        };
        
        let mut iterations_used = 0;
        
        // Pre-fund bot address once
        let bot_address = Address::from([
            0x3a, 0x3f, 0x76, 0x93, 0x11, 0x08, 0xc7, 0x96,
            0x58, 0xa9, 0x0f, 0x34, 0x0b, 0x4c, 0xbe, 0xc8,
            0x60, 0x34, 0x6b, 0x2b
        ]);
        
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
        
        trace!(iterations = self.max_iterations, "Fast gradient optimizer starting");
        
        // Create reusable EVM environment
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let evm_env = evm_config.evm_env(&alloy_consensus::Header {
            base_fee_per_gas: Some(0),
            gas_limit: 2_000_000_000,
            number: 33_634_688,
            timestamp: current_timestamp,
            ..Default::default()
        });
        
        // Create dummy signature once
        let signature = alloy_primitives::Signature::new(
            U256::from(1),
            U256::from(1), 
            false
        );
        
        // Phase 1: Adaptive search with larger steps
        let range = params.upper_bound.saturating_sub(params.lower_bound);
        let initial_step = range / U256::from(10); // Only 10 initial points
        
        // First pass: Coarse search
        let coarse_points = 10;
        let mut promising_regions = Vec::new();
        
        for i in 0..coarse_points {
            if iterations_used >= self.max_iterations {
                break;
            }
            
            let test_value = params.lower_bound + (U256::from(i) * initial_step);
            if test_value > params.upper_bound {
                break;
            }
            
            iterations_used += 1;
            
            let output = self.test_quantity_ultra_fast(
                test_value,
                &params,
                cache_db,
                evm_config,
                &evm_env,
                bot_address,
                &signature,
                iterations_used == 1,
            )?;
            
            if output.delta > 0 {
                if output.delta > best_output.delta {
                    best_output = output.clone();
                    trace!(qty = %test_value, profit_wei = output.delta, "Profit found");
                }
                promising_regions.push((test_value, output.delta));
            }
        }
        
        // Phase 2: Focus on the most promising region
        if !promising_regions.is_empty() {
            // Sort by profit and take top 2
            promising_regions.sort_by_key(|(_, delta)| -delta);
            promising_regions.truncate(2);
            
            for (center, _) in promising_regions {
                if iterations_used >= self.max_iterations {
                    break;
                }
                
                // Binary search around the center
                let search_radius = initial_step / U256::from(2);
                let mut left = if center > search_radius {
                    center - search_radius
                } else {
                    params.lower_bound
                };
                let mut right = if center + search_radius < params.upper_bound {
                    center + search_radius
                } else {
                    params.upper_bound
                };
                
                // Do 5 binary search iterations
                for _ in 0..5 {
                    if iterations_used >= self.max_iterations || right <= left {
                        break;
                    }
                    
                    let mid = (left + right) / U256::from(2);
                    iterations_used += 1;
                    
                    let output = self.test_quantity_ultra_fast(
                        mid,
                        &params,
                        cache_db,
                        evm_config,
                        &evm_env,
                        bot_address,
                        &signature,
                        false,
                    )?;
                    
                    if output.delta > best_output.delta {
                        best_output = output.clone();
                        trace!(qty = %mid, profit_wei = output.delta, "Better profit found");
                        
                        // Narrow search around this point
                        let new_radius = (right - left) / U256::from(4);
                        left = if mid > new_radius { mid - new_radius } else { left };
                        right = if mid + new_radius < right { mid + new_radius } else { right };
                    } else if output.delta > 0 {
                        // Still profitable, keep searching
                        if mid > center {
                            left = mid;
                        } else {
                            right = mid;
                        }
                    } else {
                        // Not profitable, try other side
                        if mid > center {
                            right = mid;
                        } else {
                            left = mid;
                        }
                    }
                }
            }
        }
        
        // Phase 3: Quick random sampling of remaining budget
        let remaining = self.max_iterations.saturating_sub(iterations_used);
        for i in 0..remaining.min(10) {
            iterations_used += 1;
            
            let random_value = self.fast_random(U256::from(iterations_used) + params.seed);
            let test_value = params.lower_bound + (random_value % (params.upper_bound - params.lower_bound + U256::from(1)));
            
            let output = self.test_quantity_ultra_fast(
                test_value,
                &params,
                cache_db,
                evm_config,
                &evm_env,
                bot_address,
                &signature,
                false,
            )?;
            
            if output.delta > best_output.delta {
                best_output = output;
            }
        }
        
        let total_time = start_time.elapsed().as_secs_f64() * 1000.0;
        
        trace!(
            time_ms = total_time,
            iterations = iterations_used,
            max_iterations = self.max_iterations,
            best_qty = %best_output.qty_in,
            best_profit_wei = best_output.delta,
            speedup = (900.0 / total_time.max(0.1)),
            "Fast optimization complete"
        );
        
        Ok(best_output)
    }

    /// Ultra-fast test quantity with pre-created objects
    fn test_quantity_ultra_fast<DB>(
        &self,
        qty_in: U256,
        params: &GradientParams,
        cache_db: &mut CacheDB<DB>,
        evm_config: &OpEvmConfig,
        evm_env: &reth_evm::EvmEnv<op_revm::OpSpecId>,
        bot_address: Address,
        signature: &alloy_primitives::Signature,
        should_log: bool,
    ) -> eyre::Result<OptimizeOutput> 
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // Ultra-fast calldata creation
        let qty_bytes = qty_in.to_be_bytes::<32>();
        let calldata = [&[0x00], &qty_bytes[29..32]].concat();
        
        // Minimal transaction setup
        let mut tx_env = TxEnv::default();
        tx_env.caller = bot_address;
        tx_env.nonce = 0;
        tx_env.kind = TxKind::Call(params.target_address);
        tx_env.data = calldata.clone().into();
        tx_env.gas_limit = 4_000_000;
        tx_env.gas_price = 0;
        tx_env.gas_priority_fee = None;
        tx_env.value = U256::ZERO;
        
        if should_log {
            trace!(qty = %qty_in, target = %params.target_address, "Testing initial quantity");
        }
        
        // Create minimal transaction for Optimism
        let tx_eip1559 = TxEip1559 {
            chain_id: 8453,
            nonce: 0,
            gas_limit: 4_000_000,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(params.target_address),
            value: U256::ZERO,
            access_list: Default::default(),
            input: calldata.clone().into(),
        };
        
        let signed_tx = Signed::new_unchecked(tx_eip1559, signature.clone(), Default::default());
        let tx_envelope = TxEnvelope::Eip1559(signed_tx);
        let enveloped_bytes = tx_envelope.encoded_2718();
        
        let mut op_tx = op_revm::OpTransaction::new(tx_env);
        op_tx.enveloped_tx = Some(enveloped_bytes.into());
        
        // Clone environment for this execution
        let local_env = evm_env.clone();
        
        // Create EVM with cloned environment
        let mut evm = evm_config.evm_with_env(&mut *cache_db, local_env);
        
        // Execute
        match evm.transact(op_tx) {
            Ok(exec_result) => {
                match exec_result.result {
                    ExecutionResult::Revert { output, gas_used } => {
                        // Fast profit extraction
                        let delta = if output.len() >= 32 {
                            let delta_u256 = U256::from_be_bytes::<32>(output[0..32].try_into()?);
                            
                            if delta_u256 > U256::from(i128::MAX) {
                                // Two's complement handling
                                let as_i256 = delta_u256.as_limbs();
                                if as_i256[3] & 0x8000_0000_0000_0000 != 0 {
                                    let neg = (!delta_u256).wrapping_add(U256::from(1));
                                    -(neg.try_into().unwrap_or(i128::MAX))
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
                            gas_used,
                            filtered_gas: None,
                            actual_multiplier: None,
                        })
                    }
                    _ => Ok(OptimizeOutput {
                        qty_in,
                        delta: 0,
                        calldata_used: calldata.into(),
                        gas_used: exec_result.result.gas_used(),
                        filtered_gas: None,
                        actual_multiplier: None,
                    })
                }
            }
            Err(_) => Ok(OptimizeOutput {
                qty_in,
                delta: 0,
                calldata_used: calldata.into(),
                gas_used: 0,
                filtered_gas: None,
                actual_multiplier: None,
            })
        }
    }

    /// Fast random using simple xorshift
    fn fast_random(&self, seed: U256) -> U256 {
        let mut x = seed.as_limbs()[0];
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        U256::from(x)
    }
}