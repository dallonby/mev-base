use alloy_rpc_types_eth::{BlockId, BlockOverrides, state::{EvmOverrides, StateOverride}, Bundle, StateContext, EthCallResponse};
use alloy_primitives::U256;
use futures::future::join_all;
use reth_rpc_eth_api::{helpers::EthCall, EthApiTypes, RpcTypes};
use std::time::Instant;

/// Simulates a batch of transactions with the given parameters
pub async fn simulate_transaction_batch<EthApi>(
    eth_api: &EthApi,
    transaction: <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest,
    target_block: BlockId,
    batch_size: usize,
    base_fee_override: Option<U256>,
    block_timestamp_override: Option<u64>,
) -> eyre::Result<()>
where
    EthApi: EthCall + Clone + Send + Sync + 'static,
    <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest: Clone + Send + Sync,
{
    println!("\n🔬 Starting batch simulation of {} transactions...", batch_size);
    let batch_start = Instant::now();
    
    // Create futures for all transactions
    let mut futures = Vec::with_capacity(batch_size);
    
    for _ in 0..batch_size {
        let tx_request = transaction.clone();
        let eth_api_clone = eth_api.clone();
        let target_block_clone = target_block.clone();
        
        // Create the overrides
        let mut overrides = EvmOverrides::default();
        if base_fee_override.is_some() || block_timestamp_override.is_some() {
            let mut block_overrides = BlockOverrides::default();
            if let Some(base_fee) = base_fee_override {
                block_overrides.base_fee = Some(base_fee);
            }
            if let Some(timestamp) = block_timestamp_override {
                block_overrides.time = Some(timestamp.into());
            }
            overrides.block = Some(Box::new(block_overrides));
        }
        
        let future = tokio::task::spawn(async move {
            eth_api_clone.call(tx_request, Some(target_block_clone), overrides).await
        });
        futures.push(future);
    }
    
    // Execute all simulations in parallel
    let results = join_all(futures).await;
    
    // Count results (handle both spawn errors and call errors)
    let mut successful = 0;
    let mut failed = 0;
    let mut sample_result = None;
    let mut sample_error = None;
    
    for result in results {
        match result {
            Ok(Ok(data)) => {
                successful += 1;
                if sample_result.is_none() && !data.is_empty() {
                    sample_result = Some(data);
                }
            }
            Ok(Err(e)) => {
                failed += 1;
                if sample_error.is_none() {
                    sample_error = Some(e.to_string());
                }
            }
            Err(e) => {
                failed += 1;
                println!("   ├─ Task spawn error: {}", e);
            }
        }
    }
    
    // Print sample result or error
    if let Some(data) = sample_result {
        println!("   ├─ Sample return data: 0x{}", hex::encode(&data));
    }
    if let Some(error) = sample_error {
        println!("   ├─ Sample error: {}", error);
    }
    
    let batch_elapsed = batch_start.elapsed();
    println!("✅ Batch simulation complete!");
    println!("   ├─ Successful: {}", successful);
    println!("   ├─ Failed: {}", failed);
    println!("   ├─ Total time: {:.2}ms", batch_elapsed.as_secs_f64() * 1000.0);
    println!("   └─ Avg per tx: {:.2}ms", (batch_elapsed.as_secs_f64() * 1000.0) / batch_size as f64);
    
    Ok(())
}

/// Simulates a bundle of transactions together using eth_callMany
/// 
/// This simulates multiple transactions in sequence, where each transaction
/// sees the state changes from previous transactions in the bundle.
/// 
/// # Arguments
/// * `eth_api` - The Ethereum API instance
/// * `transactions` - Array of transactions to simulate (can be signed or unsigned)
/// * `target_block` - The block to simulate against
/// * `base_fee_override` - Optional base fee override
/// * `block_timestamp_override` - Optional timestamp override
/// * `state_override` - Optional state overrides to apply before simulation
pub async fn simulate_bundle<EthApi>(
    eth_api: &EthApi,
    transactions: Vec<<<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest>,
    target_block: BlockId,
    base_fee_override: Option<U256>,
    block_timestamp_override: Option<u64>,
    state_override: Option<StateOverride>,
) -> eyre::Result<Vec<Vec<EthCallResponse>>>
where
    EthApi: EthCall + Clone + Send + Sync + 'static,
    <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest: Clone + Send + Sync,
{
    println!("\n🎯 Starting bundle simulation of {} transactions...", transactions.len());
    let bundle_start = Instant::now();
    
    // Create block overrides if needed
    let mut block_override = None;
    if base_fee_override.is_some() || block_timestamp_override.is_some() {
        let mut overrides = BlockOverrides::default();
        if let Some(base_fee) = base_fee_override {
            overrides.base_fee = Some(base_fee);
        }
        if let Some(timestamp) = block_timestamp_override {
            overrides.time = Some(timestamp.into());
        }
        block_override = Some(overrides);
    }
    
    // Create the bundle
    let bundle = Bundle {
        transactions,
        block_override,
    };
    
    // Create state context for the target block
    let state_context = Some(StateContext {
        block_number: Some(target_block),
        transaction_index: None, // Simulate all transactions in the block
    });
    
    // Call the bundle simulation
    match eth_api.call_many(vec![bundle], state_context, state_override).await {
        Ok(results) => {
            let bundle_elapsed = bundle_start.elapsed();
            
            // Process results
            let mut total_gas_used = 0u64;
            let mut successful = 0;
            let mut failed = 0;
            
            if let Some(bundle_results) = results.first() {
                for (i, result) in bundle_results.iter().enumerate() {
                    if let Some(error) = &result.error {
                        failed += 1;
                        println!("   ├─ Tx {}: ❌ Error: {}", i, error);
                    } else {
                        successful += 1;
                        if let Some(gas_used) = result.gas_used {
                            total_gas_used += gas_used;
                        }
                        if let Some(value) = &result.value {
                            if !value.is_empty() {
                                println!("   ├─ Tx {}: ✅ Gas: {}, Return: 0x{}", 
                                    i, 
                                    result.gas_used.unwrap_or(0),
                                    hex::encode(&value[..value.len().min(32)])
                                );
                            } else {
                                println!("   ├─ Tx {}: ✅ Gas: {}", i, result.gas_used.unwrap_or(0));
                            }
                        } else {
                            println!("   ├─ Tx {}: ✅ Gas: {}", i, result.gas_used.unwrap_or(0));
                        }
                    }
                }
            }
            
            println!("✅ Bundle simulation complete!");
            println!("   ├─ Successful: {}", successful);
            println!("   ├─ Failed: {}", failed);
            println!("   ├─ Total gas: {}", total_gas_used);
            println!("   ├─ Total time: {:.2}ms", bundle_elapsed.as_secs_f64() * 1000.0);
            if successful > 0 {
                println!("   └─ Avg gas per tx: {}", total_gas_used / successful as u64);
            }
            
            Ok(results)
        }
        Err(e) => {
            println!("❌ Bundle simulation failed: {:?}", e);
            Err(eyre::eyre!("Bundle simulation failed: {:?}", e))
        }
    }
}