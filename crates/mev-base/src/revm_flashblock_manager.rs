use std::collections::HashMap;
use std::sync::Arc;
use reth_provider::StateProviderFactory;
use reth_optimism_chainspec::OpChainSpec;
use alloy_rpc_types_eth::BlockId;
use crate::flashblocks::FlashblocksEvent;
use crate::revm_flashblock_executor::RevmFlashblockExecutor;

/// Manages flashblock processing using revm directly
pub struct RevmFlashblockManager<P> 
where 
    P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader + Clone,
    P::Header: alloy_consensus::BlockHeader,
{
    /// The provider for accessing blockchain state
    provider: P,
    /// Chain specification
    chain_spec: Arc<OpChainSpec>,
    /// Executor instances for each block
    executors: HashMap<u64, RevmFlashblockExecutor>,
    /// Maximum number of flashblocks per block
    max_flashblocks: usize,
    /// Number of blocks to keep in memory
    blocks_to_keep: usize,
}

impl<P> RevmFlashblockManager<P> 
where 
    P: StateProviderFactory + reth_provider::HeaderProvider + reth_provider::BlockReader + Clone,
    P::Header: alloy_consensus::BlockHeader,
{
    pub fn new(
        provider: P,
        chain_spec: Arc<OpChainSpec>,
        max_flashblocks: usize,
        blocks_to_keep: usize,
    ) -> Self {
        Self {
            provider,
            chain_spec,
            executors: HashMap::new(),
            max_flashblocks,
            blocks_to_keep,
        }
    }
    
    /// Process a flashblock event using revm
    pub async fn process_flashblock(
        &mut self,
        event: FlashblocksEvent,
        flashblock_index: u32,
    ) -> eyre::Result<()> {
        // Get or create executor for this block
        let executor = match self.executors.get_mut(&event.block_number) {
            Some(exec) => exec,
            None => {
                // Create new executor for this block
                let mut executor = RevmFlashblockExecutor::new(self.chain_spec.clone());
                
                // Initialize with the provider and block context
                // We simulate against the latest block (parent of the flashblock)
                executor.initialize(self.provider.clone(), BlockId::latest()).await?;
                
                self.executors.insert(event.block_number, executor);
                self.executors.get_mut(&event.block_number).unwrap()
            }
        };
        
        // Execute the flashblock
        let results = executor.execute_flashblock(&event, flashblock_index).await?;
        
        // Show summary
        let successful = results.iter().filter(|r| r.error.is_none()).count();
        let failed = results.len() - successful;
        println!("   📊 Flashblock {} results: {} successful, {} failed", 
            flashblock_index, successful, failed);
        
        // Clean up old executors
        self.cleanup_old_executors(event.block_number);
        
        Ok(())
    }
    
    /// Remove executors for blocks that are too old
    fn cleanup_old_executors(&mut self, current_block: u64) {
        if self.executors.len() > self.blocks_to_keep {
            let cutoff = current_block.saturating_sub(self.blocks_to_keep as u64);
            self.executors.retain(|&block_num, _| block_num >= cutoff);
        }
    }
    
    /// Get cache statistics for all executors
    pub fn get_stats(&self) -> String {
        format!(
            "RevmFlashblockManager: {} active executors, max {} flashblocks per block", 
            self.executors.len(),
            self.max_flashblocks
        )
    }
}