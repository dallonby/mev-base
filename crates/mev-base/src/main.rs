
use reth_optimism_node::{
    node::OpAddOns,
    OpNode,
};
use reth_optimism_cli::Cli;
use reth_provider::{ReceiptProvider, StateProviderFactory};
use reth_optimism_chainspec::BASE_MAINNET;
use alloy_rpc_types_eth::BlockId;

use futures::TryStreamExt;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;

use std::sync::Arc;
use tracing::{info, debug, error, warn};
use crate::transaction_service::{TransactionService, TransactionServiceConfig, WalletStrategy};
use crate::wallet_service::WalletService;
use crate::sequencer_service::SequencerService;

mod lifecycle_timing;
mod flashblocks;
mod flashblock_state;
mod mev_bundle_types;
mod mev_search_worker;
mod mev_simulation;
mod mev_task_worker;
mod revm_flashblock_executor;
mod gradient_descent;
mod gradient_descent_parallel;
mod gradient_descent_fast;
pub mod backrun_analyzer;
mod logging;
mod transaction_service;
mod wallet_service;
mod sequencer_service;
mod metrics;

/// Block subscriber ExEx that echoes block numbers
async fn block_subscriber_exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
) -> eyre::Result<()> 
{
    info!("Block subscriber ExEx started!");
    
    // Access the provider for RPC-like operations
    let provider = ctx.provider().clone();
    
    // Subscribe to chain state notifications
    while let Some(notification) = ctx.notifications.try_next().await? {
        match &notification {
            ExExNotification::ChainCommitted { new } => {
                // New blocks committed to the canonical chain
                let tip = new.tip();
                let range = new.range();
                info!(
                    start_block = %range.start(),
                    end_block = %range.end(),
                    tip_hash = %tip.hash(),
                    block_count = range.clone().count(),
                    "New blocks committed to chain"
                );
                
                // Example: Access additional block data via provider
                if let Ok(receipts) = provider.receipts_by_block(tip.hash().into()) {
                    if let Some(receipts) = receipts {
                        debug!(tx_count = receipts.len(), "Transactions in last block");
                    }
                }
            }
            ExExNotification::ChainReorged { old, new } => {
                warn!(old_range = ?old.range(), new_range = ?new.range(), "Chain reorg detected");
            }
            ExExNotification::ChainReverted { old } => {
                warn!(range = ?old.range(), "Chain reverted");
            }
        }

        // Signal that we've processed up to this height for pruning
        // This allows Reth to prune the WAL and old blocks
        if let Some(committed_chain) = notification.committed_chain() {
            // log a message here
            let num_hash = committed_chain.tip().num_hash();
            debug!(height = num_hash.number, hash = %num_hash.hash, "Processed up to height");
            ctx.events.send(ExExEvent::FinishedHeight(num_hash))?;
        }
    }

    Ok(())
}

