
use reth_optimism_node::{
    node::OpAddOns,
    OpNode,
};
use reth_optimism_cli::Cli;
use reth_provider::ReceiptProvider;

use futures::TryStreamExt;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use alloy_rpc_types_eth::{TransactionRequest, BlockId};
use alloy_primitives::{Address, U256, TxKind};
use std::str::FromStr;

mod simulation;

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
                
                let from_addr = Address::from_str("0xd0ffEe48945a9518b0B543a2C59dFb102221fBb7").unwrap();
                let to_addr = Address::from_str("0x38cef6277942faf66b9cd9f1b5132d68ba175b32").unwrap();
                
                // Create the transaction request
                let tx_request = TransactionRequest {
                    from: Some(from_addr),
                    to: Some(TxKind::Call(to_addr)),
                    value: Some(U256::from(0)),
                    gas: Some(1_000_000_000), // 1 billion gas
                    input: hex::decode("73eab4900000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000012c00000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000001dc8cff000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000753000000000000000000000000038cef6277942faf66b9cd9f1b5132d68ba175b3200000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000").unwrap().into(),
                    ..Default::default()
                };
                
                // Call the simulation function
                if let Err(e) = simulation::simulate_transaction_batch(
                    &eth_api,
                    tx_request.into(),  // convert to the API's transaction type
                    BlockId::latest(),  // target block
                    512,                // batch size
                    None,               // no base fee override
                    None,               // no timestamp override
                ).await {
                    println!("❌ Simulation batch failed: {:?}", e);
                }
            }
        });

        handle.wait_for_node_exit().await
    })
}