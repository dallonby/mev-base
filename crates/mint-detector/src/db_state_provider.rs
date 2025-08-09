use alloy_primitives::{Address, B256, U256};
use reth_db::{DatabaseEnv, tables};
use reth_db_api::{
    database::Database,
    transaction::DbTx,
};
use reth_primitives::{Account, Bytecode};
use revm::database::DatabaseRef;
use revm::primitives::{AccountInfo, KECCAK_EMPTY};
use reth_revm::db::CacheDB;
use std::sync::Arc;

/// A state provider that reads directly from the Reth database
pub struct DirectDbStateProvider {
    db: Arc<DatabaseEnv>,
    block_number: u64,
}

impl DirectDbStateProvider {
    pub fn new(db: Arc<DatabaseEnv>, block_number: u64) -> Self {
        Self { db, block_number }
    }
    
    /// Get account at a specific block
    fn get_account(&self, address: Address) -> Result<Option<Account>, reth_db_api::DatabaseError> {
        let tx = self.db.tx()?;
        
        // First, get the plain account state
        let account = tx.get::<tables::PlainAccountState>(address)?;
        
        Ok(account)
    }
    
    /// Get storage value at a specific block
    fn get_storage(&self, address: Address, index: U256) -> Result<U256, reth_db_api::DatabaseError> {
        let tx = self.db.tx()?;
        
        // For PlainStorageState, we need to get all storage for the address
        // and then look for the specific slot
        use reth_db_api::cursor::DbCursorRO;
        let mut cursor = tx.cursor_read::<tables::PlainStorageState>()?;
        
        // Seek to the address
        if let Ok(Some((key, entry))) = cursor.seek(address) {
            if key == address {
                // Check if this is the slot we're looking for
                let slot = B256::from(index);
                if entry.key == slot {
                    return Ok(entry.value);
                }
            }
        }
        
        Ok(U256::ZERO)
    }
    
    /// Get bytecode for an address
    fn get_bytecode(&self, address: Address) -> Result<Bytecode, reth_db_api::DatabaseError> {
        let tx = self.db.tx()?;
        
        // First get the account to find the code hash
        if let Some(account) = self.get_account(address)? {
            if account.bytecode_hash.is_some() && account.bytecode_hash != Some(KECCAK_EMPTY) {
                // Get bytecode from Bytecodes table using the hash
                if let Some(code_hash) = account.bytecode_hash {
                    if let Some(bytecode_entry) = tx.get::<tables::Bytecodes>(code_hash)? {
                        return Ok(bytecode_entry);
                    }
                }
            }
        }
        
        Ok(Bytecode::default())
    }
}

impl DatabaseRef for DirectDbStateProvider {
    type Error = reth_storage_errors::provider::ProviderError;
    
    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        if let Some(account) = self.get_account(address).map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))? {
            let code = if account.bytecode_hash.is_some() && account.bytecode_hash != Some(KECCAK_EMPTY) {
                Some(revm::primitives::Bytecode::new_raw(
                    self.get_bytecode(address)
                        .map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))?
                        .bytes()
                ))
            } else {
                None
            };
            
            Ok(Some(AccountInfo {
                balance: account.balance,
                nonce: account.nonce,
                code_hash: account.bytecode_hash.unwrap_or(KECCAK_EMPTY),
                code,
            }))
        } else {
            Ok(None)
        }
    }
    
    fn code_by_hash_ref(&self, code_hash: B256) -> Result<revm::primitives::Bytecode, Self::Error> {
        let tx = self.db.tx().map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))?;
        
        if let Some(bytecode_entry) = tx.get::<tables::Bytecodes>(code_hash)
            .map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))? {
            Ok(revm::primitives::Bytecode::new_raw(bytecode_entry.bytes()))
        } else {
            Ok(revm::primitives::Bytecode::default())
        }
    }
    
    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.get_storage(address, index)
            .map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))
    }
    
    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        let tx = self.db.tx().map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))?;
        
        // Get block hash from CanonicalHeaders table
        use reth_db::tables::CanonicalHeaders;
        if let Some(hash) = tx.get::<CanonicalHeaders>(number)
            .map_err(|e| reth_storage_errors::provider::ProviderError::Database(e))? {
            Ok(hash)
        } else {
            Ok(B256::ZERO)
        }
    }
}

/// A wrapper around CacheDB that uses our direct database state provider
pub fn create_cache_db_with_state(
    db: Arc<DatabaseEnv>,
    block_number: u64,
) -> CacheDB<DirectDbStateProvider> {
    let state_provider = DirectDbStateProvider::new(db, block_number);
    CacheDB::new(state_provider)
}