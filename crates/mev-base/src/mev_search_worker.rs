use crate::flashblock_state::FlashblockStateSnapshot;
use crate::mev_bundle_types::MevBundle;
use alloy_primitives::U256;
use tokio::sync::mpsc;
use crossbeam::deque::{Injector, Stealer, Worker};
use std::sync::Arc;
use std::time::Duration;

/// MEV strategy types
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MevStrategy {
    Backrun(String), // Config name for specific backrun strategy
}

/// Message to MEV search workers
#[derive(Clone)]
pub struct MevSearchTask {
    /// The flashblock state to search on
    pub state: FlashblockStateSnapshot,
    /// Which strategy to execute
    pub strategy: MevStrategy,
    /// When the flashblock was originally received
    pub flashblock_received_at: std::time::Instant,
}

/// Discovered MEV opportunity
#[derive(Debug, Clone)]
pub struct MevOpportunity {
    /// The flashblock state this was found on
    pub block_number: u64,
    pub flashblock_index: u32,
    /// The MEV bundle to execute
    pub bundle: MevBundle,
    /// Expected profit in wei
    pub expected_profit: U256,
    /// Strategy that found this
    pub strategy: String,
    /// Gas used from simulation (if available)
    pub simulated_gas_used: Option<u64>,
    /// Hash of the last transaction in the flashblock
    pub last_flashblock_tx_hash: Option<alloy_primitives::B256>,
    /// Scan ID to track this opportunity back to the trigger
    pub scan_id: String,
}

/// Work-stealing MEV search system optimized for high core counts
pub struct MevSearchSystem {
    /// Global work queue
    injector: Arc<Injector<MevSearchTask>>,
    /// Worker stealers for load balancing
    _stealers: Vec<Stealer<MevSearchTask>>,
    /// Channel for results
    _result_tx: mpsc::Sender<MevOpportunity>,
}

impl MevSearchSystem {
    /// Create a new MEV search system with specified number of workers
    pub fn new(num_workers: usize) -> (Self, mpsc::Receiver<MevOpportunity>) {
        let injector = Arc::new(Injector::new());
        let mut stealers = Vec::new();
        let mut workers = Vec::new();
        
        // Create worker queues
        for _ in 0..num_workers {
            let worker = Worker::new_fifo();
            stealers.push(worker.stealer());
            workers.push(worker);
        }
        
        let (result_tx, result_rx) = mpsc::channel(1000); // Larger buffer for 128 cores
        
        // Spawn worker threads
        for (worker_id, worker) in workers.into_iter().enumerate() {
            let injector = injector.clone();
            let stealers = stealers.clone();
            let result_tx = result_tx.clone();
            
            tokio::spawn(async move {
                mev_search_worker_steal(
                    worker_id,
                    worker,
                    injector,
                    stealers,
                    result_tx,
                ).await;
            });
        }
        
        let system = Self {
            injector,
            _stealers: stealers,
            _result_tx: result_tx,
        };
        
        (system, result_rx)
    }
    
    /// Submit a task to the work queue
    pub fn submit_task(&self, task: MevSearchTask) {
        self.injector.push(task);
    }
}

