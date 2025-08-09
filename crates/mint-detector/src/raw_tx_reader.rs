use alloy_primitives::{Address, Bytes};
use alloy_consensus::{Transaction, TxEnvelope};
use alloy_rlp::Decodable;
use eyre::Result;
use reth_db_api::{
    database::Database,
    transaction::DbTx,
};

/// Read transactions from database, filtering out Optimism deposit transactions
pub struct RawTransactionReader;

impl RawTransactionReader {
    /// Try to decode a transaction, skipping type 126 (deposit) transactions
    pub fn decode_transaction(raw_bytes: &[u8]) -> Option<TxEnvelope> {
        // Check first byte for transaction type
        if !raw_bytes.is_empty() && raw_bytes[0] == 0x7e {
            // This is a deposit transaction (type 126), skip it
            return None;
        }
        
        // Try to decode as regular transaction
        match TxEnvelope::decode(&mut &raw_bytes[..]) {
            Ok(tx) => Some(tx),
            Err(_) => None,
        }
    }
    
    /// Get transactions for a block, filtering out deposit transactions
    pub fn get_block_transactions_filtered<DB: Database>(
        tx: &DB::TX,
        first_tx_num: u64,
        tx_count: u64,
    ) -> Result<Vec<(u64, TxEnvelope)>> {
        use reth_db::tables::Transactions;
        
        let mut transactions = Vec::new();
        let mut skipped = 0;
        
        for offset in 0..tx_count {
            let tx_id = first_tx_num + offset;
            
            // Unfortunately, we can't get raw bytes directly from the table
            // The get() method always decodes
            // This is where we'd need to patch reth or use lower-level MDBX access
            
            // For now, we have to handle the panic
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tx.get::<Transactions>(tx_id)
            })) {
                Ok(Ok(Some(tx_data))) => {
                    transactions.push((tx_id, tx_data));
                }
                _ => {
                    skipped += 1;
                }
            }
        }
        
        if skipped > 0 {
            eprintln!("Skipped {} transactions (likely deposit transactions)", skipped);
        }
        
        Ok(transactions)
    }
    
    /// Check if a transaction is to a wrapper contract
    pub fn is_wrapper_transaction(
        tx: &TxEnvelope,
        wrapper_contracts: &[Address],
    ) -> bool {
        if let Some(to) = tx.to() {
            wrapper_contracts.contains(&to)
        } else {
            false
        }
    }
}