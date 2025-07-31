use alloy_consensus::TxEnvelope;
use alloy_primitives::{Address, Bytes, U256};

/// A transaction that can be either signed or unsigned
#[derive(Debug, Clone)]
pub enum BundleTransaction {
    /// A signed transaction ready to be sent
    Signed(TxEnvelope),
    /// An unsigned transaction (for simulation only)
    Unsigned {
        from: Address,
        to: Option<Address>,
        value: U256,
        input: Bytes,
        gas_limit: u64,
        gas_price: U256,
        nonce: u64,
    },
}

impl BundleTransaction {
    /// Create an unsigned transaction
    #[allow(dead_code)]
    pub fn unsigned(
        from: Address,
        to: Option<Address>,
        value: U256,
        input: Bytes,
        gas_limit: u64,
        gas_price: U256,
        nonce: u64,
    ) -> Self {
        Self::Unsigned {
            from,
            to,
            value,
            input,
            gas_limit,
            gas_price,
            nonce,
        }
    }
    
    /// Create a simple call transaction
    #[allow(dead_code)]
    pub fn call(
        from: Address,
        to: Address,
        value: U256,
        input: Bytes,
        gas_limit: u64,
        gas_price: U256,
        nonce: u64,
    ) -> Self {
        Self::unsigned(from, Some(to), value, input, gas_limit, gas_price, nonce)
    }
    
    /// Get a hash for logging (returns zero hash for unsigned)
    #[allow(dead_code)]
    pub fn hash_for_logging(&self) -> alloy_primitives::B256 {
        match self {
            Self::Signed(tx) => *tx.tx_hash(),
            Self::Unsigned { .. } => alloy_primitives::B256::ZERO,
        }
    }
}

/// An MEV bundle containing multiple transactions
#[derive(Debug, Clone)]
pub struct MevBundle {
    pub transactions: Vec<BundleTransaction>,
    pub block_number: u64,
}

impl MevBundle {
    /// Create a new MEV bundle
    pub fn new(transactions: Vec<BundleTransaction>, block_number: u64) -> Self {
        Self {
            transactions,
            block_number,
        }
    }
    
    /// Add a transaction to the bundle
    pub fn add_transaction(&mut self, tx: BundleTransaction) {
        self.transactions.push(tx);
    }
}