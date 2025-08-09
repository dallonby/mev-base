use alloy_primitives::{Address, U256};
use std::collections::{HashMap, HashSet};
use crate::arbitrage::dex::{DexPool, DexProtocol, SwapRoute};

#[derive(Clone, Debug)]
pub struct ArbitragePath {
    pub route: SwapRoute,
    pub profit: U256,
    pub gas_cost: U256,
    pub net_profit: U256,
}

pub struct PathFinder {
    pools: HashMap<(Address, Address), Vec<DexPool>>,
    token_graph: HashMap<Address, HashSet<Address>>,
    max_hops: usize,
}

impl PathFinder {
    pub fn new(max_hops: usize) -> Self {
        Self {
            pools: HashMap::new(),
            token_graph: HashMap::new(),
            max_hops,
        }
    }
    
    pub fn add_pool(&mut self, pool: DexPool) {
        let key = if pool.token0 < pool.token1 {
            (pool.token0, pool.token1)
        } else {
            (pool.token1, pool.token0)
        };
        
        self.pools.entry(key).or_insert_with(Vec::new).push(pool.clone());
        
        self.token_graph.entry(pool.token0)
            .or_insert_with(HashSet::new)
            .insert(pool.token1);
        self.token_graph.entry(pool.token1)
            .or_insert_with(HashSet::new)
            .insert(pool.token0);
    }
    
    pub fn find_arbitrage_paths(
        &self,
        start_token: Address,
        amount_in: U256,
        gas_price: U256,
    ) -> Vec<ArbitragePath> {
        let mut paths = Vec::new();
        
        // Find all cycles starting and ending with start_token
        let cycles = self.find_cycles(start_token);
        
        for cycle in cycles {
            if let Some(route) = self.build_route(&cycle, amount_in) {
                let gas_cost = gas_price * U256::from(route.gas_estimate);
                let profit = if route.expected_out > amount_in {
                    route.expected_out - amount_in
                } else {
                    U256::ZERO
                };
                
                if profit > gas_cost {
                    paths.push(ArbitragePath {
                        route,
                        profit,
                        gas_cost,
                        net_profit: profit - gas_cost,
                    });
                }
            }
        }
        
        // Sort by net profit descending
        paths.sort_by(|a, b| b.net_profit.cmp(&a.net_profit));
        paths
    }
    
    fn find_cycles(&self, start_token: Address) -> Vec<Vec<Address>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut path = vec![start_token];
        
        self.dfs_cycles(
            start_token,
            start_token,
            &mut visited,
            &mut path,
            &mut cycles,
            0,
        );
        
        cycles
    }
    
    fn dfs_cycles(
        &self,
        start: Address,
        current: Address,
        visited: &mut HashSet<Address>,
        path: &mut Vec<Address>,
        cycles: &mut Vec<Vec<Address>>,
        depth: usize,
    ) {
        if depth > 0 && depth <= self.max_hops && current == start {
            cycles.push(path.clone());
            return;
        }
        
        if depth >= self.max_hops {
            return;
        }
        
        if let Some(neighbors) = self.token_graph.get(&current) {
            for &neighbor in neighbors {
                if depth == 0 || !visited.contains(&neighbor) || (neighbor == start && depth >= 2) {
                    visited.insert(neighbor);
                    path.push(neighbor);
                    
                    self.dfs_cycles(start, neighbor, visited, path, cycles, depth + 1);
                    
                    path.pop();
                    if neighbor != start {
                        visited.remove(&neighbor);
                    }
                }
            }
        }
    }
    
    fn build_route(&self, token_path: &[Address], amount_in: U256) -> Option<SwapRoute> {
        let mut pools = Vec::new();
        let mut current_amount = amount_in;
        let mut gas_estimate = 0u64;
        
        for i in 0..token_path.len() - 1 {
            let token_in = token_path[i];
            let token_out = token_path[i + 1];
            
            let key = if token_in < token_out {
                (token_in, token_out)
            } else {
                (token_out, token_in)
            };
            
            let available_pools = self.pools.get(&key)?;
            if available_pools.is_empty() {
                return None;
            }
            
            // Select best pool for this hop (highest output)
            let mut best_pool = None;
            let mut best_output = U256::ZERO;
            
            for pool in available_pools {
                let output = self.calculate_output(pool, token_in, current_amount);
                if output > best_output {
                    best_output = output;
                    best_pool = Some(pool.clone());
                }
            }
            
            let pool = best_pool?;
            gas_estimate += self.estimate_gas_for_pool(&pool);
            pools.push(pool);
            current_amount = best_output;
        }
        
        Some(SwapRoute {
            pools,
            token_path: token_path.to_vec(),
            amount_in,
            expected_out: current_amount,
            gas_estimate,
        })
    }
    
    fn calculate_output(&self, pool: &DexPool, token_in: Address, amount_in: U256) -> U256 {
        let (reserve_in, reserve_out) = if token_in == pool.token0 {
            (pool.reserve0, pool.reserve1)
        } else {
            (pool.reserve1, pool.reserve0)
        };
        
        if reserve_in == U256::ZERO || reserve_out == U256::ZERO {
            return U256::ZERO;
        }
        
        // UniswapV2-style calculation
        let fee_multiplier = U256::from(10000 - pool.fee as u64);
        let amount_in_with_fee = amount_in * fee_multiplier / U256::from(10000);
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = reserve_in + amount_in_with_fee;
        
        numerator / denominator
    }
    
    fn estimate_gas_for_pool(&self, pool: &DexPool) -> u64 {
        match pool.protocol {
            DexProtocol::UniswapV2 => 100_000,
            DexProtocol::UniswapV3 => 150_000,
            DexProtocol::UniswapV4 => 120_000,
            DexProtocol::Aerodrome => 110_000,
        }
    }
}