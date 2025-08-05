use alloy_consensus::{BlockHeader, SignableTransaction};
use reth_provider::StateProviderFactory;
use reth_revm::{database::StateProviderDatabase, db::CacheDB};
use revm::database::{DbAccount, AccountState};
use revm::Database;
use reth_optimism_evm::OpEvmConfig;
use reth_optimism_chainspec::OpChainSpec;
use reth_optimism_node::OpRethReceiptBuilder;
use reth_optimism_primitives::OpPrimitives;
use reth_evm::ConfigureEvm;
use std::sync::Arc;
use tracing::{debug, trace, info, warn, error};

use crate::flashblock_state::FlashblockStateSnapshot;
use crate::mev_search_worker::{MevStrategy, MevOpportunity};
use crate::backrun_analyzer::BackrunAnalyzer;
use crate::gradient_descent::{GradientOptimizer, GradientParams};
use crate::gradient_descent_fast::FastGradientOptimizer;
use crate::gradient_descent_multicall::MulticallGradientOptimizer;
use crate::gradient_descent_binary::BinarySearchGradientOptimizer;
use crate::lifecycle_timing::TimingTracker;

/// A short-lived MEV task that gets its own StateProvider
pub struct MevTaskWorker {
    /// The chain specification
    chain_spec: Arc<OpChainSpec>,
    /// The MEV strategy to execute
    strategy: MevStrategy,
    /// The flashblock state snapshot to apply
    state_snapshot: FlashblockStateSnapshot,
    /// When the flashblock was received (for latency tracking)
    flashblock_received_at: std::time::Instant,
    /// Optional lifecycle timing tracker
    timing_tracker: Option<TimingTracker>,
    /// Minimum profit threshold for logging
    min_profit_threshold: alloy_primitives::U256,
    /// Gas history store for adaptive optimization
    gas_history_store: Arc<crate::gas_history_store::GasHistoryStore>,
}

impl MevTaskWorker {
    pub fn new(
        chain_spec: Arc<OpChainSpec>,
        strategy: MevStrategy,
        state_snapshot: FlashblockStateSnapshot,
        flashblock_received_at: std::time::Instant,
        timing_tracker: Option<TimingTracker>,
        min_profit_threshold: alloy_primitives::U256,
        gas_history_store: Arc<crate::gas_history_store::GasHistoryStore>,
    ) -> Self {
        Self {
            chain_spec,
            strategy,
            state_snapshot,
            flashblock_received_at,
            timing_tracker,
            min_profit_threshold,
            gas_history_store,
        }
    }
    
