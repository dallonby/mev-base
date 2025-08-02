use alloy_primitives::Address;

// Just implement the converter functions inline for this test
fn address_to_bytes(address_str: &str) -> Result<[u8; 20], String> {
    // Remove "0x" prefix if present
    let hex_str = address_str.strip_prefix("0x").unwrap_or(address_str);
    
    // Check length
    if hex_str.len() != 40 {
        return Err(format!("Invalid address length: expected 40 hex chars, got {}", hex_str.len()));
    }
    
    // Parse hex string to bytes
    let bytes = hex::decode(hex_str)
        .map_err(|e| format!("Invalid hex string: {}", e))?;
    
    // Convert to fixed array
    let mut array = [0u8; 20];
    array.copy_from_slice(&bytes);
    Ok(array)
}

fn format_address_bytes(bytes: &[u8; 20]) -> String {
    let hex_parts: Vec<String> = bytes.iter()
        .map(|b| format!("0x{:02x}", b))
        .collect();
    
    format!("[{}]", hex_parts.join(", "))
}

fn main() {
    // Example address to convert
    let address = "0xe5C17Deb99f15033451b63d2Acf34d840211b3bB";
    
    println!("Converting address: {}", address);
    
    match address_to_bytes(address) {
        Ok(bytes) => {
            println!("Success! Use this in your config:");
            println!("contract_address: Address::from({}),", format_address_bytes(&bytes));
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
    
    // Example with multiple addresses
    println!("\nConverting multiple addresses for accounts/tokens:");
    let addresses = vec![
        "0x72AB388E2E2F6FaceF59E3C3FA2C4E29011c2D38",
        "0x883e4AE0A817f29015009719353b5dD89Aa52184",
    ];
    
    println!("accounts: vec![");
    for addr in addresses {
        match address_to_bytes(addr) {
            Ok(bytes) => {
                println!("    Address::from({}),", format_address_bytes(&bytes));
            }
            Err(e) => {
                eprintln!("    // Error converting {}: {}", addr, e);
            }
        }
    }
    println!("],");
}