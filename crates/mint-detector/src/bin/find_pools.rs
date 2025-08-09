use alloy_primitives::{Address, B256};
use alloy_provider::{Provider, ProviderBuilder, IpcConnect};
use alloy_rpc_types::Filter;
use clap::Parser;
use eyre::Result;
use std::collections::HashSet;
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Token address to find pools for
    #[arg(short, long)]
    token: String,
    
    /// Number of blocks to scan backwards from latest
    #[arg(short, long, default_value_t = 100000)]
    blocks: u64,
    
    /// IPC path
    #[arg(short, long, default_value = "/tmp/op-reth")]
    ipc: String,
}

// Common DEX factory addresses on Base
const UNISWAP_V2_PAIR_CREATED_TOPIC: &str = "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9";
const UNISWAP_V3_POOL_CREATED_TOPIC: &str = "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118";
const AERODROME_PAIR_CREATED_TOPIC: &str = "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9"; // Same as V2

// Known factory addresses on Base
const BASESWAP_FACTORY: &str = "0xFDa619b6d20975be80A10332cD39b9a4b0FAa8BB";
const AERODROME_FACTORY: &str = "0x420DD381b31aEf6683db6B902084cB0FFECe40Da";
const SUSHISWAP_V2_FACTORY: &str = "0x71524B4f93c58fcbF659783284E38825f0622859";
const UNISWAP_V3_FACTORY: &str = "0x33128a8fC17869897dcE68Ed026d694621f6FDfD";

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("info".parse()?))
        .init();
    
    // Parse token address
    let token_addr = args.token.parse::<Address>()?;
    info!("Finding pools for token: {:?}", token_addr);
    
    // Connect to IPC
    let ipc = IpcConnect::new(args.ipc);
    let provider = ProviderBuilder::new()
        .connect_ipc(ipc)
        .await?;
    
    // Get latest block
    let latest_block = provider.get_block_number().await?;
    let start_block = latest_block.saturating_sub(args.blocks);
    
    info!("Scanning blocks {} to {} for pool creations...", start_block, latest_block);
    
    let mut all_pools = HashSet::new();
    
    // Method 1: Find Uniswap V2-style pairs (includes BaseSwap, SushiSwap, Aerodrome)
    info!("Searching for V2-style pairs...");
    let v2_filter = Filter::new()
        .from_block(start_block)
        .to_block(latest_block)
        .event_signature(B256::from_slice(&hex::decode(UNISWAP_V2_PAIR_CREATED_TOPIC.trim_start_matches("0x"))?));
    
    let v2_logs = provider.get_logs(&v2_filter).await?;
    
    for log in v2_logs {
        // PairCreated event: token0, token1, pair, uint
        if log.topics().len() >= 3 {
            let token0 = Address::from_slice(&log.topics()[1][12..]);
            let token1 = Address::from_slice(&log.topics()[2][12..]);
            
            if token0 == token_addr || token1 == token_addr {
                // Extract pair address from data
                if log.data().data.len() >= 32 {
                    let pair = Address::from_slice(&log.data().data[12..32]);
                    all_pools.insert(pair);
                    
                    let other_token = if token0 == token_addr { token1 } else { token0 };
                    info!("Found V2 pair: {:?} with token {:?}", pair, other_token);
                }
            }
        }
    }
    
    // Method 2: Find Uniswap V3 pools
    info!("Searching for V3 pools...");
    let v3_filter = Filter::new()
        .from_block(start_block)
        .to_block(latest_block)
        .event_signature(B256::from_slice(&hex::decode(UNISWAP_V3_POOL_CREATED_TOPIC.trim_start_matches("0x"))?))
        .address(UNISWAP_V3_FACTORY.parse::<Address>()?);
    
    let v3_logs = provider.get_logs(&v3_filter).await?;
    
    for log in v3_logs {
        // PoolCreated event: token0, token1, fee, tickSpacing, pool
        if log.topics().len() >= 3 {
            let token0 = Address::from_slice(&log.topics()[1][12..]);
            let token1 = Address::from_slice(&log.topics()[2][12..]);
            
            if token0 == token_addr || token1 == token_addr {
                // Extract pool address from data (last 20 bytes)
                if log.data().data.len() >= 32 {
                    let pool = Address::from_slice(&log.data().data[log.data().data.len()-20..]);
                    all_pools.insert(pool);
                    
                    let other_token = if token0 == token_addr { token1 } else { token0 };
                    let fee = if log.topics().len() > 3 {
                        u32::from_be_bytes([
                            log.topics()[3][28],
                            log.topics()[3][29],
                            log.topics()[3][30],
                            log.topics()[3][31],
                        ])
                    } else { 0 };
                    
                    info!("Found V3 pool: {:?} with token {:?}, fee: {}bps", pool, other_token, fee/100);
                }
            }
        }
    }
    
    // Method 3: Find pools by scanning for Transfer events to/from known routers
    info!("Searching for pools via Transfer events...");
    
    // Look for Transfer events of our token
    let transfer_topic = B256::from_slice(&hex::decode("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef")?);
    let transfer_filter = Filter::new()
        .from_block(latest_block.saturating_sub(10000)) // Last 10k blocks for transfers
        .to_block(latest_block)
        .address(token_addr)
        .event_signature(transfer_topic);
    
    let transfer_logs = provider.get_logs(&transfer_filter).await?;
    
    let mut potential_pools = HashSet::new();
    for log in transfer_logs {
        if log.topics().len() >= 3 {
            let from = Address::from_slice(&log.topics()[1][12..]);
            let to = Address::from_slice(&log.topics()[2][12..]);
            
            // Check if from/to could be pools (contracts with high activity)
            for addr in [from, to] {
                // Check if it's a contract by looking at code
                let code = provider.get_code_at(addr).await?;
                if code.len() > 100 { // Likely a contract
                    potential_pools.insert(addr);
                }
            }
        }
    }
    
    // Filter potential pools by checking for common pool signatures
    for pool in potential_pools {
        // Try to identify if it's actually a pool
        // Could check for getReserves() or liquidity() methods
        let code = provider.get_code_at(pool).await?;
        
        // Look for common pool method signatures in bytecode
        let code_hex = hex::encode(&code);
        
        // getReserves() - 0x0902f1ac
        // token0() - 0x0dfe1681
        // token1() - 0xd21220a7
        if code_hex.contains("0902f1ac") || // getReserves
           code_hex.contains("0dfe1681") || // token0
           code_hex.contains("ddca3f43") || // fee (V3)
           code_hex.contains("1698ee82") {  // getPool (V3)
            all_pools.insert(pool);
            info!("Found potential pool via transfers: {:?}", pool);
        }
    }
    
    info!("\n=== Summary ===");
    info!("Found {} pools containing token {:?}", all_pools.len(), token_addr);
    
    if !all_pools.is_empty() {
        info!("\nAll pools:");
        for pool in &all_pools {
            info!("  {:?}", pool);
        }
        
        // Save to file
        let output = serde_json::json!({
            "token": format!("{:?}", token_addr),
            "pools": all_pools.iter().map(|p| format!("{:?}", p)).collect::<Vec<_>>(),
            "block_range": {
                "start": start_block,
                "end": latest_block
            }
        });
        
        let filename = format!("pools_{}.json", args.token.trim_start_matches("0x"));
        tokio::fs::write(&filename, serde_json::to_string_pretty(&output)?).await?;
        info!("\nSaved pools to {}", filename);
    }
    
    Ok(())
}