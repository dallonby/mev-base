
use reth_optimism_node::{
    node::OpAddOns,
    OpNode,
};
use reth_optimism_cli::Cli;
use reth_provider::ReceiptProvider;

use futures::TryStreamExt;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use alloy_rpc_types_eth::TransactionRequest;
use alloy_primitives::{Address, U256, TxKind};
use std::str::FromStr;
use reth_rpc_eth_api::helpers::EthCall;
use futures::future::join_all;

/// Block subscriber ExEx that echoes block numbers
async fn block_subscriber_exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
) -> eyre::Result<()> 
{
    println!("Block subscriber ExEx started!");
    
    // Access the provider for RPC-like operations
    let provider = ctx.provider().clone();
    
    // Subscribe to chain state notifications
    while let Some(notification) = ctx.notifications.try_next().await? {
        match &notification {
            ExExNotification::ChainCommitted { new } => {
                // New blocks committed to the canonical chain
                let tip = new.tip();
                let range = new.range();
                println!("🔷 New blocks committed: {} -> {} ({}) [{} blocks]", 
                    range.start(), 
                    range.end(), 
                    tip.hash(),
                    range.clone().count()
                );
                
                // Example: Access additional block data via provider
                if let Ok(receipts) = provider.receipts_by_block(tip.hash().into()) {
                    if let Some(receipts) = receipts {
                        println!("   └─ Transactions in last block: {}", receipts.len());
                    }
                }
            }
            ExExNotification::ChainReorged { old, new } => {
                println!("⚠️  Chain reorg: {:?} -> {:?}", old.range(), new.range());
            }
            ExExNotification::ChainReverted { old } => {
                println!("⏪ Chain reverted: {:?}", old.range());
            }
        }

        // Signal that we've processed up to this height for pruning
        // This allows Reth to prune the WAL and old blocks
        if let Some(committed_chain) = notification.committed_chain() {
            // log a message here
            let num_hash = committed_chain.tip().num_hash();
            println!("✅ Processed up to height: #{} ({})", num_hash.number, num_hash.hash);
            ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
        }
    }

    Ok(())
}

fn main() -> eyre::Result<()> {
    Cli::parse_args().run(|builder, rollup_args| async move {
        let node = OpNode::new(rollup_args.clone());
        let handle = builder
            .with_types::<OpNode>()
            .with_components(node.components())
            .with_add_ons(OpAddOns::default())
            .on_node_started(|_full_node| {
                println!("Node started successfully!");
                Ok(())
            })
            .on_rpc_started(|_ctx, _handles| {
                println!("RPC server started!");
                Ok(())
            })
            .install_exex("block-echo", move |ctx| {
                async move { Ok(block_subscriber_exex(ctx)) }
            })
            .launch()
            .await?;

        // Get the eth API from the launched node and clone it for the spawned task
        let eth_api = handle.node.add_ons_handle.eth_api().clone();
        
        // Spawn a task to simulate calls every 2 seconds
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            
            loop {
                interval.tick().await;
                
                println!("\n🔬 Starting batch simulation of 512 transactions...");
                let batch_start = std::time::Instant::now();
                
                let from_addr = Address::from_str("0xd0ffEe48945a9518b0B543a2C59dFb102221fBb7").unwrap();
                let to_addr = Address::from_str("0x38cef6277942faf66b9cd9f1b5132d68ba175b32").unwrap();
                
                // Create futures for all 512 transactions
                let mut futures = Vec::with_capacity(512);
                
                for _ in 0..512 {
                    let tx_request = TransactionRequest {
                        from: Some(from_addr),
                        to: Some(TxKind::Call(to_addr)),
                        value: Some(U256::from(0)),
                        // msg.input = "0x73eab4900000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000012c00000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000001dc8cff000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000753000000000000000000000000038cef6277942faf66b9cd9f1b5132d68ba175b3200000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000"
                        input: Some("0x73eab4900000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000012c00000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000001dc8cff000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000753000000000000000000000000038cef6277942faf66b9cd9f1b5132d68ba175b3200000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000".into()),
                        // input: vec![0x00, 0x00, 0x00, 0x01].into(),
                        ..Default::default()
                    };
                    
                    // Create future for simulation
                    let eth_api_clone = eth_api.clone();
                    let future = tokio::task::spawn(async move {
                        eth_api_clone.call(tx_request.into(), None, Default::default()).await
                    });
                    futures.push(future);
                }
                
                // Execute all simulations in parallel
                let results = join_all(futures).await;
                
                // Count results (handle both spawn errors and call errors)
                let mut successful = 0;
                let mut failed = 0;
                let mut sample_error = None;
                
                for result in results {
                    match result {
                        Ok(Ok(data)) => {
                            successful += 1;
                            println!("   ├─ Unexpected success! Return data: 0x{}", hex::encode(&data));
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
                
                // Print a sample error/revert reason
                if let Some(error) = sample_error {
                    println!("   ├─ Sample revert reason: {}", error);
                }
                
                let batch_elapsed = batch_start.elapsed();
                println!("✅ Batch simulation complete!");
                println!("   ├─ Successful: {}", successful);
                println!("   ├─ Failed: {}", failed);
                println!("   ├─ Total time: {:.2}ms", batch_elapsed.as_secs_f64() * 1000.0);
                println!("   └─ Avg per tx: {:.2}ms", (batch_elapsed.as_secs_f64() * 1000.0) / 512.0);
            }
        });

        handle.wait_for_node_exit().await
    })
}