    /// Execute the MEV search task
    pub async fn execute<P>(self, provider: P) -> eyre::Result<Option<MevOpportunity>>
    where
        P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader,
        P::Header: alloy_consensus::BlockHeader,
    {
        let task_start = std::time::Instant::now();
        let latency_ms = self.flashblock_received_at.elapsed().as_secs_f64() * 1000.0;
        debug!(
            strategy = ?self.strategy, 
            latency_ms = latency_ms,
            scan_id = %self.state_snapshot.scan_id,
            block = self.state_snapshot.block_number,
            flashblock = self.state_snapshot.flashblock_index,
            "MEV Task Worker starting search"
        );
        
        // Clone the lifecycle timing for this worker
        let mut worker_timing = if let Some(ref timing_tracker) = self.timing_tracker {
            if let Ok(tracker) = timing_tracker.try_lock() {
                tracker.clone()
            } else {
                None
            }
        } else {
            None
        };
        
        // Get a fresh state provider - this will hold a database read transaction
        let provider_start = std::time::Instant::now();
        let state_provider = provider.latest()?;
        let provider_time = provider_start.elapsed().as_secs_f64() * 1000.0;
        
        // Get the block header
        let header_start = std::time::Instant::now();
        let header = provider.header_by_number(provider.best_block_number()?)?
            .ok_or_else(|| eyre::eyre!("Header not found"))?;
        let header_time = header_start.elapsed().as_secs_f64() * 1000.0;
        
        // Create CacheDB with the state provider
        let cache_start = std::time::Instant::now();
        let mut cache_db = CacheDB::new(StateProviderDatabase::new(state_provider));
        let cache_time = cache_start.elapsed().as_secs_f64() * 1000.0;
        
        // Apply the flashblock state snapshot to the CacheDB
        let apply_start = std::time::Instant::now();
        self.apply_state_snapshot(&mut cache_db)?;
        let apply_time = apply_start.elapsed().as_secs_f64() * 1000.0;
        
        // Set up EVM configuration
        let evm_start = std::time::Instant::now();
        let evm_config: OpEvmConfig<OpChainSpec, OpPrimitives> = OpEvmConfig::new(
            self.chain_spec.clone(),
            OpRethReceiptBuilder::default(),
        );
        
        // Use MEV-friendly settings for simulation with current timestamp
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let evm_header = alloy_consensus::Header {
            number: 33_634_688, // Current Base mainnet block number
            timestamp: current_timestamp, // Today's timestamp to ensure all hardforks active
            gas_limit: 2_000_000_000, // 2 billion gas limit for MEV simulation
            base_fee_per_gas: Some(0), // Zero base fee for MEV simulation
            ..Default::default()
        };
        
        let mut _evm_env = evm_config.evm_env(&evm_header);
        _evm_env.block_env.gas_limit = 2_000_000_000; // Ensure block gas limit is set
        let evm_time = evm_start.elapsed().as_secs_f64() * 1000.0;
        
        let setup_total = task_start.elapsed().as_secs_f64() * 1000.0;
        trace!(
            provider_ms = provider_time,
            header_ms = header_time,
            cache_ms = cache_time,
            apply_ms = apply_time,
            evm_ms = evm_time,
            total_ms = setup_total,
            "Setup timing breakdown"
        );
        
        // Execute the MEV strategy
        let search_start = std::time::Instant::now();
        let result = match self.strategy {
            MevStrategy::Backrun(ref config_name) => self.search_backrun(&mut cache_db, &evm_config, &mut worker_timing).await,
        };
        let search_time = search_start.elapsed().as_secs_f64() * 1000.0;
        
        let total_time = task_start.elapsed().as_secs_f64() * 1000.0;
        
        // Get strategy name for metrics
        let strategy_name = match &self.strategy {
            MevStrategy::Backrun(config) => format!("Backrun_{}", config),
        };
        
        // Record worker duration metric
        let strategy_metrics = crate::metrics::get_strategy_metrics(&strategy_name);
        strategy_metrics.worker_duration_seconds.record(total_time / 1000.0);
        
        // Log worker-specific timing if we have timing info
        if let Some(timing) = worker_timing {
            // Record gradient duration if available (regardless of profit found)
            if let (Some(start), Some(end)) = (timing.gradient_started, timing.gradient_completed) {
                let gradient_duration = end.duration_since(start).as_secs_f64();
                strategy_metrics.gradient_duration_seconds.record(gradient_duration);
            }
            
            if result.is_ok() && result.as_ref().unwrap().is_some() {
                
                info!(
                    strategy = strategy_name,
                    block = timing.block_number,
                    flashblock = timing.flashblock_index,
                    scan_id = %self.state_snapshot.scan_id,
                    total_ms = total_time,
                    search_ms = search_time,
                    gradient_ms = timing.gradient_completed
                        .and_then(|end| timing.gradient_started.map(|start| end.duration_since(start).as_secs_f64() * 1000.0))
                        .unwrap_or(0.0),
                    "Worker completed with opportunity"
                );
            }
        } else {
            debug!(total_ms = total_time, search_ms = search_time, "Task completed");
        }
        
        // The state provider (and database transaction) will be dropped here
        result
    }
    
