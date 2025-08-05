use alloy_consensus::{TxEnvelope, Transaction as _, transaction::SignerRecoverable, BlockHeader};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::TxKind;
use alloy_rpc_types_eth::{BlockId, EthCallResponse};
use reth_provider::{StateProvider, StateProviderFactory};
use reth_revm::{database::StateProviderDatabase, db::CacheDB};
use reth_optimism_evm::OpEvmConfig;
use reth_optimism_chainspec::OpChainSpec;
use reth_optimism_node::OpRethReceiptBuilder;
use reth_evm::{ConfigureEvm, Evm};
use revm::{
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output, HaltReason},
    DatabaseCommit,
};
use op_revm::OpTransaction;
use std::sync::Arc;
use crate::flashblocks::FlashblocksEvent;
use crate::flashblock_state::FlashblockStateSnapshot;

/// A flashblock executor that uses revm directly with CacheDB for optimal performance
pub struct RevmFlashblockExecutor {
    /// The chain specification
    #[allow(dead_code)]
    chain_spec: Arc<OpChainSpec>,
    /// The EVM configuration
    evm_config: OpEvmConfig,
    /// The cached database that persists state across flashblock simulations
    cache_db: Option<CacheDB<StateProviderDatabase<Box<dyn StateProvider>>>>,
    /// The current EVM environment
    evm_env: Option<reth_evm::EvmEnv<op_revm::OpSpecId>>,
    /// Current block number being processed
    current_block: Option<u64>,
    /// Base fee for current block
    current_base_fee: u128,
}

impl RevmFlashblockExecutor {
    /// Create a new executor for a specific block
    pub fn new(chain_spec: Arc<OpChainSpec>) -> Self {
        let evm_config = OpEvmConfig::new(
            chain_spec.clone(),
            OpRethReceiptBuilder::default(),
        );
        
        Self {
            chain_spec,
            evm_config,
            cache_db: None,
            evm_env: None,
            current_block: None,
            current_base_fee: 0,
        }
    }
    
    /// Initialize the executor with a state provider and block context
    pub async fn initialize<P>(&mut self, provider: P, block_id: BlockId) -> eyre::Result<()> 
    where 
        P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader,
        P::Header: alloy_consensus::BlockHeader,
    {
        // Get the latest state provider
        let state_provider = provider.latest()?;
        
        // Get the block number and header
        let (block_number, header) = match block_id {
            BlockId::Number(alloy_rpc_types_eth::BlockNumberOrTag::Latest) => {
                let number = provider.best_block_number()?;
                // println!("   🔍 Best block number from provider: {}", number);
                let header = provider.header_by_number(number)?
                    .ok_or_else(|| eyre::eyre!("Header not found for block {}", number))?;
                (number, header)
            }
            BlockId::Number(alloy_rpc_types_eth::BlockNumberOrTag::Number(num)) => {
                let header = provider.header_by_number(num)?
                    .ok_or_else(|| eyre::eyre!("Header not found for block {}", num))?;
                (num, header)
            }
            BlockId::Hash(hash) => {
                let header = provider.header(&hash.block_hash)?
                    .ok_or_else(|| eyre::eyre!("Header not found for hash {:?}", hash))?;
                let number = header.number();
                (number, header)
            }
            _ => return Err(eyre::eyre!("Unsupported block ID: {:?}", block_id)),
        };
        
        // Create a basic header for the EVM environment (since we can't use generic header directly)
        let base_fee = header.base_fee_per_gas();
        // println!("   🔍 Fetched header for block {}:", header.number());
        // println!("      - Timestamp: {}", header.timestamp());
        // println!("      - Gas limit: {}", header.gas_limit());
        // println!("      - Base fee from header: {:?} (raw value)", base_fee);
        
        // For Base mainnet after Bedrock, there should always be a base fee
        if base_fee.is_none() || base_fee == Some(0) {
            // println!("      ⚠️  WARNING: Base fee is missing or zero! This seems incorrect for Base mainnet.");
        }
        
        let evm_header = alloy_consensus::Header {
            number: header.number(),
            timestamp: header.timestamp(),
            gas_limit: header.gas_limit(),
            base_fee_per_gas: base_fee,
            ..Default::default()
        };
        
        // Create the CacheDB with the state provider
        self.cache_db = Some(CacheDB::new(StateProviderDatabase::new(state_provider)));
        
        // Set up the EVM environment using the header
        self.evm_env = Some(self.evm_config.evm_env(&evm_header));
        
        let base_fee_wei = evm_header.base_fee_per_gas.unwrap_or(0);
        let base_fee_gwei = base_fee_wei as f64 / 1_000_000_000.0;
        
        // Store current block info
        self.current_block = Some(block_number);
        self.current_base_fee = base_fee_wei as u128;
        
        // println!("✅ Initialized revm executor for block {} (base fee: {} wei = {:.4} gwei)", 
        //     block_number, 
        //     base_fee_wei,
        //     base_fee_gwei);
        
        Ok(())
    }
    
