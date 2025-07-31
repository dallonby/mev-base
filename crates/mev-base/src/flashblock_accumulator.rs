use std::collections::HashMap;
use std::time::Instant;
use alloy_primitives::{B256, Address, U256, TxKind};
use alloy_consensus::TxEnvelope;
use alloy_rpc_types_eth::{BlockId, state::StateOverride, EthCallResponse, TransactionRequest};
use reth_rpc_eth_api::{helpers::EthCall, EthApiTypes, RpcTypes};
use crate::flashblocks::{FlashblocksEvent, Metadata};
use crate::simulation::{simulate_bundle, simulate_bundle_with_hashes, BundleSimulationRequest};

/// Represents a single flashblock with its transactions and metadata
#[derive(Debug, Clone)]
pub struct FlashblockData {
    pub index: u32,
    pub block_number: u64,
    pub transactions: Vec<TxEnvelope>,
    pub state_root: B256,
    pub receipts_root: B256,
    pub metadata: Metadata,
    /// Simulation results for transactions in this flashblock
    pub simulation_results: Option<Vec<EthCallResponse>>,
}

/// Accumulates flashblocks for a specific block number and manages incremental simulation
pub struct FlashblockAccumulator<EthApi> {
    /// Current block number being accumulated
    block_number: u64,
    /// Ordered flashblocks by index
    flashblocks: Vec<Option<FlashblockData>>,
    /// Cumulative state overrides from all simulated flashblocks
    cumulative_state: StateOverride,
    /// Reference to the Ethereum API for simulations
    eth_api: EthApi,
    /// Maximum number of flashblocks per block (typically 10)
    max_flashblocks: usize,
}

