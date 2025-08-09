use alloy_primitives::{Address, B256, U256};
use reth_db_api::{database::Database, transaction::DbTx};
use reth_primitives::{Account, Bytecode};
use revm::primitives::{AccountInfo, Bytes};
use std::sync::Arc;

/// Simple database adapter that reads directly from Reth database tables
/// This is a minimal implementation for transaction replay
pub struct SimpleStateDB<DB: Database> {
    tx: Arc<DB::TX>,
}

impl<DB: Database> SimpleStateDB<DB> {
    pub fn new(tx: DB::TX) -> Self {
        Self { tx: Arc::new(tx) }
    }
}

impl<DB: Database> revm::Database for SimpleStateDB<DB> {
    type Error = eyre::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        // Read account from PlainAccountState table
        use reth_db::tables::PlainAccountState;
        
        if let Some(account) = self.tx.get::<PlainAccountState>(address)? {
            Ok(Some(AccountInfo {
                balance: account.balance,
                nonce: account.nonce,
                code_hash: account.bytecode_hash.unwrap_or(B256::ZERO),
                code: None,
            }))
        } else {
            Ok(None)
        }
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytes, Self::Error> {
        // Read bytecode from Bytecodes table
        use reth_db::tables::Bytecodes;
        
        if let Some(bytecode) = self.tx.get::<Bytecodes>(code_hash)? {
            Ok(bytecode.bytes())
        } else {
            Ok(Bytes::default())
        }
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        // For a simple implementation, return zero storage
        // Full implementation would need to read from PlainStorageState
        // with proper key construction
        Ok(U256::ZERO)
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        // Read from CanonicalHeaders table
        use reth_db::tables::CanonicalHeaders;
        
        if let Some(hash) = self.tx.get::<CanonicalHeaders>(number)? {
            Ok(hash)
        } else {
            Ok(B256::ZERO)
        }
    }
}