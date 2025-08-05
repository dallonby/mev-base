use alloy_primitives::{Address, U256, Bytes, TxKind};
use alloy_sol_types::{sol, SolCall, SolValue};
use revm::{
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
    Database,
    database::{DbAccount, AccountState},
    state::AccountInfo,
    bytecode::Bytecode,
};
use reth_revm::db::CacheDB;
use reth_optimism_evm::OpEvmConfig;
use reth_evm::{ConfigureEvm, Evm};
use crate::flashblock_state::FlashblockStateSnapshot;
use alloy_consensus::{TxEip1559, TxEnvelope, Signed};
use alloy_eips::eip2718::Encodable2718;
use tracing::{debug, trace};

// Re-export types from the main gradient descent module
pub use crate::gradient_descent::{GradientParams, OptimizeOutput};

// Define the multicall interface
sol! {
    struct TestResult {
        bool success;
        bytes returnData;
        uint256 gasUsed;
    }
    
    function batchTest(
        address target,
        uint256[] calldata quantities
    ) external returns (TestResult[] memory results);
}

/// Fixed address for the BatchGradientTest contract
pub const BATCH_TEST_ADDRESS: Address = Address::new([
    0xba, 0x7c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01
]);

/// Inject BatchGradientTest contract using code override
pub fn inject_batch_test_contract<DB>(cache_db: &mut CacheDB<DB>) 
where
    DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
{
    // BatchGradientTest runtime bytecode (optimized with --optimize --optimizer-runs 200)
    const BATCH_TEST_RUNTIME_BYTECODE: &str = "608060405234801561000f575f5ffd5b5060043610610029575f3560e01c8063092055671461002d575b5f5ffd5b61004061003b3660046101ee565b610056565b60405161004d919061027c565b60405180910390f35b6060818067ffffffffffffffff81111561007257610072610320565b6040519080825280602002602001820160405280156100c757816020015b6100b460405180606001604052805f15158152602001606081526020015f81525090565b8152602001906001900390816100905790505b5091505f5b818110156101e5575f8585838181106100e7576100e7610334565b6040515f602080830182905292909202939093013560e881901b6001600160e81b0319166021850152935091602401905060405160208183030381529060405290505f5a90505f5f8a6001600160a01b0316846040516101479190610348565b5f604051808303815f865af19150503d805f8114610180576040519150601f19603f3d011682016040523d82523d5f602084013e610185565b606091505b50915091505f5a610196908561035e565b905060405180606001604052808415158152602001838152602001828152508988815181106101c7576101c7610334565b602002602001018190525050505050505080806001019150506100cc565b50509392505050565b5f5f5f60408486031215610200575f5ffd5b83356001600160a01b0381168114610216575f5ffd5b9250602084013567ffffffffffffffff811115610231575f5ffd5b8401601f81018613610241575f5ffd5b803567ffffffffffffffff811115610257575f5ffd5b8660208260051b840101111561026b575f5ffd5b939660209190910195509293505050565b5f602082016020835280845180835260408501915060408160051b8601019250602086015f5b8281101561031457603f1987860301845281518051151586526020810151606060208801528051806060890152806020830160808a015e5f6080828a010152604083015160408901526080601f19601f83011689010197505050506020820191506020840193506001810190506102a2565b50929695505050505050565b634e487b7160e01b5f52604160045260245ffd5b634e487b7160e01b5f52603260045260245ffd5b5f82518060208501845e5f920191825250919050565b8181038181111561037d57634e487b7160e01b5f52601160045260245ffd5b9291505056fea26469706673582212202a52e5c0860ef1985ef89e7ac5914e9c3308ca66b97097ea3f10918135ab91a064736f6c634300081e0033";
    
    let bytecode = hex::decode(BATCH_TEST_RUNTIME_BYTECODE).expect("Invalid bytecode");
    
    // Create account info for the contract
    let contract_info = AccountInfo {
        balance: U256::ZERO,
        nonce: 1, // Non-zero to indicate deployed contract
        code_hash: alloy_primitives::keccak256(&bytecode),
        code: Some(Bytecode::new_raw(bytecode.into())),
    };
    
    // Insert the contract at the fixed address
    cache_db.cache.accounts.insert(BATCH_TEST_ADDRESS, DbAccount {
        info: contract_info,
        account_state: AccountState::Touched,
        storage: Default::default(),
    });
    
    trace!(address = %BATCH_TEST_ADDRESS, "BatchGradientTest contract injected via code override");
}

/// Multicall gradient descent optimizer with batched execution
pub struct MulticallGradientOptimizer {
    /// Maximum iterations for optimization
    max_iterations: usize,
    /// Batch size for multicall
    batch_size: usize,
}

impl MulticallGradientOptimizer {
    pub fn new() -> Self {
        Self {
            max_iterations: 50,
            batch_size: 20, // Test 20 quantities in a single EVM call
        }
    }

    /// Optimize quantity using multicall gradient descent
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
        