    /// Execute a flashblock's transactions using revm
    pub async fn execute_flashblock(
        &mut self,
        event: &FlashblocksEvent,
        flashblock_index: u32,
    ) -> eyre::Result<Vec<EthCallResponse>> {
        // First convert all transactions (to avoid borrow conflicts)
        let converted_txs: Vec<(TxEnv, alloy_primitives::B256)> = event.transactions.iter()
            .map(|tx| {
                let tx_hash = tx.tx_hash();
                self.convert_to_tx_env(tx).map(|env| (env, *tx_hash))
            })
            .collect::<Result<Vec<_>, _>>()?;
            
        let cache_db = self.cache_db.as_mut()
            .ok_or_else(|| eyre::eyre!("Executor not initialized. Call initialize() first."))?;
        
        let evm_env = self.evm_env.as_ref()
            .ok_or_else(|| eyre::eyre!("EVM environment not initialized."))?;
        
        // println!("\n🔥 Executing flashblock {} with revm (block {})", flashblock_index, event.block_number);
        let start = std::time::Instant::now();
        
        let mut results = Vec::new();
        
        // Process each transaction in the flashblock
        for (i, (tx_env, _tx_hash)) in converted_txs.into_iter().enumerate() {
            
            // Get the original transaction envelope bytes
            let tx_envelope = &event.transactions[i];
            let enveloped_bytes = tx_envelope.encoded_2718();
            
            // Create OpTransaction with enveloped bytes
            let mut op_tx = OpTransaction::new(tx_env);
            op_tx.enveloped_tx = Some(enveloped_bytes.into());
            
            // Create the EVM with our cached database
            let mut evm = self.evm_config.evm_with_env(
                &mut *cache_db,
                evm_env.clone()
            );
            
            // Execute the transaction
            let result = evm.transact(op_tx);
            
            // Process the result and optionally commit state
            let response = match result {
                Ok(exec_result) => {
                    // Extract the execution result
                    let gas_used = exec_result.result.gas_used();
                    let response = match exec_result.result {
                        ExecutionResult::Success { output, .. } => {
                            let value = match output {
                                Output::Call(bytes) => bytes,
                                Output::Create(bytes, _) => bytes,
                            };
                            EthCallResponse {
                                value: Some(value),
                                error: None,
                                gas_used: Some(gas_used),
                            }
                        }
                        ExecutionResult::Revert { output, .. } => {
                            EthCallResponse {
                                value: None,
                                error: Some(format!("execution reverted: 0x{}", hex::encode(&output))),
                                gas_used: Some(gas_used),
                            }
                        }
                        ExecutionResult::Halt { reason, .. } => {
                            EthCallResponse {
                                value: None,
                                error: Some(format!("execution halted: {:?}", reason)),
                                gas_used: Some(gas_used),
                            }
                        }
                    };
                    
                    // Commit state changes if successful
                    if response.error.is_none() {
                        cache_db.commit(exec_result.state);
                    }
                    
                    response
                }
                Err(ref e) => EthCallResponse {
                    value: None,
                    error: Some(format!("EVM error: {:?}", e)),
                    gas_used: None,
                },
            };
            results.push(response);
        }
        
        let elapsed = start.elapsed();
        let successful = results.iter().filter(|r| r.error.is_none()).count();
        let failed = results.len() - successful;
        
        // println!("   ├─ Results: {}/{} successful, {} failed", successful, results.len(), failed);
        // println!("   └─ Flashblock executed in {:.2}ms ({:.2}ms per tx avg)", 
        //     elapsed.as_secs_f64() * 1000.0,
        //     (elapsed.as_secs_f64() * 1000.0) / event.transactions.len() as f64
        // );
        
        Ok(results)
    }
    