    /// Apply the flashblock state snapshot to the CacheDB
    fn apply_state_snapshot<DB>(&self, cache_db: &mut CacheDB<DB>) -> eyre::Result<()>
    where
        DB: revm::Database,
    {
        // Apply account changes
        for (address, account_info) in &self.state_snapshot.account_changes {
            // Update or insert account
            match cache_db.cache.accounts.entry(*address) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let db_account = entry.get_mut();
                    db_account.info = account_info.clone();
                    db_account.account_state = AccountState::Touched;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(DbAccount {
                        info: account_info.clone(),
                        account_state: AccountState::Touched,
                        storage: Default::default(),
                    });
                }
            }
        }
        
        // Apply storage changes
        for (address, storage_changes) in &self.state_snapshot.storage_changes {
            if let Some(db_account) = cache_db.cache.accounts.get_mut(address) {
                for (slot, value) in storage_changes {
                    db_account.storage.insert(*slot, *value);
                }
            }
        }
        
        // Apply code changes (contract deployments)
        for (code_hash, bytecode) in &self.state_snapshot.code_changes {
            // Add bytecode to contracts cache
            cache_db.cache.contracts.insert(*code_hash, bytecode.clone());
        }
        
        trace!(
            accounts = self.state_snapshot.account_changes.len(),
            storage = self.state_snapshot.storage_changes.values().map(|s| s.len()).sum::<usize>(),
            contracts = self.state_snapshot.code_changes.len(),
            "Applied state snapshot"
        );
        
        Ok(())
    }
    
    
    /// Search for backrun opportunities using gradient optimizer
    async fn search_backrun<DB>(&self, cache_db: &mut CacheDB<DB>, evm_config: &OpEvmConfig<OpChainSpec, OpPrimitives>, worker_timing: &mut Option<crate::lifecycle_timing::LifecycleTiming>) -> eyre::Result<Option<MevOpportunity>>
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // Get the specific config for this worker (based on strategy name)
        let config_name = match &self.strategy {
            MevStrategy::Backrun(config) => config,
            _ => return Ok(None), // Should never happen
        };
        
        debug!(config = %config_name, "Worker searching for backrun opportunity");
        
        // Record that this strategy was triggered
        let strategy_metrics = crate::metrics::get_strategy_metrics(&format!("Backrun_{}", config_name));
        strategy_metrics.triggered_total.increment(1);
        
        // Create a backrun analyzer with the worker's profit threshold
        let analyzer = BackrunAnalyzer::new(self.min_profit_threshold);
        
        // Get the configs
        let configs = analyzer.get_configs();
        if let Some(config) = configs.get(config_name) {
                debug!(
                    config = %config_name,
                    contract = %config.contract_address,
                    scan_id = %self.state_snapshot.scan_id,
                    "Checking contract for backrun config"
                );
                
                // Check if contract exists in CacheDB
                let contract_info = cache_db.basic(config.contract_address)?;
                trace!(
                    contract = %config.contract_address,
                    config = %config_name,
                    tokens = ?config.tokens,
                    "Target contract status"
                );
                match contract_info {
                    Some(info) => {
                        trace!(
                            balance = %info.balance,
                            nonce = info.nonce,
                            code_hash = ?info.code_hash,
                            has_code = (info.code_hash != alloy_primitives::KECCAK256_EMPTY),
                            "Contract info"
                        );
                        
                        if info.code_hash == alloy_primitives::KECCAK256_EMPTY {
                            warn!(
                                contract = %config.contract_address,
                                config_name = %config_name,
                                scan_id = %self.state_snapshot.scan_id,
                                "Backrun config triggered but contract has no code - skipping"
                            );
                            return Ok(None);
                        } else {
                            debug!(
                                contract = %config.contract_address,
                                config_name = %config_name,
                                scan_id = %self.state_snapshot.scan_id,
                                code_hash = ?info.code_hash,
                                "Contract has code - proceeding with optimization"
                            );
                        }
                    }
                    None => {
                        warn!(
                            contract = %config.contract_address,
                            config_name = %config_name,
                            scan_id = %self.state_snapshot.scan_id,
                            "Backrun config triggered but contract not found in state - skipping"
                        );
                        return Ok(None);
                    }
                }
                
                // Calculate bounds based on initial quantity (matching TypeScript logic)
                let min_qty = (config.default_value / alloy_primitives::U256::from(5)).max(alloy_primitives::U256::from(1)); // max(1, 1% of initial)
                let max_qty_uncapped = config.default_value.saturating_mul(alloy_primitives::U256::from(1000)); // 100x initial
                let max_qty = if max_qty_uncapped > alloy_primitives::U256::from(0xffffff) {
                    alloy_primitives::U256::from(0xffffff) // Cap at 16.7M (24-bit max)
                } else {
                    max_qty_uncapped
                };
                
                // Get filtered gas from Redis for this target
                let filtered_gas = self.gas_history_store.get_filtered_gas(&config.contract_address).await;
                
                // Create gradient parameters
                let params = GradientParams {
                    initial_qty: config.default_value,
                    calldata_template: alloy_primitives::Bytes::from(vec![0x00, 0x00, 0x00, 0x00]), // Short format
                    seed: alloy_primitives::U256::from(self.state_snapshot.block_number * 1000 + self.state_snapshot.flashblock_index as u64),
                    lower_bound: min_qty,
                    upper_bound: max_qty,
                    target_address: config.contract_address,
                    filtered_gas,
                };
                
                // Run gradient optimization - use binary search version for best performance
                let optimizer = BinarySearchGradientOptimizer::new();
                
                // Mark gradient start in worker timing
                if let Some(ref mut timing) = worker_timing {
                    timing.gradient_started = Some(std::time::Instant::now());
                }
                
                debug!(
                    config = %config_name,
                    scan_id = %self.state_snapshot.scan_id,
                    "Starting binary search optimization"
                );
                
                match optimizer.optimize_quantity(params, &self.state_snapshot, cache_db, evm_config) {
                    Ok(result) => {
                        // Mark gradient completion in worker timing
                        if let Some(ref mut timing) = worker_timing {
                            timing.gradient_completed = Some(std::time::Instant::now());
                        }
                        
                        // Save updated filtered gas and multiplier to Redis if available
                        if let Some(new_filtered_gas) = result.filtered_gas {
                            let gas_store = self.gas_history_store.clone();
                            let target = config.contract_address;
                            let multiplier = result.actual_multiplier;
                            tokio::spawn(async move {
                                gas_store.set_filtered_gas_and_multiplier(&target, new_filtered_gas, multiplier).await;
                            });
                        }
                        
                        debug!(
                            config = %config_name,
                            scan_id = %self.state_snapshot.scan_id,
                            delta = result.delta,
                            qty_in = %result.qty_in,
                            filtered_gas = ?result.filtered_gas,
                            "Binary search completed"
                        );
                        
                        // Track problematic configs
                        if result.gas_used > 30_000_000 {
                            warn!(
                                config = %config_name,
                                target = %config.contract_address,
                                gas_used = result.gas_used,
                                "High gas usage config detected"
                            );
                        }
                        
                        if result.delta > 0 {
                            let profit = alloy_primitives::U256::from(result.delta as u128);
                            
                            // Record profit metric
                            strategy_metrics.profit_wei.record(result.delta as f64);
                            
                            // Only log at info level if above threshold
                            if profit > self.min_profit_threshold {
                                strategy_metrics.profitable_total.increment(1);
                                info!(
                                    profit_wei = result.delta,
                                    profit_eth = (result.delta as f64 / 1e18),
                                    scan_id = %self.state_snapshot.scan_id,
                                    "💎💰 PROFITABLE BACKRUN DISCOVERED! 🎯🚀 Profit: {} ETH ({} wei)! 🎊✨ MONEY PRINTER GO BRRR! 🖨️💸",
                                    (result.delta as f64 / 1e18),
                                    result.delta
                                );
                            } else {
                                info!(
                                    profit_wei = result.delta,
                                    profit_eth = (result.delta as f64 / 1e18),
                                    threshold_wei = %self.min_profit_threshold,
                                    threshold_eth = (self.min_profit_threshold.as_limbs()[0] as f64 / 1e18),
                                    scan_id = %self.state_snapshot.scan_id,
                                    "Found backrun but profit below threshold - not submitting"
                                );
                            }
                            
                            // Bot address for MEV execution
                            let bot_address = alloy_primitives::Address::from([0xc0, 0xff, 0xee, 0x48, 0x94, 0x5a, 0x95, 0x18, 
                                                                               0xb0, 0xb5, 0x43, 0xa2, 0xc5, 0x9d, 0xfb, 0x10, 
                                                                               0x22, 0x21, 0xfb, 0xb7]);
                            
                            // First, simulate the transaction with value=0 to get gas usage
                            debug!("Simulating transaction to determine gas usage");
                            
                            let gas_used = match self.simulate_transaction(
                                cache_db,
                                evm_config,
                                bot_address,
                                config.contract_address,
                                result.calldata_used.clone(),
                                alloy_primitives::U256::from(0), // Zero value for gas estimation
                            ) {
                                Ok(gas) => gas,
                                Err(e) => {
                                    warn!(error = ?e, "Failed to simulate transaction, using default gas");
                                    200_000 // Default fallback
                                }
                            };
                            
                            // Check ERC20 balance if configured
                            let balance_check_value = if let Some((erc20_token, check_address)) = config.check_balance_of {
                                match self.get_erc20_balance(cache_db, erc20_token, check_address) {
                                    Ok(balance) => {
                                        // Take bottom 2 bytes of balance
                                        let balance_u16 = (balance.as_limbs()[0] & 0xffff) as u16;
                                        debug!(
                                            erc20 = %erc20_token,
                                            address = %check_address,
                                            full_balance = %balance,
                                            encoded_balance = balance_u16,
                                            "ERC20 balance check performed"
                                        );
                                        balance_u16
                                    }
                                    Err(e) => {
                                        warn!(
                                            erc20 = %erc20_token,
                                            address = %check_address,
                                            error = ?e,
                                            "Failed to check ERC20 balance, using default"
                                        );
                                        500 // Default bribe rate on error
                                    }
                                }
                            } else {
                                500 // Default bribe rate when no balance check configured
                            };
                            
                            // Calculate bribe value based on actual gas used and balance check
                            let bribe_value = self.encode_transaction_value(gas_used, balance_check_value);
                            debug!(
                                gas_used = gas_used,
                                bribe_value = %bribe_value,
                                balance_check_value = balance_check_value,
                                "Calculated bribe value from gas simulation and balance check"
                            );
                            
                            // Create MEV bundle with calculated bribe value
                            let bundle = crate::mev_bundle_types::MevBundle::new(
                                vec![crate::mev_bundle_types::BundleTransaction::unsigned(
                                    bot_address,
                                    Some(config.contract_address),
                                    bribe_value, // Use calculated bribe value
                                    result.calldata_used,
                                    4_000_000, // gas limit
                                    alloy_primitives::U256::from(self.state_snapshot.base_fee + 100_000),
                                    0, // nonce
                                )],
                                self.state_snapshot.block_number,
                            );
                            
                            // Get the hash of the last transaction in the flashblock
                            let last_tx_hash = self.state_snapshot.transactions.last()
                                .map(|tx| *tx.tx_hash());
                            
                            return Ok(Some(MevOpportunity {
                                block_number: self.state_snapshot.block_number,
                                flashblock_index: self.state_snapshot.flashblock_index,
                                bundle,
                                expected_profit: alloy_primitives::U256::from(result.delta as u128),
                                strategy: format!("Backrun_{}", config_name),
                                simulated_gas_used: Some(gas_used),
                                last_flashblock_tx_hash: last_tx_hash,
                                scan_id: self.state_snapshot.scan_id.clone(),
                            }));
                        } else {
                            debug!(
                                scan_id = %self.state_snapshot.scan_id,
                                "Gradient optimization found no profit - not submitting"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            error = ?e,
                            config = %config_name,
                            scan_id = %self.state_snapshot.scan_id,
                            "Binary search optimization error"
                        );
                    }
                }
            }
        
        Ok(None)
    }
    
    /// Encode transaction value with gas cost and bribe info
    fn encode_transaction_value(&self, gas_cost: u64, bribe_rate: u16) -> alloy_primitives::U256 {
        let encoded = ((gas_cost / 10) << 16) | bribe_rate as u64;
        alloy_primitives::U256::from(encoded)
    }
    
    /// Simulate a transaction to get gas usage
    fn simulate_transaction<DB>(
        &self,
        cache_db: &mut CacheDB<DB>,
        evm_config: &OpEvmConfig<OpChainSpec, OpPrimitives>,
        from: alloy_primitives::Address,
        to: alloy_primitives::Address,
        calldata: alloy_primitives::Bytes,
        value: alloy_primitives::U256,
    ) -> eyre::Result<u64>
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // Fund the sender account if needed
        let sender_info = match cache_db.basic(from)? {
            Some(info) if info.balance >= value => info,
            _ => {
                // Need to fund the account
                let account_info = revm::state::AccountInfo {
                    balance: alloy_primitives::U256::from(1_000_000_000_000_000_000u64), // 1 ETH
                    nonce: 0,
                    code_hash: alloy_primitives::KECCAK256_EMPTY,
                    code: None,
                };
                
                cache_db.cache.accounts.insert(from, DbAccount {
                    info: account_info.clone(),
                    account_state: AccountState::Touched,
                    storage: Default::default(),
                });
                
                account_info
            }
        };
        
        // Create dummy signature for simulation
        let signature = alloy_primitives::Signature::new(
            alloy_primitives::U256::from(1),
            alloy_primitives::U256::from(1), 
            false
        );
        
        // Set up transaction environment
        let mut tx_env = revm::context::TxEnv::default();
        tx_env.caller = from;
        tx_env.nonce = sender_info.nonce;
        tx_env.kind = revm::primitives::TxKind::Call(to);
        tx_env.data = calldata.clone();
        tx_env.gas_limit = 4_000_000;
        tx_env.gas_price = (self.state_snapshot.base_fee + 100_000) as u128;
        tx_env.gas_priority_fee = Some(100_000u128);
        tx_env.value = value;
        
        // Create transaction for Optimism
        let tx_eip1559 = alloy_consensus::TxEip1559 {
            chain_id: 8453, // Base mainnet
            nonce: sender_info.nonce,
            gas_limit: 4_000_000,
            max_fee_per_gas: self.state_snapshot.base_fee as u128 + 100_000,
            max_priority_fee_per_gas: 100_000,
            to: alloy_primitives::TxKind::Call(to),
            value,
            access_list: Default::default(),
            input: calldata,
        };
        
        let signed_tx = alloy_consensus::Signed::new_unchecked(tx_eip1559, signature, Default::default());
        let tx_envelope = alloy_consensus::TxEnvelope::Eip1559(signed_tx);
        let enveloped_bytes = alloy_eips::eip2718::Encodable2718::encoded_2718(&tx_envelope);
        
        let mut op_tx = op_revm::OpTransaction::new(tx_env);
        op_tx.enveloped_tx = Some(enveloped_bytes.into());
        
        // Use MEV-friendly EVM environment
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let evm_env = evm_config.evm_env(&alloy_consensus::Header {
            base_fee_per_gas: Some(0), // Zero base fee for MEV simulation
            gas_limit: 2_000_000_000,
            number: 33_634_688,
            timestamp: current_timestamp,
            ..Default::default()
        });
        
        // Create EVM for simulation
        let mut evm = evm_config.evm_with_env(&mut *cache_db, evm_env);
        
        // Execute and extract gas used
        use reth_evm::Evm;
        match evm.transact(op_tx) {
            Ok(result) => {
                let gas = result.result.gas_used();
                trace!(
                    from = %from,
                    to = %to,
                    value = %value,
                    gas_used = gas,
                    "Transaction simulation complete"
                );
                Ok(gas)
            }
            Err(e) => {
                debug!(error = ?e, "Transaction simulation failed");
                Err(e.into())
            }
        }
    }
    
    /// Get ERC20 balance for an address using the existing cache_db
    fn get_erc20_balance<DB>(
        &self,
        cache_db: &mut CacheDB<DB>,
        token_address: alloy_primitives::Address,
        check_address: alloy_primitives::Address,
    ) -> eyre::Result<alloy_primitives::U256>
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // ERC20 balanceOf(address) selector = 0x70a08231
        let mut calldata = vec![0x70, 0xa0, 0x82, 0x31];
        // Append the address (padded to 32 bytes)
        calldata.extend_from_slice(&[0u8; 12]); // 12 zero bytes for padding
        calldata.extend_from_slice(check_address.as_slice());
        
        // Simulate a static call using MEV simulation environment
        let bot_address = alloy_primitives::Address::from([0xc0, 0xff, 0xee, 0x48, 0x94, 0x5a, 0x95, 0x18, 
                                                           0xb0, 0xb5, 0x43, 0xa2, 0xc5, 0x9d, 0xfb, 0x10, 
                                                           0x22, 0x21, 0xfb, 0xb7]);
        
        // Fund the bot address if needed
        match cache_db.basic(bot_address)? {
            None => {
                let account_info = revm::state::AccountInfo {
                    balance: alloy_primitives::U256::from(1_000_000_000_000_000_000u64), // 1 ETH
                    nonce: 0,
                    code_hash: alloy_primitives::KECCAK256_EMPTY,
                    code: None,
                };
                
                cache_db.cache.accounts.insert(bot_address, DbAccount {
                    info: account_info,
                    account_state: AccountState::Touched,
                    storage: Default::default(),
                });
            }
            _ => {}
        };
        
        // Use the simulate_transaction method we already have, but with minimal gas
        // This will execute the balanceOf call and return the result
        match self.simulate_balance_query(cache_db, bot_address, token_address, calldata.into()) {
            Ok(output) => {
                if output.len() >= 32 {
                    // Parse the first 32 bytes as U256
                    let mut balance_bytes = [0u8; 32];
                    balance_bytes.copy_from_slice(&output[..32]);
                    let balance = alloy_primitives::U256::from_be_bytes(balance_bytes);
                    trace!(
                        token = %token_address,
                        address = %check_address,
                        balance = %balance,
                        "ERC20 balance query successful"
                    );
                    Ok(balance)
                } else {
                    Err(eyre::eyre!("Invalid balance response length: {}", output.len()))
                }
            }
            Err(e) => {
                debug!(
                    token = %token_address,
                    address = %check_address,
                    error = ?e,
                    "ERC20 balance query failed"
                );
                Err(e)
            }
        }
    }
    
    /// Simulate a balance query call and return the output data
    fn simulate_balance_query<DB>(
        &self,
        cache_db: &mut CacheDB<DB>,
        from: alloy_primitives::Address,
        to: alloy_primitives::Address,
        calldata: alloy_primitives::Bytes,
    ) -> eyre::Result<Vec<u8>>
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        // Get sender info
        let sender_info = match cache_db.basic(from)? {
            Some(info) => info,
            None => {
                return Err(eyre::eyre!("Sender account not found"));
            }
        };
        
        // Create dummy signature for simulation
        let signature = alloy_primitives::Signature::new(
            alloy_primitives::U256::from(1),
            alloy_primitives::U256::from(1), 
            false
        );
        
        // Set up transaction environment
        let mut tx_env = revm::context::TxEnv::default();
        tx_env.caller = from;
        tx_env.nonce = sender_info.nonce;
        tx_env.kind = revm::primitives::TxKind::Call(to);
        tx_env.data = calldata.clone();
        tx_env.gas_limit = 100_000; // Small gas limit for view function
        tx_env.gas_price = 0; // Static call
        tx_env.gas_priority_fee = Some(0);
        tx_env.value = alloy_primitives::U256::ZERO;
        
        // Create transaction for Optimism
        let tx_eip1559 = alloy_consensus::TxEip1559 {
            chain_id: 8453, // Base mainnet
            nonce: sender_info.nonce,
            gas_limit: tx_env.gas_limit,
            max_fee_per_gas: 0,
            max_priority_fee_per_gas: 0,
            to: alloy_primitives::TxKind::Call(to),
            value: alloy_primitives::U256::ZERO,
            access_list: Default::default(),
            input: calldata,
        };
        
        let signed_tx = alloy_consensus::Signed::new_unchecked(tx_eip1559, signature, Default::default());
        let tx_envelope = alloy_consensus::TxEnvelope::Eip1559(signed_tx);
        let enveloped_bytes = alloy_eips::eip2718::Encodable2718::encoded_2718(&tx_envelope);
        
        let mut op_tx = op_revm::OpTransaction::new(tx_env);
        op_tx.enveloped_tx = Some(enveloped_bytes.into());
        
        // Use MEV-friendly EVM environment for the query
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let evm_config: OpEvmConfig<OpChainSpec, OpPrimitives> = OpEvmConfig::new(
            self.chain_spec.clone(),
            OpRethReceiptBuilder::default(),
        );
        
        let evm_env = evm_config.evm_env(&alloy_consensus::Header {
            base_fee_per_gas: Some(0),
            gas_limit: 2_000_000_000,
            number: self.state_snapshot.block_number,
            timestamp: current_timestamp,
            ..Default::default()
        });
        
        // Create EVM for simulation
        let mut evm = evm_config.evm_with_env(&mut *cache_db, evm_env);
        
        // Execute and extract output
        use reth_evm::Evm;
        match evm.transact(op_tx) {
            Ok(result) => {
                if let Some(output) = result.result.output() {
                    Ok(output.to_vec())
                } else {
                    Err(eyre::eyre!("No output from static call"))
                }
            }
            Err(e) => {
                Err(eyre::eyre!("Static call execution failed: {:?}", e))
            }
        }
    }
}

