use alloy_primitives::{Address, U256, B256};
use std::collections::HashMap;
use revm::{state::AccountInfo, bytecode::Bytecode};
use alloy_consensus::TxEnvelope;

/// Snapshot of state changes from flashblocks
#[derive(Clone, Debug)]
pub struct FlashblockStateSnapshot {
    /// Block number this state is for
    pub block_number: u64,
    /// Flashblock index (0-10, total 11 flashblocks) 
    pub flashblock_index: u32,
    /// Account state changes accumulated so far
    pub account_changes: HashMap<Address, AccountInfo>,
    /// Storage changes per account (address -> (slot -> value))
    pub storage_changes: HashMap<Address, HashMap<U256, U256>>,
    /// Contract code changes (for CREATE operations)
    pub code_changes: HashMap<B256, Bytecode>,
    /// Base fee for the block
    pub base_fee: u128,
    /// Timestamp of when this snapshot was created
    pub snapshot_time: std::time::Instant,
    /// Original transactions from the flashblock (for calldata analysis)
    pub transactions: Vec<TxEnvelope>,
}

impl FlashblockStateSnapshot {
    /// Create a new state snapshot
    pub fn new(
        block_number: u64,
        flashblock_index: u32,
        base_fee: u128,
    ) -> Self {
        Self {
            block_number,
            flashblock_index,
            account_changes: HashMap::new(),
            storage_changes: HashMap::new(),
            code_changes: HashMap::new(),
            base_fee,
            snapshot_time: std::time::Instant::now(),
            transactions: Vec::new(),
        }
    }
    
    /// Add an account change
    pub fn add_account_change(&mut self, address: Address, info: AccountInfo) {
        self.account_changes.insert(address, info);
    }
    
    /// Add a storage change
    pub fn add_storage_change(&mut self, address: Address, slot: U256, value: U256) {
        self.storage_changes
            .entry(address)
            .or_insert_with(HashMap::new)
            .insert(slot, value);
    }
    
    /// Add contract code
    pub fn add_code_change(&mut self, code_hash: B256, bytecode: Bytecode) {
        self.code_changes.insert(code_hash, bytecode);
    }
    
    /// Get the age of this snapshot
    pub fn age_ms(&self) -> u64 {
        self.snapshot_time.elapsed().as_millis() as u64
    }
}