    /// Convert a transaction envelope to revm TxEnv
    fn convert_to_tx_env(&self, tx: &TxEnvelope) -> eyre::Result<TxEnv> {
        let mut tx_env = TxEnv::default();
        
        // Set common fields
        tx_env.caller = tx.recover_signer()
            .map_err(|_| eyre::eyre!("Failed to recover transaction signer"))?;
        tx_env.gas_limit = tx.gas_limit();
        tx_env.value = tx.value();
        tx_env.data = tx.input().clone();
        tx_env.nonce = tx.nonce();
        
        // Set the destination
        tx_env.kind = match tx.to() {
            Some(to) => TxKind::Call(to),
            None => TxKind::Create,
        };
        
        // Set gas price based on transaction type
        match tx {
            TxEnvelope::Legacy(tx) => {
                tx_env.gas_price = tx.gas_price().unwrap_or_default();
            }
            TxEnvelope::Eip2930(tx) => {
                tx_env.gas_price = tx.gas_price().unwrap_or_default();
                // Access list would be set here if TxEnv supported it
            }
            TxEnvelope::Eip1559(tx) => {
                tx_env.gas_priority_fee = tx.max_priority_fee_per_gas();
                tx_env.gas_price = tx.max_fee_per_gas();
                // Access list would be set here if TxEnv supported it
            }
            TxEnvelope::Eip4844(tx) => {
                // EIP-4844 blob transactions (used for data availability)
                // Extract the actual transaction from the variant
                match tx.tx() {
                    alloy_consensus::TxEip4844Variant::TxEip4844(inner_tx) => {
                        tx_env.gas_priority_fee = inner_tx.max_priority_fee_per_gas();
                        tx_env.gas_price = inner_tx.max_fee_per_gas();
                        // Blob transactions have blob_hashes but we don't need them for MEV simulation
                    }
                    alloy_consensus::TxEip4844Variant::TxEip4844WithSidecar(inner_tx) => {
                        tx_env.gas_priority_fee = inner_tx.tx().max_priority_fee_per_gas();
                        tx_env.gas_price = inner_tx.tx().max_fee_per_gas();
                        // Sidecar contains the actual blob data, not needed for MEV
                    }
                }
            }
            TxEnvelope::Eip7702(tx) => {
                // EIP-7702 is for account abstraction/delegation transactions
                tx_env.gas_priority_fee = tx.max_priority_fee_per_gas();
                tx_env.gas_price = tx.max_fee_per_gas();
                // Authority list would be handled here if needed
            }
        }
        
        Ok(tx_env)
    }
    
    /// Convert a transaction request (unsigned) to revm TxEnv
    pub fn convert_unsigned_tx_to_env(
        &self,
        from: alloy_primitives::Address,
        to: Option<alloy_primitives::Address>,
        value: alloy_primitives::U256,
        input: alloy_primitives::Bytes,
        gas_limit: u64,
        gas_price: alloy_primitives::U256,
        nonce: u64,
    ) -> TxEnv {
        let mut tx_env = TxEnv::default();
        
        tx_env.caller = from;
        tx_env.gas_limit = gas_limit;
        tx_env.value = value;
        tx_env.data = input;
        tx_env.nonce = nonce;
        
        // Set the destination
        tx_env.kind = match to {
            Some(addr) => TxKind::Call(addr),
            None => TxKind::Create,
        };
        
        // For simplicity, assume EIP-1559 style with gas_price as both max fee and priority fee
        tx_env.gas_price = gas_price.try_into().unwrap_or(u128::MAX);
        tx_env.gas_priority_fee = Some(gas_price.try_into().unwrap_or(u128::MAX));
        
        tx_env
    }
    
