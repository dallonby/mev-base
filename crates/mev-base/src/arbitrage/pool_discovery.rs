use alloy_primitives::{Address, U256, B256};
use std::collections::{HashMap, HashSet};
use tracing::{info, debug, warn};
use serde::{Deserialize, Serialize};

/// Pool discovery and indexing strategy for Base mainnet
/// 
/// Strategy Overview:
/// 1. Start with high-value token list (WETH, USDC, USDT, DAI, AERO, etc.)
/// 2. Monitor factory events for new pool creation
/// 3. Track pool metrics (liquidity, volume, fees collected)
/// 4. Build and maintain token connectivity graph
/// 5. Prioritize pools by profitability potential

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PoolInfo {
    pub address: Address,
    pub protocol: String,
    pub token0: Address,
    pub token1: Address,
    pub fee_tier: u32,
    pub liquidity_usd: f64,
    pub volume_24h_usd: f64,
    pub apr: f64,
    pub last_updated: u64,
}

#[derive(Clone, Debug)]
pub struct TokenInfo {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
    pub price_usd: f64,
    pub total_liquidity_usd: f64,
    pub is_stable: bool,
    pub is_verified: bool,
}

pub struct PoolDiscoveryStrategy {
    /// Known high-value tokens to prioritize
    priority_tokens: HashSet<Address>,
    /// All discovered pools indexed by pair
    pools_by_pair: HashMap<(Address, Address), Vec<PoolInfo>>,
    /// Token metadata
    token_info: HashMap<Address, TokenInfo>,
    /// Minimum liquidity threshold in USD
    min_liquidity_usd: f64,
    /// Minimum 24h volume in USD
    min_volume_24h_usd: f64,
}

impl PoolDiscoveryStrategy {
    pub fn new() -> Self {
        let mut priority_tokens = HashSet::new();
        
        // Base mainnet high-priority tokens
        // WETH
        priority_tokens.insert(Address::from([
            0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x06
        ]));
        
        // USDC
        priority_tokens.insert(Address::from([
            0x83, 0x3e, 0x89, 0xfc, 0xd6, 0xed, 0xb6, 0xe0,
            0x8f, 0x4c, 0x7c, 0x32, 0xd4, 0xf7, 0x1b, 0x54,
            0xbd, 0xa0, 0x29, 0x13
        ]));
        
        // USDbC (Bridged USDC)
        priority_tokens.insert(Address::from([
            0xd9, 0xaa, 0xec, 0x20, 0xac, 0xbd, 0x73, 0x22,
            0x55, 0x32, 0x73, 0x55, 0xc4, 0xc5, 0x92, 0x25,
            0xbf, 0x09, 0x60, 0x60
        ]));
        
        // DAI
        priority_tokens.insert(Address::from([
            0x50, 0xc5, 0x72, 0x5e, 0xf6, 0xf6, 0xa5, 0x01,
            0x03, 0x36, 0x88, 0xfb, 0x58, 0x83, 0x15, 0xb8,
            0x35, 0xea, 0x3e, 0x45
        ]));
        
        // AERO
        priority_tokens.insert(Address::from([
            0x94, 0x01, 0x81, 0xa9, 0x4a, 0x35, 0xa4, 0x56,
            0x9e, 0x45, 0x29, 0xa3, 0xcd, 0xfb, 0x74, 0xe3,
            0x8f, 0xd9, 0x86, 0x31
        ]));
        
        Self {
            priority_tokens,
            pools_by_pair: HashMap::new(),
            token_info: HashMap::new(),
            min_liquidity_usd: 10_000.0,  // $10k minimum
            min_volume_24h_usd: 1_000.0,   // $1k daily volume minimum
        }
    }
    
    /// Phase 1: Initial Discovery
    /// Scan factory contracts for existing pools with priority tokens
    pub async fn discover_initial_pools(&mut self) {
        info!("Starting initial pool discovery");
        
        // 1. Query UniswapV2 factory for all pairs with priority tokens
        // 2. Query UniswapV3 factory for pools with priority tokens  
        // 3. Query Aerodrome factory for pools with priority tokens
        
        // For each discovered pool:
        // - Get current reserves/liquidity
        // - Calculate USD value
        // - Store if above thresholds
    }
    
    /// Phase 2: Graph Building
    /// Build token connectivity graph from discovered pools
    pub fn build_token_graph(&self) -> HashMap<Address, HashSet<Address>> {
        let mut graph = HashMap::new();
        
        for ((token0, token1), pools) in &self.pools_by_pair {
            // Only include pools meeting our criteria
            let viable_pools: Vec<_> = pools.iter()
                .filter(|p| p.liquidity_usd >= self.min_liquidity_usd)
                .filter(|p| p.volume_24h_usd >= self.min_volume_24h_usd)
                .collect();
            
            if !viable_pools.is_empty() {
                graph.entry(*token0)
                    .or_insert_with(HashSet::new)
                    .insert(*token1);
                graph.entry(*token1)
                    .or_insert_with(HashSet::new)
                    .insert(*token0);
            }
        }
        
        graph
    }
    
    /// Phase 3: Path Optimization
    /// Find optimal paths considering liquidity and gas costs
    pub fn find_optimal_paths(&self, start_token: Address, max_hops: usize) -> Vec<Vec<Address>> {
        let mut paths = Vec::new();
        let graph = self.build_token_graph();
        
        // Priority 1: Direct paths through WETH or stablecoins
        // Priority 2: Paths through high-liquidity tokens
        // Priority 3: Other viable paths
        
        // Use modified BFS to find paths weighted by liquidity
        self.bfs_weighted_paths(start_token, &graph, max_hops, &mut paths);
        
        paths
    }
    
