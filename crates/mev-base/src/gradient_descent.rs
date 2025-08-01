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

/// Parameters for gradient descent optimization
#[derive(Clone, Debug)]
pub struct GradientParams {
    pub initial_qty: U256,
    pub calldata_template: Bytes,
    pub seed: U256,
    pub lower_bound: U256,
    pub upper_bound: U256,
    pub target_address: Address,
}

/// Output from gradient descent optimization
#[derive(Clone, Debug)]
pub struct OptimizeOutput {
    pub qty_in: U256,
    pub delta: i128,  // Profit/loss in wei (signed)
    pub calldata_used: Bytes,
    pub gas_used: u64,
}

/// Gradient descent optimizer ported from Solidity
pub struct GradientOptimizer {
    /// Maximum iterations for optimization
    max_iterations: usize,
}

impl GradientOptimizer {
    pub fn new() -> Self {
        Self {
            max_iterations: 250,
        }
    }

    /// Optimize quantity using gradient descent algorithm
    /// This replicates the Solidity contract logic but runs in Rust with revm
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
        let mut best_output = OptimizeOutput {
            qty_in: params.initial_qty,
            delta: 0,
            calldata_used: params.calldata_template.clone(),
            gas_used: 0,
        };
        
        let mut iterations_used = 0;
        let mut hotspots: Vec<U256> = Vec::new();
        
        // Phase 1: Coarse grid search (40% of iterations)
        let range = params.upper_bound.saturating_sub(params.lower_bound) + U256::from(1);
        let grid_step = range / U256::from((self.max_iterations * 2) / 5);
        let grid_step = if grid_step.is_zero() { U256::from(1) } else { grid_step };
        
        // Grid search with randomized offset
        let grid_iterations = (self.max_iterations * 2) / 5;
        for i in 0..grid_iterations {
            if iterations_used >= self.max_iterations {
                break;
            }
            
            let random_offset = self.random(params.seed + U256::from(i)) % grid_step;
            let test_value = params.lower_bound + random_offset + (U256::from(i) * grid_step);
            
            if test_value > params.upper_bound {
                break;
            }
            
            iterations_used += 1;
            
            // Test this quantity
            let output = self.test_quantity(
                test_value,
                &params,
                cache_db,
                evm_config,
                state.base_fee,
                iterations_used,
            )?;
            
            if output.delta > 0 {
                // Found non-zero region
                if output.delta > best_output.delta && output.delta < i128::MAX / 2 {
                    best_output = output.clone();
                }
                
                // Store hotspot for later exploitation
                if hotspots.len() < 5 {
                    hotspots.push(test_value);
                }
            }
        }
        
        // Phase 2: Exploit hotspots (60% of remaining iterations)
        for hotspot in &hotspots {
            if iterations_used >= self.max_iterations {
                break;
            }
            
            let mut start = if *hotspot > grid_step * U256::from(2) {
                *hotspot - grid_step * U256::from(2)
            } else {
                params.lower_bound
            };
            
            let mut end = if *hotspot + grid_step * U256::from(2) < params.upper_bound {
                *hotspot + grid_step * U256::from(2)
            } else {
                params.upper_bound
            };
            
            // Binary search within hotspot region
            while end - start > U256::from(1) && iterations_used < self.max_iterations {
                let mid = (start + end) / U256::from(2);
                iterations_used += 1;
                
                let output = self.test_quantity(
                    mid,
                    &params,
                    cache_db,
                    evm_config,
                    state.base_fee,
                    iterations_used,
                )?;
                
                if output.delta > best_output.delta && output.delta < i128::MAX / 2 {
                    best_output = output.clone();
                    
                    // Focus on this region
                    start = if mid > U256::from(10) { mid - U256::from(10) } else { start };
                    end = if mid + U256::from(10) < end { mid + U256::from(10) } else { end };
                } else if output.delta > 0 {
                    // Randomly choose direction
                    if self.random(U256::from(iterations_used) + params.seed) % U256::from(2) == U256::ZERO {
                        end = mid;
                    } else {
                        start = mid + U256::from(1);
                    }
                } else {
                    // No value here, jump to a different part of the region
                    if iterations_used % 3 == 0 {
                        start = if *hotspot > grid_step { *hotspot - grid_step } else { start };
                        end = if *hotspot + grid_step < params.upper_bound {
                            *hotspot + grid_step
                        } else {
                            end
                        };
                    } else {
                        break; // Move to next hotspot
                    }
                }
            }
        }
        