/// Worker that uses work-stealing for efficient task distribution
async fn mev_search_worker_steal(
    worker_id: usize,
    worker: Worker<MevSearchTask>,
    injector: Arc<Injector<MevSearchTask>>,
    stealers: Vec<Stealer<MevSearchTask>>,
    _result_tx: mpsc::Sender<MevOpportunity>,
) {
    println!("🔍 MEV Search Worker {} starting (work-stealing enabled)", worker_id);
    
    loop {
        // First, try to get work from local queue
        let task = worker.pop().or_else(|| {
            // Try global queue
            loop {
                match injector.steal() {
                    crossbeam::deque::Steal::Success(t) => return Some(t),
                    crossbeam::deque::Steal::Empty => break,
                    crossbeam::deque::Steal::Retry => continue,
                }
            }
            
            // Try stealing from other workers
            for (i, stealer) in stealers.iter().enumerate() {
                if i != worker_id {
                    loop {
                        match stealer.steal() {
                            crossbeam::deque::Steal::Success(t) => return Some(t),
                            crossbeam::deque::Steal::Empty => break,
                            crossbeam::deque::Steal::Retry => continue,
                        }
                    }
                }
            }
            
            None
        });
        
        if let Some(task) = task {
            let state = &task.state;
            
            // Calculate latency from flashblock receipt to now
            let latency_ms = task.flashblock_received_at.elapsed().as_secs_f64() * 1000.0;
            
            println!(
                "   ⏱️  Worker {} starting {} search on block {} fb {} (latency: {:.2}ms)",
                worker_id,
                match &task.strategy {
                    MevStrategy::Backrun(config) => config,
                },
                state.block_number,
                state.flashblock_index,
                latency_ms
            );
            
            // Execute only the specified strategy
            match task.strategy {
                MevStrategy::Backrun(_) => {
                    // Backrun handled by task workers with CacheDB
                    println!("   🏃 Backrun strategy should use task workers");
                }
            }
        } else {
            // No work available, sleep briefly
            tokio::time::sleep(Duration::from_micros(100)).await;
        }
    }
}


/// Known function selectors for MEV detection
mod selectors {
    // Chainlink oracle update functions (trigger liquidation checks)
    pub const CHAINLINK_LATEST_ANSWER: &[u8] = &[0x50, 0xd2, 0x5b, 0xcd]; // latestAnswer()
    pub const CHAINLINK_TRANSMIT: &[u8] = &[0x9a, 0x6f, 0xc8, 0xf5]; // transmit()
    pub const CHAINLINK_SUBMIT: &[u8] = &[0xc9, 0x80, 0x75, 0x39]; // submit()
    pub const CHAINLINK_FORWARD: &[u8] = &[0x6f, 0xad, 0xcf, 0x72]; // forward()
    
    // Common DEX swap functions (for arbitrage detection)
    pub const UNISWAP_V2_SWAP: &[u8] = &[0x02, 0x2c, 0x0d, 0x9f]; // swap()
    pub const UNISWAP_V3_SWAP: &[u8] = &[0x12, 0x8a, 0xca, 0xb4]; // swap()
    pub const UNISWAP_V3_MULTICALL: &[u8] = &[0xac, 0x96, 0x50, 0xd8]; // multicall()
    
    pub fn is_oracle_update(calldata: &[u8]) -> bool {
        calldata.starts_with(CHAINLINK_LATEST_ANSWER) ||
        calldata.starts_with(CHAINLINK_TRANSMIT) ||
        calldata.starts_with(CHAINLINK_SUBMIT) ||
        calldata.starts_with(CHAINLINK_FORWARD)
    }
    
    pub fn is_dex_swap(calldata: &[u8]) -> bool {
        calldata.starts_with(UNISWAP_V2_SWAP) ||
        calldata.starts_with(UNISWAP_V3_SWAP) ||
        calldata.starts_with(UNISWAP_V3_MULTICALL)
    }
}

/// Known protocol addresses on Base
mod base_addresses {
    use alloy_primitives::Address;
    
    // Uniswap V3 on Base
    pub const UNISWAP_V3_FACTORY: &str = "0x33128a8fC17869897dcE68Ed026d694621f6FDfD";
    pub const UNISWAP_V3_ROUTER: &str = "0x2626664c2603336E57B271c5C0b26F421741e481";
    
    // BaseSwap (Uniswap V2 fork)
    pub const BASESWAP_FACTORY: &str = "0xFDa619b6d20975be80A10332cD39b9a4b0FAa8BB";
    pub const BASESWAP_ROUTER: &str = "0x327Df1E6de05895d2ab08513aaDD9313Fe505d86";
    
    // Aerodrome (major DEX on Base)
    pub const AERODROME_ROUTER: &str = "0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43";
    
    // SushiSwap on Base
    pub const SUSHI_V3_FACTORY: &str = "0xb45e53277a7e0F1D35f2a77160e91e25507f1763";
    