    fn bfs_weighted_paths(
        &self,
        start: Address,
        graph: &HashMap<Address, HashSet<Address>>,
        max_hops: usize,
        paths: &mut Vec<Vec<Address>>
    ) {
        #[derive(Clone)]
        struct PathState {
            path: Vec<Address>,
            total_liquidity: f64,
            visited: HashSet<Address>,
        }
        
        let mut queue = vec![PathState {
            path: vec![start],
            total_liquidity: 0.0,
            visited: HashSet::from([start]),
        }];
        
        while let Some(state) = queue.pop() {
            let current = *state.path.last().unwrap();
            
            // Check if we've completed a cycle
            if state.path.len() > 2 && current == start {
                paths.push(state.path);
                continue;
            }
            
            // Don't exceed max hops
            if state.path.len() >= max_hops + 1 {
                continue;
            }
            
            // Explore neighbors
            if let Some(neighbors) = graph.get(&current) {
                for &neighbor in neighbors {
                    // Allow returning to start to complete cycle
                    if neighbor == start && state.path.len() >= 3 {
                        let mut new_path = state.path.clone();
                        new_path.push(neighbor);
                        paths.push(new_path);
                    }
                    // Or explore new tokens
                    else if !state.visited.contains(&neighbor) {
                        let mut new_state = state.clone();
                        new_state.path.push(neighbor);
                        new_state.visited.insert(neighbor);
                        
                        // Add liquidity score for prioritization
                        if let Some(pools) = self.pools_by_pair.get(&Self::ordered_pair(current, neighbor)) {
                            if let Some(best_pool) = pools.iter().max_by(|a, b| 
                                a.liquidity_usd.partial_cmp(&b.liquidity_usd).unwrap()
                            ) {
                                new_state.total_liquidity += best_pool.liquidity_usd;
                            }
                        }
                        
                        queue.push(new_state);
                    }
                }
            }
        }
        
        // Sort paths by total liquidity
        paths.sort_by(|a, b| {
            let liquidity_a = self.calculate_path_liquidity(a);
            let liquidity_b = self.calculate_path_liquidity(b);
            liquidity_b.partial_cmp(&liquidity_a).unwrap()
        });
    }
    
    fn calculate_path_liquidity(&self, path: &[Address]) -> f64 {
        let mut total = 0.0;
        for i in 0..path.len() - 1 {
            if let Some(pools) = self.pools_by_pair.get(&Self::ordered_pair(path[i], path[i + 1])) {
                if let Some(best) = pools.iter().max_by(|a, b| 
                    a.liquidity_usd.partial_cmp(&b.liquidity_usd).unwrap()
                ) {
                    total += best.liquidity_usd;
                }
            }
        }
        total
    }
    
    fn ordered_pair(a: Address, b: Address) -> (Address, Address) {
        if a < b { (a, b) } else { (b, a) }
    }
    
    /// Phase 4: Dynamic Updates
    /// Monitor and update pool states in real-time
    pub async fn update_pool_states(&mut self) {
        debug!("Updating pool states");
        
        // Priority order for updates:
        // 1. Pools in active arbitrage paths
        // 2. High-volume pools
        // 3. Pools with priority tokens
        // 4. Recently active pools
    }
    
    /// Pool Ranking Algorithm
    pub fn rank_pools(&self) -> Vec<(Address, f64)> {
        let mut rankings = Vec::new();
        
        for (_, pools) in &self.pools_by_pair {
            for pool in pools {
                // Scoring factors:
                // - Liquidity (40%)
                // - Volume (30%)  
                // - Fee APR (20%)
                // - Token quality (10%)
                
                let liquidity_score = (pool.liquidity_usd / 1_000_000.0).min(1.0) * 0.4;
                let volume_score = (pool.volume_24h_usd / 100_000.0).min(1.0) * 0.3;
                let apr_score = (pool.apr / 100.0).min(1.0) * 0.2;
                
                let token_score = {
                    let t0_priority = self.priority_tokens.contains(&pool.token0) as u8 as f64;
                    let t1_priority = self.priority_tokens.contains(&pool.token1) as u8 as f64;
                    ((t0_priority + t1_priority) / 2.0) * 0.1
                };
                
                let total_score = liquidity_score + volume_score + apr_score + token_score;
                rankings.push((pool.address, total_score));
            }
        }
        
        rankings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        rankings
    }
}

/// Pool monitoring strategy
pub struct PoolMonitor {
    /// Pools to monitor for arbitrage
    pub watched_pools: HashSet<Address>,
    /// Update frequency in blocks
    pub update_frequency: u64,
    /// Last update block for each pool
    pub last_updates: HashMap<Address, u64>,
}

impl PoolMonitor {
    pub fn new() -> Self {
        Self {
            watched_pools: HashSet::new(),
            update_frequency: 1, // Every block for hot pools
            last_updates: HashMap::new(),
        }
    }
    
    pub fn should_update(&self, pool: Address, current_block: u64) -> bool {
        match self.last_updates.get(&pool) {
            Some(&last) => current_block >= last + self.update_frequency,
            None => true,
        }
    }
    
    pub fn add_hot_pool(&mut self, pool: Address) {
        self.watched_pools.insert(pool);
        info!("Added hot pool to monitor: {:?}", pool);
    }
    
    pub fn remove_cold_pool(&mut self, pool: Address) {
        self.watched_pools.remove(&pool);
        debug!("Removed cold pool from monitor: {:?}", pool);
    }
}