        // Phase 3: Random exploration with remaining iterations
        while iterations_used < self.max_iterations {
            iterations_used += 1;
            
            let random_value = self.random(U256::from(iterations_used) + params.seed);
            let test_value = params.lower_bound + (random_value % (params.upper_bound - params.lower_bound + U256::from(1)));
            
            let output = self.test_quantity(
                test_value,
                &params,
                cache_db,
                evm_config,
                state.base_fee,
                iterations_used,
            )?;
            
            if output.delta > best_output.delta && output.delta < i128::MAX / 2 {
                best_output = output;
            }
        }
        
        println!("      📈 Gradient optimization complete:");
        println!("         - Iterations used: {}/{}", iterations_used, self.max_iterations);
        println!("         - Best quantity: {}", best_output.qty_in);
        println!("         - Best profit: {} wei", best_output.delta);
        println!("         - Hotspots found: {}", hotspots.len());
        
        Ok(best_output)
    }

    /// Test a specific quantity by simulating the transaction
    fn test_quantity<DB>(
        &self,
        qty_in: U256,
        params: &GradientParams,
        cache_db: &mut CacheDB<DB>,
        evm_config: &OpEvmConfig,
        _base_fee: u128,
        iterations_used: usize,
    ) -> eyre::Result<OptimizeOutput> 
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // Format calldata for short format (4 bytes)
        // First byte is 0x00 for the function selector, next 3 bytes are the quantity
        let qty_bytes = qty_in.to_be_bytes::<32>();
        let mut calldata = vec![0x00];
        // Take the last 3 bytes of the quantity (24 bits max)
        calldata.extend_from_slice(&qty_bytes[29..32]);
        
        // Use constant address from first 20 bytes of the provided hash
        // 0x3a3f76931108c79658a90f340b4cbec860346b2bd5ffe918ede99e74a7e821f1
        let bot_address = Address::from([
            0x3a, 0x3f, 0x76, 0x93, 0x11, 0x08, 0xc7, 0x96,
            0x58, 0xa9, 0x0f, 0x34, 0x0b, 0x4c, 0xbe, 0xc8,
            0x60, 0x34, 0x6b, 0x2b
        ]);
        
        // Fund the bot address to bypass fee validation issues (do this every iteration)
        let bot_account_info = AccountInfo {
            balance: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
            nonce: 0,
            code_hash: alloy_primitives::KECCAK256_EMPTY,
            code: None,
        };
        
        cache_db.cache.accounts.insert(bot_address, DbAccount {
            info: bot_account_info,
            account_state: AccountState::Touched,
            storage: Default::default(),
        });
        
        if qty_in == params.initial_qty {
            println!("      💰 Funding bot address {} with 1 ETH", bot_address);
        }
        
        let mut tx_env = TxEnv::default();
        tx_env.caller = bot_address;
        tx_env.nonce = 0; // Fresh address, nonce is 0
        tx_env.kind = TxKind::Call(params.target_address);
        tx_env.data = calldata.clone().into();
        tx_env.gas_limit = 4_000_000; // Same as Solidity contract
        tx_env.gas_price = 0; // Set gas price to 0 for MEV simulation
        tx_env.gas_priority_fee = None; // Don't set priority fee for legacy tx
        tx_env.value = U256::ZERO;
        
        // Clone the environment for EVM with custom settings for MEV simulation
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let mut evm_env = evm_config.evm_env(&alloy_consensus::Header {
            base_fee_per_gas: Some(0), // Set base fee to 0 for MEV simulation
            gas_limit: 2_000_000_000,   // 2 billion gas limit
            number: 33_634_688,         // Current Base mainnet block number
            timestamp: current_timestamp, // Today's timestamp
            ..Default::default()
        });
        
        // Override block gas limit and base fee in the environment
        evm_env.block_env.gas_limit = 2_000_000_000;
        evm_env.block_env.basefee = 0; // Ensure base fee is 0
        
        // Store values for logging before moving evm_env
        let block_gas_limit = evm_env.block_env.gas_limit;
        let block_basefee = evm_env.block_env.basefee;
        
        // Create EVM and execute
        let mut evm = evm_config.evm_with_env(&mut *cache_db, evm_env);
        
        // Debug: Log optimization attempt
        if qty_in == params.initial_qty {
            println!("      🔬 Gradient optimizer starting with qty {} on {}", qty_in, params.target_address);
            println!("      📊 Transaction details:");
            println!("         - From: {} (constant address)", bot_address);
            println!("         - To: {:?}", tx_env.kind);
            println!("         - Value: {}", tx_env.value);
            println!("         - Gas limit: {}", tx_env.gas_limit);
            println!("         - Gas price: {} wei", tx_env.gas_price);
            println!("         - Calldata: 0x{}", hex::encode(&calldata));
            println!("      🌐 Block environment:");
            println!("         - Block gas limit: {}", block_gas_limit);
            println!("         - Base fee: {} wei", block_basefee);
            println!("      💡 Note: Contract designed to revert with profit data");
        }
        
        // Only log first iteration or every 50th iteration
        if qty_in == params.initial_qty || iterations_used % 50 == 0 {
            println!("      🧪 Testing qty: {} (iteration {})", qty_in, iterations_used);
        }
        
        // Save gas limit before moving tx_env
        let gas_limit_for_logging = tx_env.gas_limit;
        
        // Debug: Log the fees we're setting
        if qty_in == params.initial_qty {
            println!("      🔍 Creating EIP-1559 tx with max_fee_per_gas: 0, max_priority_fee_per_gas: 0");
            println!("      🔍 Block basefee: {}", block_basefee);
        }
        
        // Create a proper transaction envelope for Optimism
        let tx_eip1559 = TxEip1559 {
            chain_id: 8453, // Base mainnet chain ID
            nonce: tx_env.nonce,
            gas_limit: tx_env.gas_limit,
            max_fee_per_gas: 0, // Ensure 0 fee
            max_priority_fee_per_gas: 0, // Ensure 0 priority fee
            to: tx_env.kind,
            value: tx_env.value,
            access_list: Default::default(),
            input: tx_env.data.clone(),
        };
        
        // Create a dummy signature for simulation (not actually sent on-chain)
        let signature = alloy_primitives::Signature::new(
            U256::from(1),
            U256::from(1), 
            false // y_parity
        );
        
        let signed_tx = Signed::new_unchecked(tx_eip1559, signature, Default::default());
        let tx_envelope = TxEnvelope::Eip1559(signed_tx);
        let enveloped_bytes = tx_envelope.encoded_2718();
        
        // Create OpTransaction with enveloped bytes
        let mut op_tx = op_revm::OpTransaction::new(tx_env.clone());
        op_tx.enveloped_tx = Some(enveloped_bytes.into());
        
        // Debug: Verify zero fees in transaction
        if qty_in == params.initial_qty {
            println!("      🔍 OpTransaction gas_price: {}", tx_env.gas_price);
            println!("      🔍 OpTransaction gas_priority_fee: {:?}", tx_env.gas_priority_fee);
        }
        
        // Execute the transaction
        let tx_start = std::time::Instant::now();
        let result = evm.transact(op_tx);
        let tx_time = tx_start.elapsed().as_secs_f64() * 1000.0;
        
        // Only log timing for first iteration or every 50th
        if qty_in == params.initial_qty || iterations_used % 50 == 0 {
            println!("      ⏱️  Transaction executed in {:.3}ms", tx_time);
        }
        
        match result {
            Ok(exec_result) => {
                let gas_used = exec_result.result.gas_used();
                
                // Only log success for first iteration
                if qty_in == params.initial_qty {
                    println!("      ✅ Transaction success, gas used: {}", gas_used);
                }
                
                match exec_result.result {
                    ExecutionResult::Success { output, logs, .. } => {
                        // Check if there were any logs emitted
                        if !logs.is_empty() {
                            println!("      📝 Logs emitted: {}", logs.len());
                            for (i, log) in logs.iter().enumerate() {
                                println!("         Log {}: address={}, data_len={}", i, log.address, log.data.data.len());
                            }
                        }
                        
                        // Check if contract exists and has code
                        let contract_info = cache_db.basic(params.target_address)?;
                        match contract_info {
                            Some(info) => {
                                if info.code_hash == alloy_primitives::KECCAK256_EMPTY {
                                    println!("      ❌ ERROR: Contract has no code! Address: {}", params.target_address);
                                    println!("      💡 This means the contract doesn't exist or was destroyed");
                                    return Ok(OptimizeOutput {
                                        qty_in,
                                        delta: 0,
                                        calldata_used: calldata.into(),
                                        gas_used,
                                    });
                                }
                            }
                            None => {
                                println!("      ❌ ERROR: Contract not found in state!");
                                return Ok(OptimizeOutput {
                                    qty_in,
                                    delta: 0,
                                    calldata_used: calldata.into(),
                                    gas_used,
                                });
                            }
                        }
                        
                        // Extract return value (delta)
                        let delta = match output {
                            Output::Call(bytes) => {
                                println!("      📤 Return data length: {} bytes", bytes.len());
                                if bytes.is_empty() {
                                    println!("      ⚠️  No return data - possible reasons:");
                                    println!("         1. Contract doesn't have a function with selector 0x00");
                                    println!("         2. Function exists but doesn't return anything");
                                    println!("         3. Function reverted without message");
                                    println!("      🔍 Debug: Calldata sent: 0x{}", hex::encode(&calldata));
                                    println!("      🔍 Debug: Gas used: {} (out of {})", gas_used, gas_limit_for_logging);
                                    0
                                } else if bytes.len() >= 32 {
                                    // Parse as U256 then convert to signed
                                    let delta_u256 = U256::from_be_bytes::<32>(bytes[0..32].try_into()?);
                                    println!("      💵 Raw return value: {} (0x{})", delta_u256, hex::encode(&bytes[0..32]));
                                    
                                    // Check for overflow indicator (very large value)
                                    if delta_u256 > U256::from(i128::MAX) {
                                        println!("      ⚠️  Return value overflow detected, treating as 0");
                                        0
                                    } else {
                                        let delta_i128: i128 = delta_u256.try_into().unwrap_or(0);
                                        println!("      💰 Profit/Loss: {} wei", delta_i128);
                                        delta_i128
                                    }
                                } else {
                                    println!("      ⚠️  Return data too short: {} bytes", bytes.len());
                                    0
                                }
                            }
                            _ => {
                                println!("      ⚠️  Unexpected output type");
                                0
                            }
                        };
                        
                        Ok(OptimizeOutput {
                            qty_in,
                            delta,
                            calldata_used: calldata.into(),
                            gas_used,
                        })
                    }
                    ExecutionResult::Revert { output, gas_used: revert_gas_used } => {
                        // Contract reverts with profit as uint256 (32 bytes)
                        let delta = if output.len() >= 32 {
                            let delta_u256 = U256::from_be_bytes::<32>(output[0..32].try_into()?);
                            
                            // Check for overflow indicator (very large value)
                            if delta_u256 > U256::from(i128::MAX) {
                                // This is likely a negative value represented as two's complement
                                let as_i256 = delta_u256.as_limbs();
                                if as_i256[3] & 0x8000_0000_0000_0000 != 0 {
                                    // Negative number in two's complement
                                    let neg = (!delta_u256).wrapping_add(U256::from(1));
                                    let neg_i128: i128 = neg.try_into().unwrap_or(i128::MIN);
                                    -neg_i128
                                } else {
                                    0
                                }
                            } else {
                                let delta_i128: i128 = delta_u256.try_into().unwrap_or(0);
                                
                                // Only log if profitable or first iteration
                                if delta_i128 > 0 || qty_in == params.initial_qty {
                                    println!("      💰 Profit/Loss found: {} wei for qty {}", delta_i128, qty_in);
                                }
                                
                                delta_i128
                            }
                        } else {
                            if qty_in == params.initial_qty {
                                println!("      ⚠️  Revert data too short: {} bytes", output.len());
                                if !output.is_empty() {
                                    println!("      🔍 Revert data: 0x{}", hex::encode(&output));
                                }
                            }
                            0
                        };
                        
                        Ok(OptimizeOutput {
                            qty_in,
                            delta,
                            calldata_used: calldata.into(),
                            gas_used: revert_gas_used,
                        })
                    }
                    ExecutionResult::Halt { reason, .. } => {
                        println!("      ❌ Transaction halted: {:?}", reason);
                        Ok(OptimizeOutput {
                            qty_in,
                            delta: 0,
                            calldata_used: calldata.into(),
                            gas_used,
                        })
                    }
                }
            }
            Err(e) => {
                if qty_in == params.initial_qty || format!("{:?}", e).contains("LackOfFundForMaxFee") {
                    println!("      ❌ EVM error: {:?}", e);
                    println!("      🔍 Debug info:");
                    println!("         - Bot address: {}", bot_address);
                    println!("         - Gas limit: {}", tx_env.gas_limit);
                    println!("         - Gas price: {}", tx_env.gas_price);
                    println!("         - Block basefee: {}", block_basefee);
                    
                    // Check account balance
                    if let Ok(Some(account)) = cache_db.basic(bot_address) {
                        println!("         - Account balance: {} wei", account.balance);
                        println!("         - Account nonce: {}", account.nonce);
                    } else {
                        println!("         - Account not found in CacheDB!");
                    }
                }
                Ok(OptimizeOutput {
                    qty_in,
                    delta: 0,
                    calldata_used: calldata.into(),
                    gas_used: 0,
                })
            }
        }
    }


    /// Simple random number generator (similar to Solidity's keccak256 based random)
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gradient_optimizer_creation() {
        let optimizer = GradientOptimizer::new();
        assert_eq!(optimizer.max_iterations, 250);
    }
    
    #[test]
    fn test_random_generation() {
        let optimizer = GradientOptimizer::new();
        let seed = U256::from(12345);
        let random1 = optimizer.random(seed);
        let random2 = optimizer.random(seed + U256::from(1));
        assert_ne!(random1, random2);
    }
}