    pub fn is_dex_address(addr: &Address) -> bool {
        let addr_str = format!("{:?}", addr);
        addr_str.contains(&UNISWAP_V3_FACTORY[2..]) ||
        addr_str.contains(&UNISWAP_V3_ROUTER[2..]) ||
        addr_str.contains(&BASESWAP_FACTORY[2..]) ||
        addr_str.contains(&BASESWAP_ROUTER[2..]) ||
        addr_str.contains(&AERODROME_ROUTER[2..]) ||
        addr_str.contains(&SUSHI_V3_FACTORY[2..])
    }
}

/// Analyze state changes to determine which MEV strategies to trigger
pub fn analyze_state_for_strategies(state: &FlashblockStateSnapshot) -> Vec<MevStrategy> {
    use std::collections::HashSet;
    use crate::backrun_analyzer::BackrunAnalyzer;
    
    let mut strategies = HashSet::new();
    
    // Check for backrun opportunities using BackrunAnalyzer
    let backrun_analyzer = BackrunAnalyzer::new(U256::from(10_000_000_000_000u64)); // 0.00001 ETH (10 microether) min profit
    let triggered_configs = backrun_analyzer.analyze_state_for_backrun(state);
    if !triggered_configs.is_empty() {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        println!("   🎯 [{}] Backrun analyzer triggered {} configs: {:?} (scanId: {})", 
            timestamp, triggered_configs.len(), triggered_configs, state.scan_id);
        // Create a separate strategy for each triggered config
        for config_name in triggered_configs {
            strategies.insert(MevStrategy::Backrun(config_name));
        }
    }
    
    
    
    // TODO: Re-enable when we implement liquidation strategies
    // Analyze transaction calldata for MEV signals
    /*
    for tx in &state.transactions {
        let calldata = tx.input();
        
        // Oracle updates trigger liquidation checks
        if calldata.len() >= 4 && selectors::is_oracle_update(calldata) {
            strategies.insert(MevStrategy::Liquidation);
        }
        
        // DEX swaps suggest arbitrage opportunities
        if calldata.len() >= 4 && selectors::is_dex_swap(calldata) {
            strategies.insert(MevStrategy::DexArbitrage);
        }
    }
    */
    
    // TODO: Re-enable when we implement other strategies
    /*
    // Fallback to mock logic if no real strategies triggered
    if strategies.is_empty() {
        // Use mock logic based on flashblock index
        if state.flashblock_index % 3 == 0 {
            strategies.insert(MevStrategy::DexArbitrage);
        }
        if state.flashblock_index % 5 == 0 {
            strategies.insert(MevStrategy::Liquidation);
        }
        if state.flashblock_index % 7 == 0 {
            strategies.insert(MevStrategy::Sandwich);
        }
        if state.flashblock_index % 11 == 0 {
            strategies.insert(MevStrategy::JitLiquidity);
        }
    }
    */
    
    strategies.into_iter().collect()
}

/// Create MEV search system optimized for server hardware
pub fn create_mev_search_system() -> (MevSearchSystem, mpsc::Receiver<MevOpportunity>) {
    // Determine optimal worker count based on CPU cores
    let num_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    
    // Use 80% of cores for MEV search, leave some for node operations
    let num_workers = (num_cores * 8 / 10).max(4);
    
    println!("🚀 Creating MEV search system with {} workers on {} cores", num_workers, num_cores);
    
    MevSearchSystem::new(num_workers)
}

// TODO: Real MEV strategies to implement:
// 1. DEX Arbitrage - Monitor price differences across Uniswap, SushiSwap, Curve, Balancer
// 2. Liquidations - Track undercollateralized positions on Aave, Compound, MakerDAO
// 3. Sandwich - Detect large swaps and wrap with front/back transactions
// 4. JIT Liquidity - Provide liquidity just before large trades, remove after
// 5. NFT Arbitrage - Find mispriced NFTs across OpenSea, Blur, LooksRare
// 6. Cross-chain Arbitrage - Price differences between Base and Ethereum
// 7. Backrun Oracle Updates - Trade after Chainlink price updates