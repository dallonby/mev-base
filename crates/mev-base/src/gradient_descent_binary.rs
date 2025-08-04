use alloy_primitives::{Address, U256, Bytes, TxKind};
use alloy_sol_types::{sol, SolCall, SolValue};
use revm::{
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
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
use tracing::{debug, trace, warn};

// Re-export types from the main gradient descent module
pub use crate::gradient_descent::{GradientParams, OptimizeOutput};

// Define the binary search interface
sol! {
    struct BinarySearchResult {
        uint256 bestQuantity;
        int256 bestProfit;
        uint256 testsPerformed;
    }
    
    function binarySearch(
        address target,
        uint256 lowerBound,
        uint256 upperBound,
        uint256 maxIterations,
        uint256 initialValue
    ) external returns (BinarySearchResult memory result);
}

/// Fixed address for the BatchGradientTestV4 contract
pub const BATCH_TEST_V4_ADDRESS: Address = Address::new([
    0xba, 0x7c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x04  // Note: ending with 04 for V4
]);

/// Inject BatchGradientTestV4 contract using code override
pub fn inject_batch_test_v4_contract<DB>(cache_db: &mut CacheDB<DB>) 
where
    DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
{
    // BatchGradientTestV4 runtime bytecode (optimized)
    const BATCH_TEST_V4_RUNTIME_BYTECODE: &str = "60806040526004361015610011575f80fd5b5f3560e01c806309205567146100345763409c47181461002f575f80fd5b61015c565b346100ae5760403660031901126100ae5761004d6100b2565b60243567ffffffffffffffff81116100ae57366023820112156100ae57806004013567ffffffffffffffff81116100ae573660248260051b840101116100ae576100aa92602461009e930190610359565b604051918291826100c8565b0390f35b5f80fd5b600435906001600160a01b03821682036100ae57565b602081016020825282518091526040820191602060408360051b8301019401925f915b8383106100fa57505050505090565b90919293946020806080600193603f198682030187528260408b51805115158452828101516060848601528051938491826060880152018686015e5f84840186015201516040830152601f01601f1916010197019594919091019201906100eb565b346100ae5760a03660031901126100ae57606061018d61017a6100b2565b602435604435606435916084359361054d565b6040805191805183526020810151602084015201516040820152f35b634e487b7160e01b5f52604160045260245ffd5b6060810190811067ffffffffffffffff8211176101d957604052565b6101a9565b90601f8019910116810190811067ffffffffffffffff8211176101d957604052565b6040519061020f6060836101de565b565b67ffffffffffffffff81116101d95760051b60200190565b9061023382610211565b61024060405191826101de565b8281528092610251601f1991610211565b01905f5b82811061026157505050565b602090604051610270816101bd565b5f81526060838201525f604082015282828501015201610255565b634e487b7160e01b5f52603260045260245ffd5b91908110156102af5760051b0190565b61028b565b3d156102ee573d9067ffffffffffffffff82116101d957604051916102e3601f8201601f1916602001846101de565b82523d5f602084013e565b606090565b634e487b7160e01b5f52601160045260245ffd5b60181981019190821161031657565b6102f3565b60091981019190821161031657565b5f1981019190821161031657565b9190820391821161031657565b80518210156102af5760209160051b010190565b61036283610229565b925f5b818110610373575050505090565b806103a0610390610387600194868961029f565b3562ffffff1690565b60e81b6001600160e81b03191690565b6040515f602082018181526001600160e81b031993909316602183015260048252909181906103d06024856101de565b5a93519082895af1906103ec6103e46102b4565b915a90610338565b906103ff6103f8610200565b9315158452565b602083015260408201526104138288610345565b5261041e8187610345565b5001610365565b60405190610432826101bd565b5f6040838281528260208201520152565b906103e88202918083046103e8149015171561031657565b908160011b918083046002149015171561031657565b90620182b8820291808304620182b8149015171561031657565b90610384820291808304610384149015171561031657565b90605a820291808304605a149015171561031657565b8181029291811591840414171561031657565b5f1981146103165760010190565b6103e80190816103e81161031657565b606401908160641161031657565b600a019081600a1161031657565b906001820180921161031657565b906002820180921161031657565b9190820180921161031657565b8115610539570690565b634e487b7160e01b5f52601260045260245ffd5b94929394610559610425565b9282845260208401925f845261057088600a900490565b958661057b8a610443565b91838910610aef575b848311610ae7575b61059f6105988b61045b565b6005900490565b996105b36105ac8c61045b565b6003900490565b60288111610adf575b5f5b81811080610ad2575b156106e857610602908e816106555750855b8d811061064e575b878111610647575b8c8b8a8310158061063d575b610607575b5050506104cc565b6105be565b61061383604092610af8565b910161061f81516104cc565b90528c518113610632575b508c8b6105fa565b8c528c525f8061062a565b508b8311156105f5565b50866105e9565b508c6105e1565b600a8210156106925761068661068d9161068061067b610674866104a3565b600a900490565b6104f8565b906104b9565b6064900490565b6105d9565b60198210156106c55761068661068d916106806106c06106b96106b48761031b565b61048b565b600f900490565b6104ea565b61068661068d916106806106e36106b96106de87610307565b610471565b6104da565b50919a909394999b506106fb9250610338565b9760148911610ac9575b6040805142602082019081524492820192909252606087811b6bffffffffffffffffffffffff19169082015261074881607481015b03601f1981018352826101de565b519020915f925b8a841080610abc575b156107fa5760408051602081019283529081018590526107b69190610780816060810161073a565b519020936107a18d61079b610795828a610338565b8861052f565b90610522565b8b811015806107f0575b6107bc575b506104cc565b9261074f565b6107c6818a610af8565b60408c016107d481516104cc565b90528a5181136107e5575b506107b0565b8a528a525f806107df565b50878111156107ab565b509497999850959490505115610ab157835160011c968785518181115f14610aaa576108269250610338565b9681610833828751610522565b1015610aa45761084591508451610522565b9460408401918251965b84881080610a9b575b15610a8e5761087061086a828b610522565b60011c90565b9061087b8285610af8565b8b61088687516104cc565b80885260208a019283518113610a83575b50885f9286119182610a79575b5050610a44575b5f85851080610a3a575b610a05575b808213156108de57505050506108d26108d89161032a565b976104cc565b9661084f565b9a9b929a13156108fc5750506108f66108d891610506565b986104cc565b61091461090e838c9a9d9b9e9c610338565b60021c90565b91821515806109f3575b610931575b505050505050505050505090565b61093b838c610522565b84116109a6575b5061094d8285610338565b831061095a575b80610923565b61096d6109678385610522565b86610af8565b9061097887516104cc565b8752805182136109885750610954565b529799969895976108d8919061099e9082610522565b8752986104cc565b6109b96109b38486610338565b87610af8565b6109c388516104cc565b8852825181136109d35750610942565b6108d89493919c99506109eb929d9a9b9d5282610338565b8752976104cc565b50876109ff8851610514565b1061091e565b50610a126109b385610506565b610a1c88516104cc565b885282518113156108ba57808352610a3385610506565b8a526108ba565b50888851106108b5565b50610a516109678461032a565b610a5b87516104cc565b875281518113156108ab57808252610a728461032a565b89526108ab565b109050885f6108a4565b8352848a525f610897565b5050505050925092505090565b50808910610858565b50610845565b5050610826565b505050925092505090565b508160408a015110610758565b60149850610705565b508260408c0151106105c7565b5060286105bc565b84925061058c565b97508297610584565b6040515f6020820181815260e89490941b6001600160e81b0319166021830152928392918390610b2b816024810161073a565b5192623d0900f1610b3a6102b4565b9015610b4557505f90565b8051602011610b55576020015190565b505f9056fea2646970667358221220bd98ccfd3104b4dca8146cc731825ca4f6c22c7e841d67c1761ccb7ab87081ce64736f6c634300081e0033";
    
    let bytecode = hex::decode(BATCH_TEST_V4_RUNTIME_BYTECODE).expect("Invalid bytecode");
    
    // Create account info for the contract
    let contract_info = AccountInfo {
        balance: U256::ZERO,
        nonce: 1, // Non-zero to indicate deployed contract
        code_hash: alloy_primitives::keccak256(&bytecode),
        code: Some(Bytecode::new_raw(bytecode.into())),
    };
    
    // Insert the contract at the fixed address
    cache_db.cache.accounts.insert(BATCH_TEST_V4_ADDRESS, DbAccount {
        info: contract_info,
        account_state: AccountState::Touched,
        storage: Default::default(),
    });
    
    trace!(address = %BATCH_TEST_V4_ADDRESS, "BatchGradientTestV4 contract injected via code override");
}

/// Binary search gradient descent optimizer with single EVM call
pub struct BinarySearchGradientOptimizer {
    /// Maximum iterations for the binary search
    max_iterations: usize,
}

impl BinarySearchGradientOptimizer {
    pub fn new() -> Self {
        Self {
            max_iterations: 40, // Further reduced for faster execution
        }
    }
    
    /// Adjust bounds based on filtered gas usage
    fn adjust_bounds_for_gas(&self, mut params: GradientParams) -> GradientParams {
        const TARGET_GAS: u64 = 35_000_000; // Target 35M gas
        
        if let Some(filtered_gas) = params.filtered_gas {
            // Calculate adjustment factor based on gas usage
            let adjustment = if filtered_gas > TARGET_GAS * 2 {
                0.5 // Reduce to 50% if using way too much gas
            } else if filtered_gas > TARGET_GAS {
                0.8 // Reduce to 80% if slightly over
            } else if filtered_gas < TARGET_GAS / 2 {
                1.5 // Increase to 150% if plenty of headroom
            } else {
                1.0 // Keep as is
            };
            
            // Calculate new upper bound
            let current_multiplier = params.upper_bound / params.initial_qty;
            let new_multiplier = U256::from((current_multiplier.to::<u64>() as f64 * adjustment) as u64);
            let new_multiplier = new_multiplier.max(U256::from(10)).min(U256::from(1000));
            let new_upper = params.initial_qty * new_multiplier;
            
            if new_upper != params.upper_bound {
                debug!(
                    target = %params.target_address,
                    old_upper = %params.upper_bound,
                    new_upper = %new_upper,
                    filtered_gas_millions = filtered_gas / 1_000_000,
                    adjustment = adjustment,
                    "Adjusting upper bound based on gas usage"
                );
                params.upper_bound = new_upper;
            }
        }
        
        params
    }

    /// Optimize quantity using in-contract binary search
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
        // Adjust bounds based on filtered gas history
        let params = self.adjust_bounds_for_gas(params);
        let start_time = std::time::Instant::now();
        
        debug!(
            target = %params.target_address,
            lower = %params.lower_bound,
            upper = %params.upper_bound,
            "BinarySearchGradientOptimizer::optimize_quantity called"
        );
        
        // Inject the batch test V4 contract if not already present
        if !cache_db.cache.accounts.contains_key(&BATCH_TEST_V4_ADDRESS) {
            debug!("Injecting BatchGradientTestV4 contract");
            inject_batch_test_v4_contract(cache_db);
        }
        
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
        
        trace!(
            iterations = self.max_iterations,
            lower = %params.lower_bound,
            upper = %params.upper_bound,
            "Binary search gradient optimizer starting"
        );
        
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
        
        // Encode the binary search call
        let call = binarySearchCall {
            target: params.target_address,
            lowerBound: params.lower_bound,
            upperBound: params.upper_bound,
            maxIterations: U256::from(self.max_iterations),
            initialValue: params.initial_qty,
        };
        let calldata = call.abi_encode();
        
        // Setup transaction
        let mut tx_env = TxEnv::default();
        tx_env.caller = bot_address;
        tx_env.nonce = 0;
        tx_env.kind = TxKind::Call(BATCH_TEST_V4_ADDRESS);
        tx_env.data = calldata.clone().into();
        tx_env.gas_limit = 1_000_000_000; // 1 billion gas limit
        tx_env.gas_price = 0;
        tx_env.gas_priority_fee = None;
        tx_env.value = U256::ZERO;
        
        // Create transaction for Optimism
        let tx_eip1559 = TxEip1559 {
            chain_id: 8453,
            nonce: 0,
            gas_limit: 1_000_000_000,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(BATCH_TEST_V4_ADDRESS),
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
        
        debug!(
            target = %params.target_address,
            contract = %BATCH_TEST_V4_ADDRESS,
            "Executing binary search EVM transaction"
        );
        
        match evm.transact(op_tx) {
            Ok(exec_result) => {
                let total_time = start_time.elapsed().as_secs_f64() * 1000.0;
                debug!("Binary search EVM transaction executed successfully");
                
                match exec_result.result {
                    ExecutionResult::Success { output, gas_used, .. } => {
                        debug!(gas_used = gas_used, "Binary search execution success");
                        
                        // Log high gas consumption
                        if gas_used > 50_000_000 {
                            warn!(
                                target = %params.target_address,
                                gas_used = gas_used,
                                gas_used_millions = gas_used / 1_000_000,
                                initial_qty = %params.initial_qty,
                                upper_bound = %params.upper_bound,
                                upper_multiplier = %(params.upper_bound / params.initial_qty),
                                "High gas consumption in V4 optimizer"
                            );
                        }
                        // Decode the result
                        let result = match output {
                            Output::Call(bytes) => {
                                let decoded = BinarySearchResult::abi_decode(&bytes)?;
                                
                                // Convert signed 256 to i128
                                let best_profit: i128 = if decoded.bestProfit.is_negative() {
                                    // Handle negative values
                                    let abs_val = decoded.bestProfit.unsigned_abs();
                                    -(abs_val.try_into().unwrap_or(i128::MAX))
                                } else {
                                    decoded.bestProfit.try_into().unwrap_or(i128::MAX)
                                };
                                
                                debug!(
                                    time_ms = total_time,
                                    best_qty = %decoded.bestQuantity,
                                    best_profit = best_profit,
                                    tests_performed = %decoded.testsPerformed,
                                    gas_used = gas_used,
                                    "Binary search internal results"
                                );
                                
                                // Calculate new filtered gas value using IIR filter
                                const ALPHA: f64 = 0.05; // IIR filter coefficient (5% new, 95% old)
                                let new_filtered_gas = match params.filtered_gas {
                                    Some(old_filtered) => {
                                        // IIR filter: new_value = alpha * current + (1 - alpha) * old
                                        ((gas_used as f64 * ALPHA) + (old_filtered as f64 * (1.0 - ALPHA))) as u64
                                    }
                                    None => {
                                        // First run, use current gas as initial value
                                        gas_used
                                    }
                                };
                                
                                // Create calldata for the best quantity
                                let qty_bytes = decoded.bestQuantity.to_be_bytes::<32>();
                                let calldata = [&[0x00], &qty_bytes[29..32]].concat();
                                
                                OptimizeOutput {
                                    qty_in: decoded.bestQuantity,
                                    delta: best_profit,
                                    calldata_used: calldata.into(),
                                    gas_used: 200_000, // Estimate for actual swap
                                    filtered_gas: Some(new_filtered_gas),
                                }
                            }
                            _ => {
                                debug!("Unexpected output type from binary search");
                                OptimizeOutput {
                                    qty_in: params.initial_qty,
                                    delta: 0,
                                    calldata_used: params.calldata_template.clone(),
                                    gas_used: 0,
                                    filtered_gas: params.filtered_gas,
                                }
                            }
                        };
                        
                        Ok(result)
                    }
                    ExecutionResult::Revert { output, .. } => {
                        warn!(
                            data = ?output,
                            data_hex = ?hex::encode(&output),
                            target = %params.target_address,
                            "Binary search contract reverted"
                        );
                        Ok(OptimizeOutput {
                            qty_in: params.initial_qty,
                            delta: 0,
                            calldata_used: params.calldata_template.clone(),
                            gas_used: 0,
                            filtered_gas: params.filtered_gas,
                        })
                    }
                    ExecutionResult::Halt { reason, gas_used } => {
                        debug!(
                            halt_reason = ?reason,
                            gas_used = gas_used,
                            "Binary search halted"
                        );
                        Ok(OptimizeOutput {
                            qty_in: params.initial_qty,
                            delta: 0,
                            calldata_used: params.calldata_template.clone(),
                            gas_used: 0,
                            filtered_gas: params.filtered_gas,
                        })
                    }
                }
            }
            Err(e) => {
                warn!(
                    error = ?e,
                    target = %params.target_address,
                    "Binary search transaction failed"
                );
                Ok(OptimizeOutput {
                    qty_in: params.initial_qty,
                    delta: 0,
                    calldata_used: params.calldata_template.clone(),
                    gas_used: 0,
                    filtered_gas: params.filtered_gas,
                })
            }
        }
    }
}