impl<EthApi> FlashblockAccumulator<EthApi>
where
    EthApi: EthCall + Clone + Send + Sync + 'static,
    <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest: Clone + Send + Sync,
{
    /// Creates a new accumulator for a specific block
    pub fn new(block_number: u64, eth_api: EthApi, max_flashblocks: usize) -> Self {
        Self {
            block_number,
            flashblocks: vec![None; max_flashblocks],
            cumulative_state: StateOverride::default(),
            eth_api,
            max_flashblocks,
        }
    }

    /// Adds a new flashblock and simulates it on top of previous state
    pub async fn add_flashblock(&mut self, event: FlashblocksEvent, index: u32) -> eyre::Result<()> {
        // Check if this is for the correct block
        if event.block_number != self.block_number {
            return Err(eyre::eyre!(
                "Flashblock for wrong block: expected {}, got {}",
                self.block_number,
                event.block_number
            ));
        }

        // Check index bounds
        if index as usize >= self.max_flashblocks {
            return Err(eyre::eyre!(
                "Flashblock index {} exceeds maximum {} (valid indices: 0-{})",
                index,
                self.max_flashblocks - 1,
                self.max_flashblocks - 1
            ));
        }

        // Create flashblock data
        let mut flashblock = FlashblockData {
            index,
            block_number: event.block_number,
            transactions: event.transactions.clone(),
            state_root: event.state_root,
            receipts_root: event.receipts_root,
            metadata: event.metadata,
            simulation_results: None,
        };

        // If there are transactions, simulate them on top of cumulative state
        if !flashblock.transactions.is_empty() {
            println!("\n📊 Simulating flashblock {} with {} transactions", index, flashblock.transactions.len());
            let simulation_start = Instant::now();
            
            // Convert transactions to TransactionRequest
            // For now, we'll create minimal transaction requests from the envelopes
            let mut tx_requests = Vec::new();
            let mut tx_hashes = Vec::new();
            for tx in &flashblock.transactions {
                use alloy_consensus::Transaction as _;
                
                // Calculate transaction hash for identification
                let tx_hash = tx.tx_hash();
                tx_hashes.push(tx_hash.clone());
                
                let tx_req = match tx {
                    TxEnvelope::Legacy(tx) => {
                        TransactionRequest {
                            from: tx.recover_signer().ok(),
                            to: tx.to().map(TxKind::Call),
                            gas: Some(tx.gas_limit()),
                            gas_price: tx.gas_price(),
                            value: Some(tx.value()),
                            input: tx.input().clone().into(),
                            nonce: Some(tx.nonce()),
                            ..Default::default()
                        }
                    }
                    TxEnvelope::Eip2930(tx) => {
                        TransactionRequest {
                            from: tx.recover_signer().ok(),
                            to: tx.to().map(TxKind::Call),
                            gas: Some(tx.gas_limit()),
                            gas_price: tx.gas_price(),
                            value: Some(tx.value()),
                            input: tx.input().clone().into(),
                            nonce: Some(tx.nonce()),
                            access_list: tx.access_list().cloned(),
                            ..Default::default()
                        }
                    }
                    TxEnvelope::Eip1559(tx) => {
                        TransactionRequest {
                            from: tx.recover_signer().ok(),
                            to: tx.to().map(TxKind::Call),
                            gas: Some(tx.gas_limit()),
                            max_fee_per_gas: Some(tx.max_fee_per_gas()),
                            max_priority_fee_per_gas: tx.max_priority_fee_per_gas(),
                            value: Some(tx.value()),
                            input: tx.input().clone().into(),
                            nonce: Some(tx.nonce()),
                            access_list: tx.access_list().cloned(),
                            ..Default::default()
                        }
                    }
                    _ => continue, // Skip other transaction types for now
                };
                
                // For now, we'll collect the requests in a simpler way
                tx_requests.push(tx_req);
            }

            // Convert to the API's transaction request type with hashes
            let api_requests: Vec<_> = tx_requests.into_iter()
                .zip(tx_hashes.clone().into_iter())
                .map(|(req, hash)| {
                    // This is a workaround - in a real implementation you'd want proper conversion
                    let json = serde_json::to_value(req).unwrap();
                    let api_tx = serde_json::from_value(json).unwrap();
                    BundleSimulationRequest {
                        transaction: api_tx,
                        tx_hash: Some(hash),
                    }
                })
                .collect();
            
            // Simulate the bundle with cumulative state overrides
            // We simulate against 'latest' which is the last finalized block
            let simulation_block = BlockId::latest();
            println!("   🎯 Simulating against latest block");
            
            let results = simulate_bundle_with_hashes(
                &self.eth_api,
                api_requests,
                simulation_block,
                None, // base fee override
                None, // timestamp override
                Some(self.cumulative_state.clone()),
            ).await?;

            // Store simulation results
            if let Some(bundle_results) = results.first() {
                flashblock.simulation_results = Some(bundle_results.clone());
                
                // Process results and show timing
                let simulation_duration = simulation_start.elapsed();
                println!("⏱️  Simulation completed in {:.2}ms", simulation_duration.as_secs_f64() * 1000.0);
                
                // Analyze results for reverts
                let mut reverted_count = 0;
                let mut successful_count = 0;
                
                for (i, (result, tx_hash)) in bundle_results.iter().zip(tx_hashes.iter()).enumerate() {
                    if let Some(error) = &result.error {
                        reverted_count += 1;
                        println!("   ❌ Tx {}: {} REVERTED - {}", i, tx_hash, error);
                    } else {
                        successful_count += 1;
                        if let Some(gas_used) = result.gas_used {
                            println!("   ✅ Tx {}: {} - Gas: {}", i, tx_hash, gas_used);
                        } else {
                            println!("   ✅ Tx {}: {} - Success", i, tx_hash);
                        }
                    }
                }
                
                println!("   📈 Summary: {} successful, {} reverted", successful_count, reverted_count);
                if bundle_results.len() > 0 {
                    let avg_time = (simulation_duration.as_secs_f64() * 1000.0) / bundle_results.len() as f64;
                    println!("   ⚡ Average time per tx: {:.2}ms", avg_time);
                }
                
                // Update cumulative state based on simulation results
                self.update_cumulative_state(&flashblock, bundle_results)?;
            }
        }

        // Store the flashblock
        self.flashblocks[index as usize] = Some(flashblock);
        
        Ok(())
    }

    /// Updates cumulative state overrides based on flashblock execution
    fn update_cumulative_state(
        &mut self,
        flashblock: &FlashblockData,
        _results: &[EthCallResponse],
    ) -> eyre::Result<()> {
        // Parse new account balances from metadata
        for (address_str, balance_str) in &flashblock.metadata.new_account_balances {
            if let Ok(address) = address_str.parse::<Address>() {
                if let Ok(balance) = U256::from_str_radix(balance_str.trim_start_matches("0x"), 16) {
                    // Update the balance in cumulative state
                    let account_override = self.cumulative_state.entry(address).or_default();
                    account_override.balance = Some(balance);
                }
            }
        }

        // You can extend this to update other state changes like:
        // - Storage slots that were modified
        // - Code deployments
        // - Nonce updates
        
        Ok(())
    }

    /// Gets all transactions up to and including the specified flashblock index
    pub fn get_cumulative_transactions(&self, up_to_index: u32) -> Vec<TxEnvelope> {
        let mut transactions = Vec::new();
        
        for i in 0..=up_to_index.min(self.max_flashblocks as u32 - 1) {
            if let Some(flashblock) = &self.flashblocks[i as usize] {
                transactions.extend(flashblock.transactions.clone());
            }
        }
        
        transactions
    }

    /// Gets the cumulative state override up to the specified flashblock
    pub fn get_cumulative_state(&self) -> &StateOverride {
        &self.cumulative_state
    }

    /// Checks if all flashblocks have been received
    pub fn is_complete(&self) -> bool {
        self.flashblocks.iter().all(|fb| fb.is_some())
    }

    /// Gets the number of flashblocks received
    pub fn flashblocks_received(&self) -> usize {
        self.flashblocks.iter().filter(|fb| fb.is_some()).count()
    }

    /// Gets a specific flashblock by index
    pub fn get_flashblock(&self, index: u32) -> Option<&FlashblockData> {
        self.flashblocks.get(index as usize)?.as_ref()
    }

    /// Gets all received flashblocks in order
    pub fn get_all_flashblocks(&self) -> Vec<&FlashblockData> {
        self.flashblocks.iter()
            .filter_map(|fb| fb.as_ref())
            .collect()
    }

    /// Simulates a new transaction on top of the current accumulated state
    pub async fn simulate_on_top(
        &self,
        transaction: <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest,
    ) -> eyre::Result<Vec<Vec<EthCallResponse>>> {
        // Convert transaction to API type
        let json = serde_json::to_value(transaction).unwrap();
        let api_tx: <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest = 
            serde_json::from_value(json).unwrap();
            
        simulate_bundle(
            &self.eth_api,
            vec![api_tx],
            BlockId::latest(),
            None,
            None,
            Some(self.cumulative_state.clone()),
        ).await
    }
}

