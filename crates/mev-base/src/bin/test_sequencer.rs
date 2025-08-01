use alloy_consensus::{TxEip1559, TxEnvelope, Signed, SignableTransaction};
use alloy_primitives::{Address, U256, Bytes};
use alloy_signer_local::PrivateKeySigner;
use alloy_network::TxSigner;
use alloy_eips::eip2718::Encodable2718;
use eyre::Result;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap())
        )
        .init();

    // Create a test wallet (this is a well-known test private key)
    let private_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    let wallet = PrivateKeySigner::from_str(private_key)?;
    let from_address = wallet.address();
    
    println!("Test wallet address: {}", from_address);
    
    // Create a simple transfer transaction
    let to_address = Address::from_str("0x0000000000000000000000000000000000000001")?;
    let value = U256::from(0); // 0 ETH transfer
    let nonce = 0u64; // First transaction
    let gas_limit = 21000u64; // Standard transfer
    let max_fee_per_gas = 10_000_000u128; // 0.01 gwei (Base has very low fees)
    let max_priority_fee_per_gas = 1_000_000u128; // 0.001 gwei
    
    // Build EIP-1559 transaction
    let mut tx = TxEip1559 {
        chain_id: 8453, // Base mainnet
        nonce,
        gas_limit,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        to: alloy_primitives::TxKind::Call(to_address),
        value,
        access_list: Default::default(),
        input: Bytes::new(), // Empty calldata
    };
    
    println!("Transaction details:");
    println!("  From: {}", from_address);
    println!("  To: {}", to_address);
    println!("  Value: {} ETH", value);
    println!("  Gas limit: {}", gas_limit);
    println!("  Max fee per gas: {} gwei", max_fee_per_gas as f64 / 1e9);
    println!("  Max priority fee: {} gwei", max_priority_fee_per_gas as f64 / 1e9);
    println!("  Nonce: {}", nonce);
    
    // Sign the transaction
    let signature = wallet.sign_transaction(&mut tx).await?;
    let tx_hash = tx.signature_hash();
    println!("Transaction hash: {}", tx_hash);
    
    // Create signed transaction envelope
    let signed_tx = TxEnvelope::Eip1559(Signed::new_unchecked(tx, signature, tx_hash));
    
    // Encode using EIP-2718
    let encoded_bytes = signed_tx.encoded_2718();
    let encoded_hex = format!("0x{}", hex::encode(&encoded_bytes));
    
    println!("Encoded transaction ({} bytes): {}", encoded_bytes.len(), encoded_hex);
    
    // Try different encoding methods to debug
    println!("\nDifferent encoding methods:");
    
    // Method 1: Direct RLP encoding (wrong for typed transactions)
    let rlp_bytes = alloy_rlp::encode(&signed_tx);
    let rlp_hex = format!("0x{}", hex::encode(&rlp_bytes));
    println!("RLP encoding ({} bytes): {}...", rlp_bytes.len(), &rlp_hex[..50.min(rlp_hex.len())]);
    
    // Method 2: EIP-2718 encoding (correct for typed transactions)
    println!("EIP-2718 encoding ({} bytes): {}...", encoded_bytes.len(), &encoded_hex[..50.min(encoded_hex.len())]);
    
    // Check the transaction type byte
    if !encoded_bytes.is_empty() {
        println!("\nTransaction type byte: 0x{:02x} (should be 0x02 for EIP-1559)", encoded_bytes[0]);
    }
    
    // Create sequencer client
    let client = reqwest::Client::new();
    let sequencer_url = std::env::var("SEQUENCER_URL")
        .unwrap_or_else(|_| "https://mainnet-sequencer.base.org/".to_string());
    
    println!("\nSubmitting to sequencer: {}", sequencer_url);
    
    // Create JSON-RPC request
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_sendRawTransaction",
        "params": [encoded_hex],
        "id": 1
    });
    
    println!("Request body: {}", serde_json::to_string_pretty(&request_body)?);
    
    // Send request
    let response = client
        .post(&sequencer_url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;
    
    let status = response.status();
    let response_text = response.text().await?;
    
    println!("\nResponse status: {}", status);
    println!("Response body: {}", response_text);
    
    // Parse response
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response_text) {
        if let Some(error) = json.get("error") {
            println!("\nError details:");
            println!("{}", serde_json::to_string_pretty(error)?);
        }
    }
    
    Ok(())
}