/// Get the MEV worker timeout duration from environment or use default
fn get_worker_timeout() -> std::time::Duration {
    std::env::var("MEV_WORKER_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(std::time::Duration::from_secs(30))
}

/// Spawn a short-lived MEV task
pub fn spawn_mev_task<P>(
    chain_spec: Arc<OpChainSpec>,
    provider: P,
    strategy: MevStrategy,
    state_snapshot: FlashblockStateSnapshot,
    flashblock_received_at: std::time::Instant,
    result_tx: tokio::sync::mpsc::Sender<MevOpportunity>,
    timing_tracker: Option<TimingTracker>,
    min_profit_threshold: alloy_primitives::U256,
    gas_history_store: Arc<crate::gas_history_store::GasHistoryStore>,
)
where
    P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader + Clone + Send + 'static,
    P::Header: alloy_consensus::BlockHeader,
{
    tokio::spawn(async move {
        let worker = MevTaskWorker::new(
            chain_spec,
            strategy.clone(),
            state_snapshot,
            flashblock_received_at,
            timing_tracker,
            min_profit_threshold,
            gas_history_store,
        );
        
        // Add timeout to prevent stuck workers
        let timeout_duration = get_worker_timeout();
        match tokio::time::timeout(timeout_duration, worker.execute(provider)).await {
            Ok(Ok(Some(opportunity))) => {
                // Only log at info level if above threshold
                if opportunity.expected_profit > min_profit_threshold {
                    info!("MEV opportunity found");
                } else {
                    debug!("MEV opportunity found below threshold");
                }
                if let Err(e) = result_tx.send(opportunity).await {
                    error!(error = ?e, "Failed to send MEV opportunity");
                }
            }
            Ok(Ok(None)) => {
                // No opportunity found
            }
            Ok(Err(e)) => {
                error!(error = ?e, "MEV task error");
            }
            Err(_) => {
                error!(
                    strategy = ?strategy,
                    timeout_secs = timeout_duration.as_secs(),
                    "MEV task timed out after {} seconds - likely stuck database transaction",
                    timeout_duration.as_secs()
                );
            }
        }
    });
}

/// Spawn multiple MEV tasks in batch for reduced overhead
pub fn spawn_mev_tasks_batch<P>(
    chain_spec: Arc<OpChainSpec>,
    provider: P,
    strategies: Vec<MevStrategy>,
    state_snapshot: FlashblockStateSnapshot,
    flashblock_received_at: std::time::Instant,
    result_tx: tokio::sync::mpsc::Sender<MevOpportunity>,
    timing_tracker: Option<TimingTracker>,
    min_profit_threshold: alloy_primitives::U256,
    gas_history_store: Arc<crate::gas_history_store::GasHistoryStore>,
)
where
    P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader + Clone + Send + 'static,
    P::Header: alloy_consensus::BlockHeader,
{
    // Convert to Arc to share across tasks without cloning
    let state_snapshot = Arc::new(state_snapshot);
    
    // Spawn all tasks with a single batch operation
    let handles: Vec<_> = strategies.into_iter().map(|strategy| {
        let chain_spec = chain_spec.clone();
        let provider = provider.clone();
        let state_snapshot = state_snapshot.clone();
        let result_tx = result_tx.clone();
        let timing_tracker = timing_tracker.clone();
        let gas_history_store = gas_history_store.clone();
        
        tokio::spawn(async move {
            let worker = MevTaskWorker::new(
                chain_spec,
                strategy.clone(),
                (*state_snapshot).clone(), // Only clone when actually needed
                flashblock_received_at,
                timing_tracker,
                min_profit_threshold,
                gas_history_store,
            );
            
            // Add timeout to prevent stuck workers
            let timeout_duration = get_worker_timeout();
            match tokio::time::timeout(timeout_duration, worker.execute(provider)).await {
                Ok(Ok(Some(opportunity))) => {
                    // Only log at info level if above threshold
                    if opportunity.expected_profit > min_profit_threshold {
                        info!("MEV opportunity found");
                    } else {
                        debug!("MEV opportunity found below threshold");
                    }
                    if let Err(e) = result_tx.send(opportunity).await {
                        error!(error = ?e, "Failed to send MEV opportunity");
                    }
                }
                Ok(Ok(None)) => {
                    // No opportunity found
                }
                Ok(Err(e)) => {
                    error!(error = ?e, "MEV task error");
                }
                Err(_) => {
                    error!(
                        strategy = ?strategy,
                        timeout_secs = timeout_duration.as_secs(),
                        "MEV task timed out after {} seconds - likely stuck database transaction",
                        timeout_duration.as_secs()
                    );
                }
            }
        })
    }).collect();
    
    // Optionally join all tasks to track completion
    tokio::spawn(async move {
        for handle in handles {
            let _ = handle.await;
        }
    });
}