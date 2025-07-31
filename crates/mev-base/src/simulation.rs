use alloy_rpc_types_eth::{BlockId, BlockOverrides, state::EvmOverrides};
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