        // Inject the batch test contract if not already present
        if !cache_db.cache.accounts.contains_key(&BATCH_TEST_ADDRESS) {
            inject_batch_test_contract(cache_db);
        }
        
        let mut best_output = OptimizeOutput {
            qty_in: params.initial_qty,
            delta: 0,
            calldata_used: params.calldata_template.clone(),
            gas_used: 0,
            filtered_gas: None,
            actual_multiplier: None,
        };
        
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
        
        trace!(iterations = self.max_iterations, batch_size = self.batch_size, "Multicall gradient optimizer starting");
        
        // Create reusable EVM environment
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let evm_env = evm_config.evm_env(&alloy_consensus::Header {
            base_fee_per_gas: Some(0),
            gas_limit: 2_000_000_000,
            number: state.block_number,
            timestamp: current_timestamp,
            ..Default::default()
        });
        
        // Create dummy signature once
        let signature = alloy_primitives::Signature::new(
            U256::from(1),
            U256::from(1), 
            false
        );
        
        // Phase 1: Coarse grid search with batched tests
        let range = params.upper_bound.saturating_sub(params.lower_bound);
        let initial_step = range / U256::from(20); // 20 initial points
        
        let mut test_values = Vec::new();
        for i in 0..20 {
            let test_value = params.lower_bound + (U256::from(i) * initial_step);
            if test_value <= params.upper_bound {
                test_values.push(test_value);
            }
        }
        
        // Execute batch test
        let batch_results = self.execute_batch_test(
            &test_values,
            &params,
            cache_db,
            evm_config,
            &evm_env,
            bot_address,
            &signature,
        )?;
        
        // Process results and find promising regions
        let mut promising_regions = Vec::new();
        for (i, (qty, result)) in test_values.iter().zip(batch_results.iter()).enumerate() {
            if result.delta > 0 {
                if result.delta > best_output.delta {
                    best_output = result.clone();
                    trace!(qty = %qty, profit_wei = result.delta, "Profit found in batch");
                }
                promising_regions.push((*qty, result.delta));
            }
        }
        
        // Phase 2: Binary search around promising regions
        if !promising_regions.is_empty() {
            promising_regions.sort_by_key(|(_, delta)| -delta);
            let best_region = promising_regions[0].0;
            
            // Binary search with batching
            let mut search_radius = initial_step;
            let mut center = best_region;
            
            // Do 2-3 rounds of binary search
            for round in 0..3 {
                if search_radius < U256::from(1) {
                    break;
                }
                
                // Create batch of test points for binary search
                let mut binary_tests = Vec::new();
                
                // Test center and points at various distances
                binary_tests.push(center);
                
                // Add points at different radii for parallel binary search
                for i in 1..=5 {
                    let distance = (search_radius * U256::from(i)) / U256::from(5);
                    
                    if center > distance && center - distance >= params.lower_bound {
                        binary_tests.push(center - distance);
                    }
                    
                    if center + distance <= params.upper_bound {
                        binary_tests.push(center + distance);
                    }
                }
                
                if binary_tests.len() <= 1 {
                    break;
                }
                
                // Execute batch
                let binary_results = self.execute_batch_test(
                    &binary_tests,
                    &params,
                    cache_db,
                    evm_config,
                    &evm_env,
                    bot_address,
                    &signature,
                )?;
                
                // Find best result and update center
                let mut best_in_batch = None;
                for (qty, result) in binary_tests.iter().zip(binary_results.iter()) {
                    if result.delta > best_output.delta {
                        best_output = result.clone();
                        best_in_batch = Some(*qty);
                        trace!(qty = %qty, profit_wei = result.delta, round, "Better profit in binary search");
                    }
                }
                
                // Update center and reduce radius
                if let Some(new_center) = best_in_batch {
                    center = new_center;
                }
                search_radius = search_radius / U256::from(3);
            }
        }
        
        let total_time = start_time.elapsed().as_secs_f64() * 1000.0;
        
        // Count total tests performed
        let total_tests = if promising_regions.is_empty() {
            test_values.len()
        } else {
            test_values.len() + 11 * 3 // initial + up to 11 per round * 3 rounds
        };
        
        trace!(
            time_ms = total_time,
            evm_calls = "1-4", // 1 initial + up to 3 binary search rounds
            total_tests = total_tests,
            best_qty = %best_output.qty_in,
            best_profit_wei = best_output.delta,
            "Multicall optimization complete"
        );
        
