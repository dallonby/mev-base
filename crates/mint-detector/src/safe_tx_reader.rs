use alloy_primitives::Address;
use alloy_consensus::{Transaction, TxEnvelope};
use eyre::Result;
use reth_db_api::{
    database::Database,
    transaction::DbTx,
    cursor::DbCursorRO,
};

/// Safely read transactions from the database, skipping Optimism deposit transactions
pub struct SafeTransactionReader<'a, TX: DbTx> {
    tx: &'a TX,
}

impl<'a, TX: DbTx> SafeTransactionReader<'a, TX> {
    pub fn new(tx: &'a TX) -> Self {
        Self { tx }
    }
    
    /// Try to get a transaction by ID, returning None for deposit transactions
    pub fn get_transaction(&self, tx_id: u64) -> Result<Option<TxEnvelope>> {
        use reth_db::tables::Transactions;
        
        // First, let's try to get it normally
        match self.tx.get::<Transactions>(tx_id) {
            Ok(Some(tx_data)) => Ok(Some(tx_data)),
            Ok(None) => Ok(None),
            Err(e) => {
                // Check if it's the deposit transaction error
                let error_msg = e.to_string();
                if error_msg.contains("Unsupported TxType") || error_msg.contains("126") {
                    // This is a deposit transaction, skip it
                    Ok(None)
                } else {
                    // Some other error
                    Err(e.into())
                }
            }
        }
    }
    
    /// Get transactions for a block, skipping deposit transactions
    pub fn get_block_transactions(
        &self,
        first_tx_num: u64,
        tx_count: u64,
    ) -> Result<Vec<(u64, TxEnvelope)>> {
        let mut transactions = Vec::new();
        
        for offset in 0..tx_count {
            let tx_id = first_tx_num + offset;
            if let Some(tx_data) = self.get_transaction(tx_id)? {
                transactions.push((tx_id, tx_data));
            }
        }
        
        Ok(transactions)
    }
    
    /// Check if a transaction is to a specific address
    pub fn is_transaction_to(
        &self,
        tx_id: u64,
        target: Address,
    ) -> Result<bool> {
        if let Some(tx_data) = self.get_transaction(tx_id)? {
            Ok(tx_data.to() == Some(target))
        } else {
            Ok(false)
        }
    }
}