fn main() -> eyre::Result<()> {
    // Initialize logging before anything else
    logging::init_logging();
    
    Cli::parse_args().run(|builder, rollup_args| async move {
        let node = OpNode::new(rollup_args.clone());
        let handle = builder
            .with_types::<OpNode>()
            .with_components(node.components())
            .with_add_ons(OpAddOns::default())
            .on_node_started(|_full_node| {
                info!("Node started successfully!");
                Ok(())
            })
            .on_rpc_started(|_ctx, _handles| {
                info!("RPC server started!");
                Ok(())
            })
            // ExEx disabled - not currently used
            // .install_exex("block-echo", move |ctx| {
            //     async move { Ok(block_subscriber_exex(ctx)) }
            // })
            .launch()
            .await?;

        
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
        
        info!("Flashblocks client connected to wss://mainnet.flashblocks.base.org/ws");
        
        
        // Create a channel for flashblock processing queue
        let (flashblock_tx, mut flashblock_rx) = tokio::sync::mpsc::channel(100);
        
        // Spawn task to receive flashblocks and queue them
        tokio::spawn(async move {
            while let Ok(event) = flashblocks_receiver.recv().await {
                debug!(
                    block = event.block_number,
                    flashblock = event.index,
                    tx_count = event.transactions.len(),
                    state_root = %event.state_root,
                    receipts_root = %event.receipts_root,
                    "Flashblocks event received"
                );
                
                // Increment metrics
                crate::metrics::MEV_METRICS.flashblocks_received_total.increment(1);
                
                // Queue the event for processing
                if let Err(e) = flashblock_tx.send(event).await {
                    error!(error = %e, "Failed to queue flashblock");
                }
            }
        });
        
        // Clone provider for the spawned task
        let blockchain_provider_for_task = blockchain_provider.clone();
        
        // Create channel for MEV results
        let (mev_result_tx, mut mev_result_rx) = tokio::sync::mpsc::channel::<mev_search_worker::MevOpportunity>(1000);
        
        // Create timing tracker
        let timing_tracker = lifecycle_timing::create_timing_tracker();
        
        // Initialize transaction services
        let wallet_service = match WalletService::from_env() {
            Ok(service) => {
                info!("Wallet service initialized with {} wallets", service.wallet_count());
                Arc::new(service)
            }
            Err(e) => {
                warn!("Failed to initialize wallet service: {}. Transaction submission disabled.", e);
                // Create empty wallet service
                Arc::new(WalletService::new(vec![]).unwrap())
            }
        };
        
        let sequencer_service = match SequencerService::from_env() {
            Ok(service) => {
                info!("Sequencer service initialized");
                Arc::new(service)
            }
            Err(e) => {
                error!("Failed to initialize sequencer service: {}", e);
                return Err(e.into());
            }
        };
        
        // Load transaction service config from env
        let tx_config = TransactionServiceConfig {
            enabled: std::env::var("BLOCK_TX_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse::<bool>()
                .unwrap_or(true),
            dry_run: std::env::var("BLOCK_TX_DRY_RUN")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .unwrap_or(false),
            chain_id: 8453, // Base mainnet
            default_gas_limit: std::env::var("BLOCK_TX_DEFAULT_GAS_LIMIT")
                .ok()
                .and_then(|s| s.parse::<u64>().ok()),
            gas_multiplier: std::env::var("BLOCK_TX_GAS_MULTIPLIER")
                .unwrap_or_else(|_| "1.2".to_string())
                .parse::<f64>()
                .unwrap_or(1.2),
            wallet_strategy: match std::env::var("BLOCK_TX_WALLET_STRATEGY")
                .unwrap_or_else(|_| "default".to_string())
                .to_lowercase()
                .as_str() {
                "random" => WalletStrategy::Random,
                "round-robin" => WalletStrategy::RoundRobin,
                _ => WalletStrategy::Default,
            },
        };
        
        let transaction_service = Arc::new(TransactionService::new(
            tx_config.clone(),
            wallet_service.clone(),
            sequencer_service.clone(),
        ));
        
        info!(
            enabled = tx_config.enabled,
            dry_run = tx_config.dry_run,
            wallet_strategy = ?tx_config.wallet_strategy,
            "Transaction service initialized"
        );
        
        // Define minimum profit threshold from env or default (before spawning tasks)
        let min_profit_threshold = std::env::var("MEV_MIN_PROFIT_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(alloy_primitives::U256::from)
            .unwrap_or_else(|| alloy_primitives::U256::from(10_000_000_000_000u64)); // Default: 0.00001 ETH (10 microether)
        
        info!(
            threshold_wei = %min_profit_threshold,
            threshold_eth = format!("{:.6}", min_profit_threshold.as_limbs()[0] as f64 / 1e18),
            "MEV profit threshold configured"
        );
        
        // Clone for the MEV handler task
        let threshold_for_handler = min_profit_threshold;
        
        // Clone provider for MEV handler
        let mev_provider = blockchain_provider.clone();
        
        // Spawn MEV opportunity handler with JSON logging
        tokio::spawn(async move {
            while let Some(opportunity) = mev_result_rx.recv().await {
                info!(
                    strategy = %opportunity.strategy,
                    block = opportunity.block_number,
                    flashblock = opportunity.flashblock_index,
                    profit_wei = %opportunity.expected_profit,
                    bundle_size = opportunity.bundle.transactions.len(),
                    "MEV opportunity found"
                );
                
                // Record opportunity metrics
                crate::metrics::MEV_METRICS.opportunities_found_total.increment(1);
                
                // Log to JSON if profit exceeds threshold
                if opportunity.expected_profit > threshold_for_handler {
                    crate::metrics::MEV_METRICS.opportunities_profitable_total.increment(1);
                    if let Err(e) = log_mev_opportunity_to_json(&opportunity) {
                        error!(error = ?e, "Failed to log MEV opportunity to JSON");
                    }
                    
                    // Process the opportunity (build, sign, and submit transaction)
                    let process_start = std::time::Instant::now();
                    
                    match transaction_service.process_opportunity(&opportunity, &mev_provider).await {
                        Ok(()) => {
                            let elapsed = process_start.elapsed();
                            info!(
                                strategy = %opportunity.strategy,
                                block = opportunity.block_number,
                                elapsed_ms = elapsed.as_millis(),
                                "Successfully processed MEV opportunity"
                            );
                        }
                        Err(e) => {
                            error!(
                                strategy = %opportunity.strategy,
                                block = opportunity.block_number,
                                error = ?e,
                                "Failed to process MEV opportunity"
                            );
                        }
                    }
                } else {
                    debug!(
                        strategy = %opportunity.strategy,
                        profit_wei = %opportunity.expected_profit,
                        threshold_wei = %threshold_for_handler,
                        "MEV opportunity below profit threshold, skipping"
                    );
                }
            }
        });
        
        // Spawn dedicated synchronous flashblock simulator thread
        tokio::spawn(async move {
            info!("Starting dedicated flashblock simulator thread");
            
            // Create revm executor with the node's provider
            let chain_spec = BASE_MAINNET.clone();
            let mut revm_executor = revm_flashblock_executor::RevmFlashblockExecutor::new(chain_spec.clone());
            let mut revm_initialized = false;
            let mut current_block = 0u64;
            
            while let Some(event) = flashblock_rx.recv().await {
                let sim_start = std::time::Instant::now();
                
                // Create lifecycle timing for this flashblock
                let mut timing = lifecycle_timing::LifecycleTiming::new(
                    event.received_at,
                    event.block_number,
                    event.index,
                );
                timing.processing_started = Some(sim_start);
                
                // Record queue latency metric
                let queue_latency = sim_start.duration_since(event.received_at).as_secs_f64();
                crate::metrics::MEV_METRICS.flashblock_queue_latency_seconds.record(queue_latency);
                
                // Clone for workers
                let timing_for_workers = Arc::new(tokio::sync::Mutex::new(Some(timing.clone())));
                *timing_tracker.lock().await = Some(timing.clone());
                
                debug!(
                    block = event.block_number,
                    flashblock = event.index,
                    "Processing flashblock in simulator thread"
                );
                
                // Use revm-based executor
                debug!("Using revm-based executor");
                
                // Re-initialize for new block if needed
                if !revm_initialized || event.block_number != current_block {
                    if event.block_number != current_block {
                        debug!(old_block = current_block, new_block = event.block_number, "New block detected");
                        current_block = event.block_number;
                    }
                    
                    match revm_executor.initialize(blockchain_provider_for_task.clone(), BlockId::latest()).await {
                        Ok(_) => {
                            debug!("Revm executor initialized with node provider");
                            revm_initialized = true;
                        }
                        Err(e) => {
                            error!(error = ?e, "Failed to initialize revm executor");
                            continue;
                        }
                    }
                }
                
                // Execute with revm
                match revm_executor.execute_flashblock(&event, event.index).await {
                    Ok(results) => {
                        let successful = results.iter().filter(|r| r.error.is_none()).count();
                        debug!(
                            successful = successful,
                            total = results.len(),
                            "Revm execution complete"
                        );
                        
                        // Update timing and record metric
                        timing.execution_completed = Some(std::time::Instant::now());
                        let exec_duration = timing.execution_completed.unwrap().duration_since(timing.processing_started.unwrap()).as_secs_f64();
                        crate::metrics::MEV_METRICS.flashblock_execution_duration_seconds.record(exec_duration);
                        
                        // Export state snapshot and trigger MEV search
                        let export_start = std::time::Instant::now();
                        match revm_executor.export_state_snapshot(event.index, event.transactions.clone()) {
                            Ok(state_snapshot) => {
                                let export_time = export_start.elapsed().as_secs_f64() * 1000.0;
                                debug!(
                                    accounts = state_snapshot.account_changes.len(),
                                    time_ms = export_time,
                                    "State snapshot exported"
                                );
                                
                                // Update timing and record metric
                                timing.state_export_completed = Some(std::time::Instant::now());
                                let export_duration = export_start.elapsed().as_secs_f64();
                                crate::metrics::MEV_METRICS.state_export_duration_seconds.record(export_duration);
                                
                                // Analyze state to determine which strategies to trigger
                                let strategies = mev_search_worker::analyze_state_for_strategies(&state_snapshot);
                                timing.strategy_analysis_completed = Some(std::time::Instant::now());
                                
                                if !strategies.is_empty() {
                                    debug!(
                                        count = strategies.len(),
                                        strategies = ?strategies,
                                        "Triggering MEV strategies"
                                    );
                                    
                                    // Spawn all MEV tasks in batch for reduced overhead
                                    mev_task_worker::spawn_mev_tasks_batch(
                                        chain_spec.clone(),
                                        blockchain_provider_for_task.clone(),
                                        strategies,
                                        state_snapshot.clone(),
                                        event.received_at,
                                        mev_result_tx.clone(),
                                        Some(timing_for_workers.clone()),
                                        min_profit_threshold,
                                    );
                                    timing.workers_spawned = Some(std::time::Instant::now());
                                } else {
                                    debug!("No MEV strategies triggered for this flashblock");
                                }
                                
                                // Benchmarking removed - no longer needed
                            }
                            Err(e) => {
                                error!(error = ?e, "Failed to export state snapshot");
                            }
                        }
                        
                        // Example: Simulate an MEV bundle on top of the current flashblock state
                        // This demonstrates how to test MEV opportunities after flashblock execution
                        if event.index == 10 && !event.transactions.is_empty() {
                            debug!("Testing MEV bundle simulation on final flashblock");
                            
                            // Create a test bundle with one of the existing transactions
                            // In real usage, this would be your MEV transactions
                            let test_bundle = vec![event.transactions[0].clone()];
                            
                            match revm_executor.simulate_bundle(test_bundle, event.block_number).await {
                                Ok(_bundle_results) => {
                                    debug!("MEV bundle simulation completed");
                                }
                                Err(e) => {
                                    error!(error = ?e, "MEV bundle simulation failed");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = ?e, "Revm execution failed");
                    }
                }
                
                let total_time = sim_start.elapsed().as_secs_f64() * 1000.0;
                debug!(
                    block = event.block_number,
                    flashblock = event.index,
                    time_ms = total_time,
                    "Flashblock processing completed"
                );
                
                // Update timing tracker with final timing
                *timing_tracker.lock().await = Some(timing);
            }
            
            warn!("Flashblock simulator thread exiting");
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
        //             error!(error = ?e, "Simulation batch failed");
        //         }
        //     }
        // });

        handle.wait_for_node_exit().await
    })
}

/// Log MEV opportunity to JSON file
fn log_mev_opportunity_to_json(opportunity: &mev_search_worker::MevOpportunity) -> eyre::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use serde::Serialize;
    
    #[derive(Serialize)]
    struct MevResultLog {
        timestamp: u64,
        block_number: u64,
        flashblock_index: u32,
        strategy: String,
        expected_profit_wei: String,
        expected_profit_eth: f64,
        bundle_size: usize,
        // Add first transaction details if available
        first_tx_to: Option<String>,
        first_tx_calldata: Option<String>,
    }
    
    let first_tx = opportunity.bundle.transactions.first();
    
    let (first_tx_to, first_tx_calldata) = match first_tx {
        Some(mev_bundle_types::BundleTransaction::Unsigned { to, input, .. }) => {
            (to.map(|addr| format!("{:?}", addr)), Some(format!("0x{}", hex::encode(input))))
        }
        Some(mev_bundle_types::BundleTransaction::Signed(_)) => {
            // For signed transactions, we'd need to decode the envelope
            (None, None)
        }
        None => (None, None),
    };
    
    let result = MevResultLog {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        block_number: opportunity.block_number,
        flashblock_index: opportunity.flashblock_index,
        strategy: opportunity.strategy.clone(),
        expected_profit_wei: opportunity.expected_profit.to_string(),
        expected_profit_eth: opportunity.expected_profit.as_limbs()[0] as f64 / 1e18,
        bundle_size: opportunity.bundle.transactions.len(),
        first_tx_to,
        first_tx_calldata,
    };
    
    // Append to JSONL file (JSON Lines format)
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("mev_results.jsonl")?;
    
    let json = serde_json::to_string(&result)?;
    writeln!(file, "{}", json)?;
    
    Ok(())
}