        Ok(best_output)
    }

    /// Execute a batch of tests using multicall
    fn execute_batch_test<DB>(
        &self,
        quantities: &[U256],
        params: &GradientParams,
        cache_db: &mut CacheDB<DB>,
        evm_config: &OpEvmConfig,
        evm_env: &reth_evm::EvmEnv<op_revm::OpSpecId>,
        bot_address: Address,
        signature: &alloy_primitives::Signature,
    ) -> eyre::Result<Vec<OptimizeOutput>>
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        let batch_start = std::time::Instant::now();
        
        // Encode the multicall
        let call = batchTestCall {
            target: params.target_address,
            quantities: quantities.to_vec(),
        };
        let calldata = call.abi_encode();
        
        // Setup transaction
        let mut tx_env = TxEnv::default();
        tx_env.caller = bot_address;
        tx_env.nonce = 0;
        tx_env.kind = TxKind::Call(BATCH_TEST_ADDRESS);
        tx_env.data = calldata.clone().into();
        tx_env.gas_limit = 20_000_000; // Higher limit for batch
        tx_env.gas_price = 0;
        tx_env.gas_priority_fee = None;
        tx_env.value = U256::ZERO;
        
        // Create transaction for Optimism
        let tx_eip1559 = TxEip1559 {
            chain_id: 8453,
            nonce: 0,
            gas_limit: 20_000_000,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(BATCH_TEST_ADDRESS),
            value: U256::ZERO,
            access_list: Default::default(),
            input: calldata.into(),
        };
        
        let signed_tx = Signed::new_unchecked(tx_eip1559, signature.clone(), Default::default());
        let tx_envelope = TxEnvelope::Eip1559(signed_tx);
        let enveloped_bytes = tx_envelope.encoded_2718();
        
        let mut op_tx = op_revm::OpTransaction::new(tx_env);
        op_tx.enveloped_tx = Some(enveloped_bytes.into());
        
        // Clone environment
        let local_env = evm_env.clone();
        
        // Create and execute EVM
        let mut evm = evm_config.evm_with_env(&mut *cache_db, local_env);
        
        trace!(
            quantities = quantities.len(),
            target = %params.target_address,
            "Executing batch test"
        );
        
        match evm.transact(op_tx) {
            Ok(exec_result) => {
                let batch_time = batch_start.elapsed().as_secs_f64() * 1000.0;
                
                match exec_result.result {
                    ExecutionResult::Success { output, gas_used, .. } => {
                        // Decode the results
                        let results = match output {
                            Output::Call(bytes) => {
                                self.decode_batch_results(&bytes, quantities, params)?
                            }
                            _ => {
                                debug!("Unexpected output type from batch test");
                                // Return zeros for all
                                quantities.iter().map(|&qty| OptimizeOutput {
                                    qty_in: qty,
                                    delta: 0,
                                    calldata_used: self.create_calldata(qty),
                                    gas_used: 0,
                                    filtered_gas: None,
                                    actual_multiplier: None,
                                }).collect()
                            }
                        };
                        
                        trace!(
                            batch_size = quantities.len(),
                            time_ms = batch_time,
                            gas_used = gas_used,
                            "Batch test completed"
                        );
                        
                        Ok(results)
                    }
                    ExecutionResult::Revert { output, .. } => {
                        debug!(data = ?output, "Batch test reverted");
                        // Return zeros for all on revert
                        Ok(quantities.iter().map(|&qty| OptimizeOutput {
                            qty_in: qty,
                            delta: 0,
                            calldata_used: self.create_calldata(qty),
                            gas_used: 0,
                            filtered_gas: None,
                            actual_multiplier: None,
                        }).collect())
                    }
                    _ => {
                        debug!("Batch test halted");
                        Ok(quantities.iter().map(|&qty| OptimizeOutput {
                            qty_in: qty,
                            delta: 0,
                            calldata_used: self.create_calldata(qty),
                            gas_used: 0,
                            filtered_gas: None,
                            actual_multiplier: None,
                        }).collect())
                    }
                }
            }
            Err(e) => {
                debug!(error = ?e, "Batch test transaction failed");
                Ok(quantities.iter().map(|&qty| OptimizeOutput {
                    qty_in: qty,
                    delta: 0,
                    calldata_used: self.create_calldata(qty),
                    gas_used: 0,
                    filtered_gas: None,
                    actual_multiplier: None,
                }).collect())
            }
        }
    }

    /// Decode batch test results
    fn decode_batch_results(
        &self,
        output: &Bytes,
        quantities: &[U256],
        params: &GradientParams,
    ) -> eyre::Result<Vec<OptimizeOutput>> {
        // Decode TestResult[] from output
        let decoded = <Vec<TestResult>>::abi_decode(output)?;
        
        let mut results = Vec::new();
        
        for (i, (qty, test_result)) in quantities.iter().zip(decoded.iter()).enumerate() {
            let delta = if test_result.success {
                // Contract succeeded - no profit
                0i128
            } else if test_result.returnData.len() >= 32 {
                // Extract profit from revert data
                let delta_u256 = U256::from_be_bytes::<32>(test_result.returnData[0..32].try_into()?);
                
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
            
            results.push(OptimizeOutput {
                qty_in: *qty,
                delta,
                calldata_used: self.create_calldata(*qty),
                gas_used: test_result.gasUsed.try_into().unwrap_or(0),
                filtered_gas: None,
                actual_multiplier: None,
            });
        }
        
        Ok(results)
    }

    /// Create calldata for a single test
    fn create_calldata(&self, qty: U256) -> Bytes {
        let qty_bytes = qty.to_be_bytes::<32>();
        let calldata = [&[0x00], &qty_bytes[29..32]].concat();
        calldata.into()
    }
}