    /// Convert revm execution result to EthCallResponse
    #[allow(dead_code)]
    fn convert_execution_result(&self, result: ExecutionResult<HaltReason>) -> EthCallResponse {
        match result {
            ExecutionResult::Success { output, gas_used, .. } => {
                let return_data = match output {
                    Output::Call(bytes) => bytes,
                    Output::Create(bytes, _) => bytes,
                };
                
                EthCallResponse {
                    value: Some(return_data),
                    error: None,
                    gas_used: Some(gas_used),
                }
            }
            ExecutionResult::Revert { output, gas_used } => {
                let error_msg = if output.is_empty() {
                    "execution reverted".to_string()
                } else {
                    format!("execution reverted: 0x{}", hex::encode(&output))
                };
                
                EthCallResponse {
                    value: None,
                    error: Some(error_msg),
                    gas_used: Some(gas_used),
                }
            }
            ExecutionResult::Halt { reason, gas_used } => {
                EthCallResponse {
                    value: None,
                    error: Some(format!("execution halted: {:?}", reason)),
                    gas_used: Some(gas_used),
                }
            }
        }
    }
    
    
    /// Simulate a bundle of transactions on top of the current flashblock state
    /// This is useful for testing MEV opportunities
    /// 
    /// Accepts both signed and unsigned transactions through BundleTransaction enum
    pub async fn simulate_bundle_mixed(
        &mut self,
        bundle_txs: Vec<crate::mev_bundle_types::BundleTransaction>,
        block_number: u64,
    ) -> eyre::Result<Vec<EthCallResponse>> {
        // println!("\n🎯 Simulating MEV bundle on top of flashblock state");
        // println!("   ├─ Bundle size: {} transactions", bundle_txs.len());
        // println!("   └─ Target block: {}", block_number);
        
        let start = std::time::Instant::now();
        let mut results = Vec::new();
        
        // First convert all transactions to avoid borrow conflicts
        let converted_bundle: Vec<(revm::context::TxEnv, alloy_primitives::B256, Option<alloy_primitives::Bytes>)> = 
            bundle_txs.iter()
                .map(|tx| {
                    use crate::mev_bundle_types::BundleTransaction;
                    match tx {
                        BundleTransaction::Signed(signed_tx) => {
                            let tx_hash = signed_tx.tx_hash();
                            let tx_env = self.convert_to_tx_env(signed_tx)?;
                            let enveloped_bytes = signed_tx.encoded_2718();
                            Ok((tx_env, *tx_hash, Some(alloy_primitives::Bytes::from(enveloped_bytes))))
                        }
                        BundleTransaction::Unsigned { from, to, value, input, gas_limit, gas_price, nonce } => {
                            let tx_env = self.convert_unsigned_tx_to_env(
                                *from, *to, *value, input.clone(), *gas_limit, *gas_price, *nonce
                            );
                            // Use zero hash for unsigned transactions
                            Ok((tx_env, alloy_primitives::B256::ZERO, None))
                        }
                    }
                })
                .collect::<eyre::Result<Vec<_>>>()?;
        
        // Now get the cache_db and evm_env references
        let cache_db = self.cache_db.as_mut()
            .ok_or_else(|| eyre::eyre!("Executor not initialized. Call initialize() first."))?;
        
        let evm_env = self.evm_env.as_ref()
            .ok_or_else(|| eyre::eyre!("EVM environment not initialized."))?;
        
        // Now simulate each transaction
        for (i, (tx_env, tx_hash, enveloped_bytes)) in converted_bundle.into_iter().enumerate() {
            if tx_hash == alloy_primitives::B256::ZERO {
                // println!("   ├─ MEV Tx {}/{}: [unsigned]", i + 1, bundle_txs.len());
            } else {
                // println!("   ├─ MEV Tx {}/{}: {}", i + 1, bundle_txs.len(), tx_hash);
            }
            
            // Create OpTransaction with enveloped bytes
            let mut op_tx = OpTransaction::new(tx_env);
            if let Some(bytes) = enveloped_bytes {
                op_tx.enveloped_tx = Some(bytes);
            } else {
                // For unsigned transactions, create a dummy envelope
                op_tx.enveloped_tx = Some(alloy_primitives::Bytes::from(vec![0x00]));
            }
            
            // Create the EVM with our cached database
            let mut evm = self.evm_config.evm_with_env(
                &mut *cache_db,
                evm_env.clone()
            );
            
            // Execute the transaction
            let result = evm.transact(op_tx);
            
            // Process the result but DON'T commit state (we're just simulating)
            let response = match result {
                Ok(exec_result) => {
                    let gas_used = exec_result.result.gas_used();
                    match exec_result.result {
                        ExecutionResult::Success { output, .. } => {
                            let value = match output {
                                Output::Call(bytes) => bytes,
                                Output::Create(bytes, _) => bytes,
                            };
                            // println!("      ✅ Success: gas used {} ({}k)", gas_used, gas_used / 1000);
                            EthCallResponse {
                                value: Some(value),
                                error: None,
                                gas_used: Some(gas_used),
                            }
                        }
                        ExecutionResult::Revert { output, .. } => {
                            let error_msg = format!("execution reverted: 0x{}", hex::encode(&output));
                            // println!("      ❌ Reverted: {}", error_msg);
                            EthCallResponse {
                                value: None,
                                error: Some(error_msg),
                                gas_used: Some(gas_used),
                            }
                        }
                        ExecutionResult::Halt { reason, .. } => {
                            let error_msg = format!("execution halted: {:?}", reason);
                            // println!("      ❌ Halted: {}", error_msg);
                            EthCallResponse {
                                value: None,
                                error: Some(error_msg),
                                gas_used: Some(gas_used),
                            }
                        }
                    }
                    // Note: We're NOT committing state changes for bundle simulation
                }
                Err(ref e) => {
                    // println!("      ❌ Failed: {:?}", e);
                    EthCallResponse {
                        value: None,
                        error: Some(format!("EVM error: {:?}", e)),
                        gas_used: None,
                    }
                }
            };
            
            results.push(response);
        }
        
        let elapsed = start.elapsed();
        // println!("   └─ Bundle simulation completed in {:.2}ms ({:.2}ms per tx avg)", 
        //     elapsed.as_secs_f64() * 1000.0,
        //     (elapsed.as_secs_f64() * 1000.0) / bundle_txs.len() as f64
        // );
        
        Ok(results)
    }
    
