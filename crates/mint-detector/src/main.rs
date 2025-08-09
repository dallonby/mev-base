use alloy_primitives::{Address, Bytes, U256, keccak256, TxKind};
use alloy_consensus::Header;
use eyre::Result;
use revm::{
    context::TxEnv,
    context_interface::result::{ExecutionResult, Output},
    state::AccountInfo,
};
use reth_revm::db::CacheDB;
use reth_optimism_chainspec::{BASE_MAINNET, OpChainSpec};
use reth_optimism_evm::OpEvmConfig;
use reth_optimism_primitives::OpPrimitives;
use reth_evm::{ConfigureEvm, Evm};
use reth_optimism_node::OpRethReceiptBuilder;

fn main() -> Result<()> {
    println!("Starting mint detector...");
    
    // Create a simple in-memory database using CacheDB
    let mut cache_db = CacheDB::new(revm::database::EmptyDB::new());
    
    // Setup a test account
    let test_address = Address::from([0x01; 20]);
    let account_info = AccountInfo {
        balance: U256::from(1_000_000_000_000_000_000u128), // 1 ETH
        nonce: 0,
        code_hash: keccak256(&[]),
        code: None,
    };
    cache_db.insert_account_info(test_address, account_info);
    
    // Create EVM with Base mainnet configuration
    let chain_spec = BASE_MAINNET.clone();
    println!("Using chain: Base");
    println!("Chain ID: {}", chain_spec.chain.id());
    
    // Create OpEvmConfig for Base mainnet with explicit type parameters
    let evm_config: OpEvmConfig<OpChainSpec, OpPrimitives> = OpEvmConfig::new(
        chain_spec.clone(),
        OpRethReceiptBuilder::default(),
    );
    
    // Setup EVM environment using the same approach as mev-base
    let current_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
        
    let evm_env = evm_config.evm_env(&Header {
        timestamp: current_timestamp,
        gas_limit: 30_000_000,
        base_fee_per_gas: Some(50_000_000), // 0.05 gwei typical for Base
        number: 1_000_000,
        ..Default::default()
    });
    
    // Create the EVM using OpEvmConfig
    let mut evm = evm_config.evm_with_env(&mut cache_db, evm_env);
    
    // Create a simple Optimism transaction with proper envelope
    let tx_env = TxEnv {
        caller: test_address,
        gas_limit: 1_000_000,
        gas_price: 1_000_000_000,
        kind: TxKind::Call(Address::from([0x02; 20])),
        data: Bytes::new(),
        value: U256::ZERO,
        ..Default::default()
    };
    
    // For testing, create a simple EIP-1559 transaction envelope
    use alloy_consensus::{TxEip1559, Signed, TxEnvelope};
    use alloy_eips::eip2718::Encodable2718;
    
    let tx_eip1559 = TxEip1559 {
        chain_id: 8453,
        nonce: 0,
        gas_limit: 1_000_000,
        max_fee_per_gas: 1_000_000_000,
        max_priority_fee_per_gas: 1_000_000,
        to: TxKind::Call(Address::from([0x02; 20])),
        value: U256::ZERO,
        access_list: Default::default(),
        input: Bytes::new(),
    };
    
    // Create a dummy signature for testing
    let signature = alloy_primitives::Signature::from_scalars_and_parity(
        alloy_primitives::B256::from([1u8; 32]),
        alloy_primitives::B256::from([2u8; 32]),
        false
    );
    
    let signed_tx = Signed::new_unchecked(tx_eip1559, signature, Default::default());
    let tx_envelope = TxEnvelope::Eip1559(signed_tx);
    let enveloped_bytes = tx_envelope.encoded_2718();
    
    let mut op_tx = op_revm::OpTransaction::new(tx_env);
    op_tx.enveloped_tx = Some(enveloped_bytes.into());
    
    // Execute transaction
    match evm.transact(op_tx) {
        Ok(result) => {
            match result.result {
                ExecutionResult::Success { output, gas_used, .. } => {
                    println!("Transaction successful!");
                    println!("Gas used: {}", gas_used);
                    match output {
                        Output::Call(value) => println!("Output: {:?}", value),
                        Output::Create(value, addr) => println!("Created contract at {:?}: {:?}", addr, value),
                    }
                }
                ExecutionResult::Revert { output, gas_used } => {
                    println!("Transaction reverted!");
                    println!("Gas used: {}", gas_used);
                    println!("Revert data: {:?}", output);
                }
                ExecutionResult::Halt { reason, gas_used } => {
                    println!("Transaction halted!");
                    println!("Gas used: {}", gas_used);
                    println!("Halt reason: {:?}", reason);
                }
            }
        }
        Err(e) => {
            println!("Transaction failed: {:?}", e);
        }
    }
    
    Ok(())
}