
use reth_optimism_node::{
    node::OpAddOns,
    OpNode,
};
use reth_optimism_cli::Cli;
use reth_provider::ReceiptProvider;
use reth_optimism_chainspec::BASE_MAINNET;
use alloy_rpc_types_eth::BlockId;

use futures::TryStreamExt;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use std::sync::Arc;
use tokio::sync::Mutex;

mod simulation;
mod flashblocks;
mod flashblock_accumulator;
mod revm_flashblock_executor;
mod revm_flashblock_manager;
// mod stateful_simulation;  
// mod cached_simulation;
mod in_memory_flashblock_state;

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
        
        // Get the provider from the node for revm executor
        let blockchain_provider = handle.node.provider().clone();
        
        // Start flashblocks client
        let mut flashblocks_client = flashblocks::FlashblocksClient::new(
            "wss://mainnet.flashblocks.base.org/ws".to_string(),
            4096, // event buffer size
        );
        
        // Subscribe to flashblocks events
        let mut flashblocks_receiver = flashblocks_client.subscribe();
        
        // Start the flashblocks connection
        flashblocks_client.start().await?;
        
        println!("🔌 Flashblocks client connected to wss://mainnet.flashblocks.base.org/ws");
        
        // Create flashblock manager (RPC-based for comparison)
        let flashblock_manager = Arc::new(Mutex::new(
            flashblock_accumulator::FlashblockManager::new(
                eth_api.clone(),
                11,  // max flashblocks per block (indices 0-10)
                5,   // keep last 5 blocks in memory
            )
        ));
        
        // Flag to use revm-based executor instead of RPC
        let use_revm_executor = true;
        
        // Create a channel for flashblock processing queue
        let (flashblock_tx, mut flashblock_rx) = tokio::sync::mpsc::channel(100);
        
        // Spawn task to receive flashblocks and queue them
        tokio::spawn(async move {
            while let Ok(event) = flashblocks_receiver.recv().await {
                println!("\n📦 Flashblocks Event:");
                println!("   ├─ Block: {}", event.block_number);
                println!("   ├─ Index: {}", event.index);
                println!("   ├─ Transactions: {}", event.transactions.len());
                println!("   ├─ State Root: {}", event.state_root);
                println!("   └─ Receipts Root: {}", event.receipts_root);
                
                // Queue the event for processing
                if let Err(e) = flashblock_tx.send(event).await {
                    println!("❌ Failed to queue flashblock: {}", e);
                }
            }
        });
        
        // Clone provider for the spawned task
        let blockchain_provider_for_task = blockchain_provider.clone();
        
        // Spawn dedicated synchronous flashblock simulator thread
        tokio::spawn(async move {
            println!("🚀 Starting dedicated flashblock simulator thread");
            
            // Create revm executor with the node's provider
            let chain_spec = BASE_MAINNET.clone();
            let mut revm_executor = revm_flashblock_executor::RevmFlashblockExecutor::new(chain_spec);
            let mut revm_initialized = false;
            let mut current_block = 0u64;
            
            while let Some(event) = flashblock_rx.recv().await {
                let sim_start = std::time::Instant::now();
                println!("\n🔄 Processing flashblock {} for block {} in simulator thread", 
                    event.index, event.block_number);
                
                if use_revm_executor {
                    // Use revm-based executor
                    println!("   🔧 Using revm-based executor");
                    
                    // Re-initialize for new block if needed
                    if !revm_initialized || event.block_number != current_block {
                        if event.block_number != current_block {
                            println!("   🔄 New block detected: {} -> {}", current_block, event.block_number);
                            current_block = event.block_number;
                        }
                        
                        match revm_executor.initialize(blockchain_provider_for_task.clone(), BlockId::latest()).await {
                            Ok(_) => {
                                println!("   ✅ Revm executor initialized with node provider");
                                revm_initialized = true;
                            }
                            Err(e) => {
                                println!("   ❌ Failed to initialize revm executor: {:?}", e);
                                continue;
                            }
                        }
                    }
                    
                    // Execute with revm
                    match revm_executor.execute_flashblock(&event, event.index).await {
                        Ok(results) => {
                            let successful = results.iter().filter(|r| r.error.is_none()).count();
                            println!("   📊 Revm execution complete: {}/{} successful", successful, results.len());
                        }
                        Err(e) => {
                            println!("   ❌ Revm execution failed: {:?}", e);
                        }
                    }
                } else {
                    // Use RPC-based manager
                    let mut manager = flashblock_manager.lock().await;
                    match manager.process_flashblock(event.clone(), event.index).await {
                        Ok(_) => {
                            // Check if we can simulate MEV opportunities
                            if let Some(accumulator) = manager.get_accumulator(event.block_number) {
                                println!("   📊 Accumulated state for block {} - {}/{} flashblocks", 
                                    event.block_number, 
                                    accumulator.flashblocks_received(),
                                    11
                                );
                                
                                // Example: Simulate a new MEV transaction on top of current state
                                // This is where you would add your MEV logic
                            }
                        }
                        Err(e) => {
                            println!("   ❌ Failed to process flashblock: {:?}", e);
                        }
                    }
                    drop(manager);
                }
                
                println!("🏁 Flashblock {} processing completed in {:.2}ms total", 
                    event.index, 
                    sim_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            
            println!("⚠️  Flashblock simulator thread exiting");
        });
        
        // // Spawn a task to simulate calls every 2 seconds
        // tokio::spawn(async move {
        //     let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            
        //     loop {
        //         interval.tick().await;
                
        //         let from_addr = Address::from_str("0xd0ffEe48945a9518b0B543a2C59dFb102221fBb7").unwrap();
        //         let to_addr = Address::from_str("0x38cef6277942faf66b9cd9f1b5132d68ba175b32").unwrap();
                
        //         // Create the transaction request
        //         let tx_request = TransactionRequest {
        //             from: Some(from_addr),
        //             to: Some(TxKind::Call(to_addr)),
        //             value: Some(U256::from(0)),
        //             gas: Some(1_000_000_000), // 1 billion gas
        //             input: hex::decode("73eab4900000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000012c00000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000001dc8cff000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000753000000000000000000000000038cef6277942faf66b9cd9f1b5132d68ba175b3200000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000").unwrap().into(),
        //             ..Default::default()
        //         };
                
        //         // Call the simulation function
        //         if let Err(e) = simulation::simulate_transaction_batch(
        //             &eth_api,
        //             tx_request.into(),  // convert to the API's transaction type
        //             BlockId::latest(),  // target block
        //             512,                // batch size
        //             None,               // no base fee override
        //             None,               // no timestamp override
        //         ).await {
        //             println!("❌ Simulation batch failed: {:?}", e);
        //         }
        //     }
        // });

        handle.wait_for_node_exit().await
    })
}