/// Manages multiple block accumulators
pub struct FlashblockManager<EthApi> {
    /// Active accumulators by block number
    accumulators: HashMap<u64, FlashblockAccumulator<EthApi>>,
    /// Ethereum API reference
    eth_api: EthApi,
    /// Maximum flashblocks per block
    max_flashblocks: usize,
    /// Maximum number of blocks to keep in memory
    max_blocks: usize,
}

impl<EthApi> FlashblockManager<EthApi>
where
    EthApi: EthCall + Clone + Send + Sync + 'static,
    <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest: Clone + Send + Sync,
{
    pub fn new(eth_api: EthApi, max_flashblocks: usize, max_blocks: usize) -> Self {
        Self {
            accumulators: HashMap::new(),
            eth_api,
            max_flashblocks,
            max_blocks,
        }
    }

    /// Processes a new flashblock event
    pub async fn process_flashblock(&mut self, event: FlashblocksEvent, index: u32) -> eyre::Result<()> {
        let block_number = event.block_number;
        
        // Get or create accumulator for this block
        if !self.accumulators.contains_key(&block_number) {
            // Clean up old blocks if we're at capacity
            if self.accumulators.len() >= self.max_blocks {
                self.cleanup_old_blocks(block_number);
            }
            
            let accumulator = FlashblockAccumulator::new(
                block_number,
                self.eth_api.clone(),
                self.max_flashblocks,
            );
            self.accumulators.insert(block_number, accumulator);
        }
        
        // Add flashblock to accumulator
        if let Some(accumulator) = self.accumulators.get_mut(&block_number) {
            accumulator.add_flashblock(event, index).await?;
            
            println!(
                "📦 Block {}: {}/{} flashblocks received", 
                block_number,
                accumulator.flashblocks_received(),
                self.max_flashblocks
            );
        }
        
        Ok(())
    }

    /// Gets an accumulator for a specific block
    pub fn get_accumulator(&self, block_number: u64) -> Option<&FlashblockAccumulator<EthApi>> {
        self.accumulators.get(&block_number)
    }

    /// Cleans up old blocks to maintain memory limits
    fn cleanup_old_blocks(&mut self, current_block: u64) {
        // Remove blocks that are more than max_blocks behind current
        let cutoff = current_block.saturating_sub(self.max_blocks as u64);
        self.accumulators.retain(|&block_num, _| block_num > cutoff);
    }
}