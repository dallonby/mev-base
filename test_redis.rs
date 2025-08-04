use redis::{Client, AsyncCommands};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Redis connection...");
    
    // Get Redis connection details from environment
    let redis_host = std::env::var("REDIS_HOST").unwrap_or_else(|_| "localhost".to_string());
    let redis_port = std::env::var("REDIS_PORT")
        .unwrap_or_else(|_| "6379".to_string())
        .parse::<u16>()
        .unwrap_or(6379);
    let redis_password = std::env::var("REDIS_PASSWORD").unwrap_or_default();
    
    // Build Redis URL
    let redis_url = if redis_password.is_empty() {
        format!("redis://{}:{}/", redis_host, redis_port)
    } else {
        format!("redis://:{}@{}:{}/", redis_password, redis_host, redis_port)
    };
    
    println!("Connecting to: {}:{}", redis_host, redis_port);
    
    // Connect to Redis
    let client = Client::open(redis_url)?;
    let mut con = client.get_async_connection().await?;
    
    println!("✓ Connected to Redis successfully!");
    
    // Test 1: Set and get a simple value
    println!("\nTest 1: Basic set/get");
    let key = "test:mev:simple";
    let value = "Hello MEV!";
    
    con.set(key, value).await?;
    println!("  Set '{}' = '{}'", key, value);
    
    let result: String = con.get(key).await?;
    println!("  Get '{}' = '{}'", key, result);
    assert_eq!(value, result);
    println!("  ✓ Basic set/get works!");
    
    // Test 2: Set with TTL (like gas history)
    println!("\nTest 2: Set with TTL");
    let gas_key = "mev:gas:0x1234567890123456789012345678901234567890";
    let gas_value = "35000000"; // 35M gas
    
    con.set_ex(gas_key, gas_value, 3600).await?; // 1 hour TTL
    println!("  Set '{}' = '{}' with 1 hour TTL", gas_key, gas_value);
    
    let ttl: i64 = con.ttl(gas_key).await?;
    println!("  TTL remaining: {} seconds", ttl);
    assert!(ttl > 3590 && ttl <= 3600);
    println!("  ✓ TTL works correctly!");
    
    // Test 3: Get non-existent key
    println!("\nTest 3: Get non-existent key");
    let missing_key = "mev:gas:0xnonexistent";
    let result: Option<String> = con.get(missing_key).await?;
    println!("  Get '{}' = {:?}", missing_key, result);
    assert_eq!(result, None);
    println!("  ✓ Missing keys return None!");
    
    // Test 4: Simulate gas history workflow
    println!("\nTest 4: Gas history workflow");
    let target_address = "0x940181a94A35A4569E4529A3CDfB74e38FD98631";
    let gas_history_key = format!("mev:gas:{}", target_address);
    
    // First run - no history
    let initial_gas: Option<String> = con.get(&gas_history_key).await?;
    println!("  Initial gas history: {:?}", initial_gas);
    
    // Simulate first run with 50M gas
    let first_run_gas = 50_000_000u64;
    con.set_ex(&gas_history_key, first_run_gas.to_string(), 3600).await?;
    println!("  First run gas: {} (50M)", first_run_gas);
    
    // Get filtered value for second run
    let stored_gas: String = con.get(&gas_history_key).await?;
    let stored_gas_u64: u64 = stored_gas.parse()?;
    println!("  Retrieved gas: {}", stored_gas_u64);
    
    // Apply IIR filter (α = 0.05)
    let second_run_gas = 30_000_000u64; // Second run uses 30M
    let filtered_gas = (second_run_gas as f64 * 0.05 + stored_gas_u64 as f64 * 0.95) as u64;
    println!("  Second run gas: {} (30M)", second_run_gas);
    println!("  Filtered gas: {} (IIR with α=0.05)", filtered_gas);
    
    // Store updated filtered value
    con.set_ex(&gas_history_key, filtered_gas.to_string(), 3600).await?;
    println!("  ✓ Gas history workflow complete!");
    
    // Test 5: Pub/Sub (for transaction broadcasting)
    println!("\nTest 5: Pub/Sub test");
    let channel = "baseTransactionBroadcast";
    let test_tx = r#"{"signedTx": "0x123..."}"#;
    
    let published: i32 = con.publish(channel, test_tx).await?;
    println!("  Published to '{}': {} subscribers", channel, published);
    println!("  ✓ Pub/Sub works!");
    
    // Cleanup
    println!("\nCleaning up test keys...");
    con.del(key).await?;
    con.del(gas_key).await?;
    con.del(&gas_history_key).await?;
    println!("✓ Cleanup complete!");
    
    println!("\n✅ All Redis tests passed!");
    
    Ok(())
}