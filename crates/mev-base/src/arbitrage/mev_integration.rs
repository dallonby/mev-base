use alloy_primitives::{Address, U256};
use reth_revm::db::CacheDB;
use revm::{DatabaseRef, state::AccountInfo, bytecode::Bytecode};
use std::sync::Arc;
use std::convert::Infallible;
use tokio::sync::mpsc;
use tracing::{info, debug, warn, error};

use crate::arbitrage::{
    ArbitrageAnalyzer, ArbitrageConfig, AtomicArbitrageExecutor,
    PoolDiscoveryStrategy, PoolMonitor, ArbitragePath,
};
use crate::flashblock_state::FlashblockStateSnapshot;
use crate::mev_bundle_types::MevBundle;

/// Integration layer between arbitrage system and existing MEV infrastructure
pub struct ArbitrageMevIntegration {
    /// Arbitrage analyzer
    analyzer: ArbitrageAnalyzer,
    /// Atomic executor
    executor: AtomicArbitrageExecutor,
    /// Pool discovery
    discovery: PoolDiscoveryStrategy,
    /// Pool monitor
    monitor: PoolMonitor,
    /// Channel to send bundles to sequencer
    bundle_tx: mpsc::Sender<MevBundle>,
}

impl ArbitrageMevIntegration {
    pub fn new(
        config: ArbitrageConfig,
        bundle_tx: mpsc::Sender<MevBundle>,
    ) -> Self {
        Self {
            analyzer: ArbitrageAnalyzer::new(config.clone()),
            executor: AtomicArbitrageExecutor::new(config.min_profit_threshold),
            discovery: PoolDiscoveryStrategy::new(),
            monitor: PoolMonitor::new(),
            bundle_tx,
        }
    }
    
    /// Main entry point called by MEV task worker
    pub async fn process_flashblock<DB: DatabaseRef>(
        &mut self,
        state: &FlashblockStateSnapshot,
        cache_db: &mut CacheDB<DB>,
    ) -> Vec<MevBundle> {
        let mut bundles = Vec::new();
        
        // Step 1: Update pool states for hot pools
        self.update_hot_pools(state.block_number, cache_db).await;
        
        // Step 2: Analyze each transaction for arbitrage triggers
        for tx in &state.transactions {
            let paths = self.analyzer.analyze_transaction(tx, state, cache_db);
            if !paths.is_empty() {
                    info!(
                        "Found {} arbitrage paths from tx {}",
                        paths.len(),
                        tx.hash()
                    );
                    
                    // Process top opportunities
                    for path in paths.iter().take(3) {
                        if let Some(bundle) = self.execute_arbitrage(path, state, cache_db).await {
                            bundles.push(bundle);
                        }
                    }
            }
        }
        
        // Step 3: Check for standalone arbitrage (not triggered by specific tx)
        if let Some(standalone) = self.find_standalone_arbitrage(state, cache_db).await {
            bundles.extend(standalone);
        }
        
        info!(
            "Generated {} arbitrage bundles for block {}",
            bundles.len(),
            state.block_number
        );
        
        bundles
    }
    
    /// Update states of frequently used pools
    async fn update_hot_pools<DB: DatabaseRef>(
        &mut self,
        block_number: u64,
        cache_db: &mut CacheDB<DB>,
    ) {
        // Update pools that are in active arbitrage paths
        for pool_address in self.monitor.watched_pools.iter() {
            if self.monitor.should_update(*pool_address, block_number) {
                debug!("Updating hot pool {}", pool_address);
                // Pool fetcher would update the pool state here
            }
        }
    }
    
    /// Execute an arbitrage opportunity
    async fn execute_arbitrage<DB: DatabaseRef>(
        &mut self,
        path: &ArbitragePath,
        state: &FlashblockStateSnapshot,
        cache_db: &mut CacheDB<DB>,
    ) -> Option<MevBundle> {
        // Execute atomically
        match self.executor.execute_arbitrage(path, state, cache_db) {
            Ok(calldata) => {
                let tx = self.executor.build_transaction_envelope(
                    calldata,
                    0, // Nonce will be set by transaction service
                    U256::from(state.base_fee),
                    U256::from(path.route.gas_estimate),
                );
                
                let bundle = MevBundle {
                    block_number: state.block_number,
                    transactions: vec![crate::mev_bundle_types::BundleTransaction::Signed(tx)],
                };
                
                info!(
                    "Created arbitrage bundle with profit {} wei",
                    path.net_profit
                );
                
                Some(bundle)
            }
            Err(e) => {
                warn!("Failed to execute arbitrage: {}", e);
                None
            }
        }
    }
    
    /// Find arbitrage opportunities not triggered by specific transactions
    async fn find_standalone_arbitrage<DB: DatabaseRef>(
        &mut self,
        _state: &FlashblockStateSnapshot,
        _cache_db: &mut CacheDB<DB>,
    ) -> Option<Vec<MevBundle>> {
        // This would periodically scan for arbitrage cycles
        // independent of incoming transactions
        
        // For now, return None
        None
    }
    
    /// Called when a bundle is successfully included
    pub fn on_bundle_success(&mut self, _bundle: &MevBundle, actual_profit: U256) {
        info!(
            "Arbitrage bundle succeeded with profit {} (expected {})",
            actual_profit, "N/A"
        );
        
        // Update our models based on actual results
        // This helps improve future predictions
    }
    
    /// Called when a bundle fails or is not included  
    pub fn on_bundle_failure(&mut self, _bundle: &MevBundle, reason: &str) {
        warn!(
            "Arbitrage bundle failed: {} (expected profit {})",
            reason, "N/A"
        );
        
        // Learn from failures to avoid similar issues
    }
}

/// Worker task for continuous arbitrage monitoring
pub async fn run_arbitrage_worker(
    mut integration: ArbitrageMevIntegration,
    mut flashblock_rx: mpsc::Receiver<Arc<FlashblockStateSnapshot>>,
) {
    info!("Starting arbitrage worker");
    
    while let Some(state) = flashblock_rx.recv().await {
        // Process each flashblock for arbitrage opportunities
        let start = std::time::Instant::now();
        
        // Create a cache DB for this flashblock
        // In production, this would be properly initialized
        let mut cache_db = CacheDB::new(EmptyDB);
        
        let bundles = integration.process_flashblock(&state, &mut cache_db).await;
        
        let elapsed = start.elapsed();
        debug!(
            "Processed flashblock {} in {:?}, found {} opportunities",
            state.block_number,
            elapsed,
            bundles.len()
        );
        
        // Send bundles to sequencer
        for bundle in bundles {
            if let Err(e) = integration.bundle_tx.send(bundle).await {
                error!("Failed to send bundle: {}", e);
            }
        }
    }
}

/// Empty database for testing
struct EmptyDB;

impl DatabaseRef for EmptyDB {
    type Error = Infallible;
    
    fn basic_ref(&self, _address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(None)
    }
    
    fn code_by_hash_ref(&self, _code_hash: revm::primitives::B256) -> Result<Bytecode, Self::Error> {
        Ok(Bytecode::default())
    }
    
    fn storage_ref(&self, _address: Address, _index: revm::primitives::U256) -> Result<revm::primitives::U256, Self::Error> {
        Ok(revm::primitives::U256::ZERO)
    }
    
    fn block_hash_ref(&self, _number: u64) -> Result<revm::primitives::B256, Self::Error> {
        Ok(revm::primitives::B256::ZERO)
    }
}