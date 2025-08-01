use reth_provider::StateProviderFactory;
use reth_revm::{database::StateProviderDatabase, db::CacheDB};
use reth_optimism_chainspec::OpChainSpec;
use std::sync::Arc;
use std::time::Instant;

use crate::flashblock_state::FlashblockStateSnapshot;

/// Benchmark the overhead of spawning a worker, creating CacheDB, and applying state
pub async fn benchmark_worker_overhead<P>(
    _chain_spec: Arc<OpChainSpec>,
    provider: P,
    state_snapshot: FlashblockStateSnapshot,
    num_iterations: usize,
) -> eyre::Result<()>
where
    P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader + Clone + Send + Sync + 'static,
    P::Header: alloy_consensus::BlockHeader,
{
    println!("\n📊 Benchmarking MEV Task Worker Overhead ({} iterations)", num_iterations);
    println!("   State snapshot size: {} accounts, {} storage, {} contracts",
        state_snapshot.account_changes.len(),
        state_snapshot.storage_changes.values().map(|s| s.len()).sum::<usize>(),
        state_snapshot.code_changes.len()
    );
    
    let mut timings = Vec::new();
    
    for i in 0..num_iterations {
        let start = Instant::now();
        
        // Measure just the overhead - spawn a task that returns immediately
        let provider_clone = provider.clone();
        let state_clone = state_snapshot.clone();
        
        let task_handle = tokio::spawn(async move {
            let inner_start = Instant::now();
            
            // Get state provider
            let provider_start = Instant::now();
            let state_provider = provider_clone.latest()?;
            let provider_time = provider_start.elapsed().as_secs_f64() * 1000.0;
            
            // Get block header
            let header_start = Instant::now();
            let _header = provider_clone.header_by_number(provider_clone.best_block_number()?)?
                .ok_or_else(|| eyre::eyre!("Header not found"))?;
            let header_time = header_start.elapsed().as_secs_f64() * 1000.0;
            
            // Create CacheDB
            let cache_start = Instant::now();
            let mut cache_db = CacheDB::new(StateProviderDatabase::new(state_provider));
            let cache_time = cache_start.elapsed().as_secs_f64() * 1000.0;
            
            // Apply state snapshot
            let apply_start = Instant::now();
            
            // Apply account changes
            for (address, account_info) in &state_clone.account_changes {
                use revm::database::{DbAccount, AccountState};
                
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
            for (address, storage_changes) in &state_clone.storage_changes {
                if let Some(db_account) = cache_db.cache.accounts.get_mut(address) {
                    for (slot, value) in storage_changes {
                        db_account.storage.insert(*slot, *value);
                    }
                }
            }
            
            // Apply code changes
            for (code_hash, bytecode) in &state_clone.code_changes {
                cache_db.cache.contracts.insert(*code_hash, bytecode.clone());
            }
            
            let apply_time = apply_start.elapsed().as_secs_f64() * 1000.0;
            
            let total_inner = inner_start.elapsed().as_secs_f64() * 1000.0;
            
            Ok::<_, eyre::Error>((provider_time, header_time, cache_time, apply_time, total_inner))
        });
        
        let spawn_time = start.elapsed().as_secs_f64() * 1000.0;
        
        match task_handle.await {
            Ok(Ok((provider_time, header_time, cache_time, apply_time, total_inner))) => {
                let total_time = start.elapsed().as_secs_f64() * 1000.0;
                timings.push((spawn_time, provider_time, header_time, cache_time, apply_time, total_inner, total_time));
                
                if i == 0 {
                    println!("\n   First iteration breakdown:");
                    println!("      ├─ Task spawn: {:.2}ms", spawn_time);
                    println!("      ├─ State provider: {:.2}ms", provider_time);
                    println!("      ├─ Block header: {:.2}ms", header_time);
                    println!("      ├─ CacheDB creation: {:.2}ms", cache_time);
                    println!("      ├─ Apply snapshot: {:.2}ms", apply_time);
                    println!("      ├─ Total inner: {:.2}ms", total_inner);
                    println!("      └─ Total (spawn + exec): {:.2}ms", total_time);
                }
            }
            Ok(Err(e)) => {
                println!("   ❌ Task error: {:?}", e);
            }
            Err(e) => {
                println!("   ❌ Join error: {:?}", e);
            }
        }
    }
    
    // Calculate statistics
    if !timings.is_empty() {
        let total_times: Vec<f64> = timings.iter().map(|t| t.6).collect();
        let inner_times: Vec<f64> = timings.iter().map(|t| t.5).collect();
        let spawn_times: Vec<f64> = timings.iter().map(|t| t.0).collect();
        let provider_times: Vec<f64> = timings.iter().map(|t| t.1).collect();
        let apply_times: Vec<f64> = timings.iter().map(|t| t.4).collect();
        
        let avg_total = total_times.iter().sum::<f64>() / total_times.len() as f64;
        let avg_inner = inner_times.iter().sum::<f64>() / inner_times.len() as f64;
        let avg_spawn = spawn_times.iter().sum::<f64>() / spawn_times.len() as f64;
        let avg_provider = provider_times.iter().sum::<f64>() / provider_times.len() as f64;
        let avg_apply = apply_times.iter().sum::<f64>() / apply_times.len() as f64;
        
        let min_total = total_times.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_total = total_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        
        println!("\n   📈 Benchmark Results ({} iterations):", num_iterations);
        println!("      ├─ Average total time: {:.2}ms", avg_total);
        println!("      ├─ Average task spawn: {:.2}ms", avg_spawn);
        println!("      ├─ Average state provider: {:.2}ms", avg_provider);
        println!("      ├─ Average apply snapshot: {:.2}ms", avg_apply);
        println!("      ├─ Average inner execution: {:.2}ms", avg_inner);
        println!("      ├─ Min total time: {:.2}ms", min_total);
        println!("      └─ Max total time: {:.2}ms", max_total);
        
        // Estimate throughput
        let tasks_per_second = 1000.0 / avg_total;
        println!("\n   💫 Estimated throughput: {:.0} tasks/second", tasks_per_second);
        println!("      └─ For 100 parallel tasks: {:.2}ms overhead per batch", avg_total);
    }
    
    Ok(())
}