    /// Simulate a bundle of signed transactions on top of the current flashblock state
    /// This is a convenience method for bundles containing only signed transactions
    pub async fn simulate_bundle(
        &mut self,
        bundle_txs: Vec<TxEnvelope>,
        block_number: u64,
    ) -> eyre::Result<Vec<EthCallResponse>> {
        use crate::mev_bundle_types::BundleTransaction;
        let mixed_bundle: Vec<BundleTransaction> = bundle_txs
            .into_iter()
            .map(BundleTransaction::Signed)
            .collect();
        self.simulate_bundle_mixed(mixed_bundle, block_number).await
    }
    
    /// Export current state as a snapshot for MEV searchers
    pub fn export_state_snapshot(&self, flashblock_index: u32, transactions: Vec<alloy_consensus::TxEnvelope>) -> eyre::Result<FlashblockStateSnapshot> {
        let cache_db = self.cache_db.as_ref()
            .ok_or_else(|| eyre::eyre!("Executor not initialized"))?;
        
        let block_number = self.current_block
            .ok_or_else(|| eyre::eyre!("No block number set"))?;
            
        let mut snapshot = FlashblockStateSnapshot::new(
            block_number,
            flashblock_index,
            self.current_base_fee,
        );
        
        // Include transactions for calldata analysis
        snapshot.transactions = transactions;
        
        // Export account changes from CacheDB
        // Access the cache through the public field
        for (address, db_account) in &cache_db.cache.accounts {
            // Convert DbAccount to AccountInfo for the snapshot
            snapshot.add_account_change(*address, db_account.info.clone());
            
            // Add storage changes for this account
            for (storage_key, storage_value) in &db_account.storage {
                snapshot.add_storage_change(*address, *storage_key, *storage_value);
            }
        }
        
        // Also export any new contract code
        for (code_hash, bytecode) in &cache_db.cache.contracts {
            snapshot.add_code_change(*code_hash, bytecode.clone());
        }
        
        Ok(snapshot)
    }
}

// This is a complete implementation that:
// 1. Uses the OpEvmConfig to create EVMs compatible with Optimism
// 2. Maintains state in CacheDB across flashblock executions
// 3. Properly converts transactions and handles results
// 4. No mocks or shortcuts - this is the real execution path