use alloy_consensus::BlockHeader;
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

use crate::flashblock_state::FlashblockStateSnapshot;
use crate::mev_search_worker::{MevStrategy, MevOpportunity};
use crate::backrun_analyzer::BackrunAnalyzer;
use crate::gradient_descent::{GradientOptimizer, GradientParams};
use crate::gradient_descent_fast::FastGradientOptimizer;
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
}

impl MevTaskWorker {
    pub fn new(
        chain_spec: Arc<OpChainSpec>,
        strategy: MevStrategy,
        state_snapshot: FlashblockStateSnapshot,
        flashblock_received_at: std::time::Instant,
        timing_tracker: Option<TimingTracker>,
    ) -> Self {
        Self {
            chain_spec,
            strategy,
            state_snapshot,
            flashblock_received_at,
            timing_tracker,
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
        println!("🔍 MEV Task Worker starting {:?} search (latency: {:.2}ms)", 
            self.strategy, latency_ms);
        
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
        println!("   ⏱️  Setup timing breakdown:");
        println!("      ├─ State provider: {:.2}ms", provider_time);
        println!("      ├─ Block header: {:.2}ms", header_time);
        println!("      ├─ CacheDB creation: {:.2}ms", cache_time);
        println!("      ├─ Apply snapshot: {:.2}ms", apply_time);
        println!("      ├─ EVM setup: {:.2}ms", evm_time);
        println!("      └─ Total setup: {:.2}ms", setup_total);
        
        // Execute the MEV strategy
        let search_start = std::time::Instant::now();
        let result = match self.strategy {
            MevStrategy::DexArbitrage => self.search_dex_arbitrage(&mut cache_db),
            MevStrategy::Liquidation => self.search_liquidations(&mut cache_db),
            MevStrategy::Sandwich => self.search_sandwich(&mut cache_db),
            MevStrategy::JitLiquidity => self.search_jit_liquidity(&mut cache_db),
            MevStrategy::Backrun => self.search_backrun(&mut cache_db, &evm_config, &self.timing_tracker),
        };
        let search_time = search_start.elapsed().as_secs_f64() * 1000.0;
        
        let total_time = task_start.elapsed().as_secs_f64() * 1000.0;
        println!("   ⏱️  Task completed in {:.2}ms (search: {:.2}ms)", total_time, search_time);
        
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
        
        println!("   ⚡ Applied state snapshot: {} accounts, {} storage, {} contracts",
            self.state_snapshot.account_changes.len(),
            self.state_snapshot.storage_changes.values().map(|s| s.len()).sum::<usize>(),
            self.state_snapshot.code_changes.len()
        );
        
        Ok(())
    }
    
    /// Search for DEX arbitrage opportunities
    fn search_dex_arbitrage<DB>(&self, _cache_db: &mut CacheDB<DB>) -> eyre::Result<Option<MevOpportunity>>
    where
        DB: revm::Database,
    {
        // TODO: Implement actual DEX arbitrage search
        // 1. Check current pool states for affected DEXs
        // 2. Calculate arbitrage paths
        // 3. Simulate swaps to verify profitability
        // 4. Build MEV bundle if profitable
        
        println!("   📊 Searching for DEX arbitrage opportunities...");
        
        // For now, return None (no opportunity found)
        Ok(None)
    }
    
    /// Search for liquidation opportunities
    fn search_liquidations<DB>(&self, _cache_db: &mut CacheDB<DB>) -> eyre::Result<Option<MevOpportunity>>
    where
        DB: revm::Database,
    {
        // TODO: Implement actual liquidation search
        // 1. Check lending protocol positions
        // 2. Calculate liquidation profitability
        // 3. Build liquidation bundle if profitable
        
        println!("   💸 Searching for liquidation opportunities...");
        
        // For now, return None
        Ok(None)
    }
    
    /// Search for sandwich opportunities
    fn search_sandwich<DB>(&self, _cache_db: &mut CacheDB<DB>) -> eyre::Result<Option<MevOpportunity>>
    where
        DB: revm::Database,
    {
        // TODO: Implement sandwich attack search
        println!("   🥪 Searching for sandwich opportunities...");
        Ok(None)
    }
    
    /// Search for JIT liquidity opportunities
    fn search_jit_liquidity<DB>(&self, _cache_db: &mut CacheDB<DB>) -> eyre::Result<Option<MevOpportunity>>
    where
        DB: revm::Database,
    {
        // TODO: Implement JIT liquidity provision
        println!("   💧 Searching for JIT liquidity opportunities...");
        Ok(None)
    }
    
    /// Search for backrun opportunities using gradient optimizer
    fn search_backrun<DB>(&self, cache_db: &mut CacheDB<DB>, evm_config: &OpEvmConfig<OpChainSpec, OpPrimitives>, timing_tracker: &Option<TimingTracker>) -> eyre::Result<Option<MevOpportunity>>
    where
        DB: revm::Database + revm::DatabaseRef + std::fmt::Debug,
        <DB as revm::DatabaseRef>::Error: Send + Sync + 'static,
    {
        println!("   🏃 Searching for backrun opportunities...");
        
        // Create a backrun analyzer
        let analyzer = BackrunAnalyzer::new(alloy_primitives::U256::from(1_000_000_000_000_000u64)); // 0.001 ETH min profit
        
        // Check which configs are triggered by the state changes
        let triggered_configs = analyzer.analyze_state_for_backrun(&self.state_snapshot);
        
        if triggered_configs.is_empty() {
            println!("   ❌ No backrun opportunities detected");
            return Ok(None);
        }
        
        println!("   🎯 Triggered {} backrun configs: {:?}", triggered_configs.len(), triggered_configs);
        
        // For now, just try the first triggered config
        // TODO: Try all configs in parallel
        if let Some(config_name) = triggered_configs.first() {
            // Get the config
            let configs = analyzer.get_configs();
            if let Some(config) = configs.get(config_name) {
                // Check if contract exists in CacheDB
                let contract_info = cache_db.basic(config.contract_address)?;
                println!("   🔍 Target contract {} status:", config.contract_address);
                println!("      - Config: {}", config_name);
                println!("      - Tokens: {:?}", config.tokens);
                match contract_info {
                    Some(info) => {
                        println!("      - Balance: {} wei", info.balance);
                        println!("      - Nonce: {}", info.nonce);
                        println!("      - Code hash: {:?}", info.code_hash);
                        println!("      - Code exists: {}", info.code_hash != alloy_primitives::KECCAK256_EMPTY);
                        
                        if info.code_hash == alloy_primitives::KECCAK256_EMPTY {
                            println!("      ❌ Contract has no code - skipping optimization!");
                            return Ok(None);
                        }
                    }
                    None => {
                        println!("      ⚠️  Contract not found in state - skipping!");
                        return Ok(None);
                    }
                }
                
                // Create gradient parameters
                let params = GradientParams {
                    initial_qty: config.default_value,
                    calldata_template: alloy_primitives::Bytes::from(vec![0x00, 0x00, 0x00, 0x00]), // Short format
                    seed: alloy_primitives::U256::from(self.state_snapshot.block_number * 1000 + self.state_snapshot.flashblock_index as u64),
                    lower_bound: alloy_primitives::U256::from(10),
                    upper_bound: alloy_primitives::U256::from(100_000_000), // 100M
                    target_address: config.contract_address,
                };
                
                // Run gradient optimization - use fast version for speed
                let optimizer = FastGradientOptimizer::new();
                
                // Mark gradient start in timing (if available)
                if let Some(ref timing) = timing_tracker {
                    if let Ok(mut t) = timing.try_lock() {
                        if let Some(ref mut lifecycle) = *t {
                            lifecycle.gradient_started = Some(std::time::Instant::now());
                        }
                    }
                }
                
                match optimizer.optimize_quantity(params, &self.state_snapshot, cache_db, evm_config) {
                    Ok(result) => {
                        // Mark gradient completion in timing
                        if let Some(ref timing) = timing_tracker {
                            if let Ok(mut t) = timing.try_lock() {
                                if let Some(ref mut lifecycle) = *t {
                                    lifecycle.gradient_completed = Some(std::time::Instant::now());
                                    
                                    // Print timing report
                                    println!("{}", lifecycle.generate_report());
                                }
                            }
                        }
                        
                        if result.delta > 0 {
                            println!("   💰 Found profitable backrun! Profit: {} wei", result.delta);
                            
                            // Create MEV bundle
                            let bundle = crate::mev_bundle_types::MevBundle::new(
                                vec![crate::mev_bundle_types::BundleTransaction::unsigned(
                                    // Use a dummy from address for now
                                    alloy_primitives::Address::from([0xc0, 0xff, 0xee, 0x48, 0x94, 0x5a, 0x95, 0x18, 
                                                                     0xb0, 0xb5, 0x43, 0xa2, 0xc5, 0x9d, 0xfb, 0x10, 
                                                                     0x22, 0x21, 0xfb, 0xb7]),
                                    Some(config.contract_address),
                                    self.encode_transaction_value(result.gas_used, 500), // 5% bribe
                                    result.calldata_used,
                                    4_000_000, // gas limit
                                    alloy_primitives::U256::from(self.state_snapshot.base_fee + 1_000_000_000),
                                    0, // nonce
                                )],
                                self.state_snapshot.block_number,
                            );
                            
                            return Ok(Some(MevOpportunity {
                                block_number: self.state_snapshot.block_number,
                                flashblock_index: self.state_snapshot.flashblock_index,
                                bundle,
                                expected_profit: alloy_primitives::U256::from(result.delta as u128),
                                strategy: format!("Backrun_{}", config_name),
                            }));
                        } else {
                            println!("   ❌ No profitable backrun found");
                        }
                    }
                    Err(e) => {
                        println!("   ❌ Gradient optimization error: {:?}", e);
                    }
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
)
where
    P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader + Clone + Send + 'static,
    P::Header: alloy_consensus::BlockHeader,
{
    tokio::spawn(async move {
        let worker = MevTaskWorker::new(
            chain_spec,
            strategy,
            state_snapshot,
            flashblock_received_at,
            timing_tracker,
        );
        
        match worker.execute(provider).await {
            Ok(Some(opportunity)) => {
                println!("   💰 MEV opportunity found!");
                if let Err(e) = result_tx.send(opportunity).await {
                    println!("   ❌ Failed to send MEV opportunity: {:?}", e);
                }
            }
            Ok(None) => {
                // No opportunity found
            }
            Err(e) => {
                println!("   ❌ MEV task error: {:?}", e);
            }
        }
    });
}