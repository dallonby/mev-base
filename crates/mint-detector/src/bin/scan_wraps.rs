use alloy_primitives::{Address, U256, B256, Bytes, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder, IpcConnect};
use alloy_rpc_types::{Filter, TransactionTrait};
use clap::Parser;
use eyre::Result;
use mint_detector::template::MintTemplate;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tracing::{info, debug};
use serde::{Deserialize, Serialize};
use serde_json;
use futures::stream::{self, StreamExt};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Start block number (if not provided, starts from latest - blocks)
    #[arg(short, long)]
    start: Option<u64>,
    
    /// Scan from latest block backwards
    #[arg(short = 'l', long)]
    latest: bool,
    
    /// Number of blocks to scan
    #[arg(short, long, default_value_t = 100)]
    blocks: u64,
    
    /// IPC path
    #[arg(short, long, default_value = "/tmp/op-reth")]
    ipc: String,
    
    /// Number of concurrent traces (optimal: 50-100 for IPC)
    #[arg(short = 't', long, default_value_t = 50)]
    trace_concurrency: usize,
    
    /// Number of worker threads for CPU-bound analysis
    #[arg(short = 'w', long, default_value_t = 8)]
    workers: usize,
}

// Placeholders for calldata template generation
const SIMILAR_TO_PLACEHOLDER: &str = "1b1b1b1b1b1b1b1b2b2b2b2b5b1b1b1b";
const SIMILAR_FROM_PLACEHOLDER: &str = "1c1c1c1c1c1c1c1c2c2c2c2c5c1c1c1c";
const CALLFROM_PLACEHOLDER: &str = "1414141414141414141414142424242424141414";
const CALLTO_PLACEHOLDER: &str = "1313131313131313131313132323232333131313";
const TXSENDER_PLACEHOLDER: &str = "1212121212121212121212122222222212121212";
const QUANT_FROM_PLACEHOLDER: &str = "16161616161616162626262646161616";
const QUANT_TO_PLACEHOLDER: &str = "15151515151515152525252555151515";
const OTHER_EVENT_FROM: &str = "1717171717171717171717172727272747171717";
const OTHER_EVENT_TO: &str = "1818181818181818181818182828282858181818";
const EVENT_FROM: &str = "1919191919191919191919192929292969191919";
const EVENT_TO: &str = "1a1a1a1a1a1a1a1a1a1a1a1a2a2a2a2a7a1a1a1a";

// ERC20 Transfer event topic
const TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

// Ignored tokens
const IGNORED_TOKENS: &[&str] = &[
    "0x030ba81f1c18d280636f32af80b9aad02cf0854e",
    "0x7acdf2012aac69d70b86677fe91eb66e08961880",
    "0x36d3ca43ae7939645c306e26603ce16e39a89192",
    "0xaa3d9118dab202ba5ea98018b98f49c0d1abd329",  // Added per user request
    "0x903cd4e618cd8c9d585436264edec3c1874bfc57",  // Added per user request
    "0x820c137fa70c8691f0e44dc420a5e53c168921dc",  // Added per user request - ignore if to or from
    "0xa040a8564c433970d7919c441104b1d25b9eaa1c",  // Added per user request - ignore if to or from
];

// ERC-4337 EntryPoint contract on Base - ignore all transactions to this
const ERC4337_ENTRYPOINT: &str = "0x0000000071727de22e5e9d8baf0edac6f37da032";

// Additional contract to ignore (possibly another bundler/aggregator)
const IGNORE_CONTRACT: &str = "0x1e33f2c390fd3fb03f4908463f57d9929377176b";

// Function selectors to ignore (not wrap/unwrap operations)
const ADD_LIQUIDITY_SELECTOR: &str = "0xe8078d94"; // addLiquidity

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CallFrame {
    #[serde(rename = "type")]
    call_type: String,
    from: Address,
    to: Option<Address>,
    value: Option<String>,
    gas: Option<String>,
    #[serde(rename = "gasUsed")]
    gas_used: Option<String>,
    input: Option<String>,
    output: Option<String>,
    error: Option<String>,
    calls: Option<Vec<CallFrame>>,
    logs: Option<Vec<TraceLog>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TraceLog {
    address: Address,
    topics: Vec<FixedBytes<32>>,
    data: String,
}

#[derive(Debug, Clone)]
struct Erc20Event {
    from: Address,
    to: Address,
    token: Address,
    amount: U256,
    guid: String,
}

#[derive(Debug, Clone)]
struct PossibleMint {
    mint_type: String,
    #[allow(dead_code)]
    tx_hash: B256,
    from_token: Address,
    to_token: Address,
    amount_from: U256,
    amount_to: U256,
    original_call_data: String,
    modified_call_data: String,
    #[allow(dead_code)]
    call_from: Address,
    call_to: Address,
    from_symbol: Option<String>,
    to_symbol: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("info".parse()?))
        .init();
    
    info!("Connecting to IPC at: {}", args.ipc);
    info!("Using {} concurrent traces, {} worker threads", args.trace_concurrency, args.workers);
    
    // Connect to IPC
    let ipc = IpcConnect::new(args.ipc);
    let provider = Arc::new(
        ProviderBuilder::new()
            .connect_ipc(ipc)
            .await?
    );
    
    // Get latest block
    let latest_block = provider.get_block_number().await?;
    info!("Latest block: {}", latest_block);
    
    // Determine block range
    let (start_block, end_block) = if args.latest {
        // Scan from latest block backwards
        (latest_block.saturating_sub(args.blocks), latest_block)
    } else if let Some(start) = args.start {
        // Scan from specified start block
        (start, (start + args.blocks).min(latest_block))
    } else {
        // Default: scan latest blocks
        (latest_block.saturating_sub(args.blocks), latest_block)
    };
    
    info!("Scanning blocks {} to {} for ZeroOrDead burn patterns (transfers to 0x0 or 0xdead)...", 
        start_block, end_block);
    
    // Create filter for ERC20 Transfer events
    let filter = Filter::new()
        .from_block(start_block)
        .to_block(end_block)
        .event_signature(B256::from_slice(&hex::decode(TRANSFER_TOPIC.trim_start_matches("0x"))?));
    
    info!("Fetching all ERC20 Transfer events in block range...");
    let start_time = std::time::Instant::now();
    let logs = provider.get_logs(&filter).await?;
    info!("Found {} Transfer events in {:.2}s", logs.len(), start_time.elapsed().as_secs_f64());
    
    // Collect unique transaction hashes that have transfers
    let mut tx_hashes: HashSet<B256> = HashSet::new();
    for log in &logs {
        if let Some(tx_hash) = log.transaction_hash {
            tx_hashes.insert(tx_hash);
        }
    }
    
    info!("Found {} unique transactions with ERC20 transfers", tx_hashes.len());
    
    let all_mints = Arc::new(Mutex::new(Vec::<PossibleMint>::new()));
    let templates = Arc::new(Mutex::new(HashMap::<String, MintTemplate>::new()));
    
    // Use semaphore to limit concurrent traces
    let semaphore = Arc::new(Semaphore::new(args.trace_concurrency));
    
    // Process all transactions concurrently with controlled parallelism
    let tx_list: Vec<B256> = tx_hashes.into_iter().collect();
    let trace_start = std::time::Instant::now();
    
    // Create a stream of futures and process them concurrently
    let futures = stream::iter(tx_list.into_iter().map(|tx_hash| {
        let provider = provider.clone();
        let all_mints = all_mints.clone();
        let templates = templates.clone();
        let semaphore = semaphore.clone();
        
        async move {
            // Acquire permit before tracing
            let _permit = semaphore.acquire().await.unwrap();
            
            trace_and_analyze_transaction(
                provider,
                tx_hash,
                all_mints,
                templates
            ).await
        }
    }))
    .buffer_unordered(args.trace_concurrency)
    .collect::<Vec<_>>();
    
    // Wait for all traces to complete
    futures.await;
    
    info!("Traced all transactions in {:.2}s", trace_start.elapsed().as_secs_f64());
    
    // Print summary
    let mints_count = all_mints.lock().await.len();
    let templates_count = templates.lock().await.len();
    
    info!("\n=== Summary ===");
    info!("Found {} transactions with ZeroOrDead burn patterns", mints_count);
    info!("Generated {} unique calldata templates", templates_count);
    info!("Total processing time: {:.2}s", start_time.elapsed().as_secs_f64());
    
    if templates_count > 0 {
        info!("\n=== Templates ===");
        let templates = templates.lock().await;
        for (key, template) in templates.iter().take(10) {
            info!("\nTemplate for {}:", key);
            info!("  Type: {}", template.mint_type);
            info!("  Contract: {:?}", template.contract);
            info!("  From token: {:?}", template.from_token);
            info!("  To token: {:?}", template.to_token);
            info!("  Template: {}", &template.calldata_template[..100.min(template.calldata_template.len())]);
            if let Some(tx) = &template.example_tx {
                info!("  Example tx: {}", tx);
            }
        }
        
        // Save to JSON file
        let output = serde_json::to_string_pretty(&*templates)?;
        tokio::fs::write("wrap_templates.json", output).await?;
        info!("\nSaved {} templates to wrap_templates.json", templates_count);
    }
    
    Ok(())
}

async fn trace_and_analyze_transaction(
    provider: Arc<impl Provider + 'static>,
    tx_hash: B256,
    all_mints: Arc<Mutex<Vec<PossibleMint>>>,
    templates: Arc<Mutex<HashMap<String, MintTemplate>>>,
) -> Result<()> {
    debug!("Tracing transaction {:?}", tx_hash);
    
    // Get transaction details
    let tx = match provider.get_transaction_by_hash(tx_hash).await? {
        Some(tx) => tx,
        None => {
            debug!("Transaction {:?} not found", tx_hash);
            return Ok(());
        }
    };
    
    // Skip ERC-4337 transactions and other ignored contracts
    if let Some(to) = tx.to() {
        let to_str = format!("{:?}", to).to_lowercase();
        if to_str.contains(ERC4337_ENTRYPOINT.trim_start_matches("0x")) {
            debug!("Skipping ERC-4337 transaction to EntryPoint: {:?}", tx_hash);
            return Ok(());
        }
        if to_str.contains(IGNORE_CONTRACT.trim_start_matches("0x")) {
            debug!("Skipping transaction to ignored contract: {:?}", tx_hash);
            return Ok(());
        }
    }
    
    // Skip liquidity operations (not wrap/unwrap MEV)
    let input = tx.input();
    if input.len() >= 4 {
        let selector = format!("0x{}", hex::encode(&input[0..4]));
        if selector == ADD_LIQUIDITY_SELECTOR {
            debug!("Skipping addLiquidity transaction: {:?}", tx_hash);
            return Ok(());
        }
    }
    
    // Get transaction trace using debug_traceTransaction with logs
    let trace_result = provider.raw_request::<_, serde_json::Value>(
        "debug_traceTransaction".into(),
        (
            format!("{:?}", tx_hash),
            serde_json::json!({
                "tracer": "callTracer",
                "tracerConfig": {
                    "onlyTopCall": false,
                    "withLog": true,
                    "enableReturnData": true
                }
            })
        )
    ).await;
    
    let trace = match trace_result {
        Ok(v) => v,
        Err(e) => {
            debug!("Failed to trace transaction {}: {}", tx_hash, e);
            return Ok(());
        }
    };
    
    // Parse the trace as a call frame
    let mut call_frame: CallFrame = match serde_json::from_value(trace) {
        Ok(frame) => frame,
        Err(e) => {
            debug!("Failed to parse trace for {}: {}", tx_hash, e);
            return Ok(());
        }
    };
    
    // CPU-bound work: spawn to blocking thread pool
    let (call_frame, tx, tx_hash) = tokio::task::spawn_blocking(move || {
        // Combine logs from all children calls into parent's logs (like TypeScript)
        combine_logs_recursive(&mut call_frame);
        (call_frame, tx, tx_hash)
    }).await?;
    
    // Analyze the call frame for wrap/unwrap patterns
    let mut hashes_seen = HashSet::new();
    let possible_mints = analyze_calls_for_mints(
        &call_frame,
        None,
        &tx,
        &mut hashes_seen,
        tx_hash
    );
    
    if !possible_mints.is_empty() {
        debug!("Found {} patterns in tx {:?}", possible_mints.len(), tx_hash);
        
        for mint in &possible_mints {
            // Skip ignored tokens
            if IGNORED_TOKENS.iter().any(|&t| {
                let addr = t.trim_start_matches("0x");
                format!("{:?}", mint.from_token).contains(addr) || 
                format!("{:?}", mint.to_token).contains(addr)
            }) {
                continue;
            }
            
            debug!("  {}: {} -> {} (amounts: {} -> {}) via {}", 
                mint.mint_type,
                mint.from_token, 
                mint.to_token, 
                mint.amount_from, 
                mint.amount_to,
                mint.call_to
            );
            
            // Create template
            let template = MintTemplate {
                from_token: mint.from_token,
                to_token: mint.to_token,
                contract: mint.call_to,
                calldata_template: mint.modified_call_data.clone(),
                original_calldata: Bytes::from(hex::decode(&mint.original_call_data.trim_start_matches("0x")).unwrap_or_default()),
                from_symbol: mint.from_symbol.clone(),
                to_symbol: mint.to_symbol.clone(),
                example_tx: Some(format!("{:?}", tx_hash)),
                mint_type: mint.mint_type.clone(),
            };
            
            let key = format!("{:?}-{:?}", mint.from_token, mint.to_token);
            templates.lock().await.insert(key, template);
        }
        
        // Store mints
        all_mints.lock().await.extend(possible_mints);
    }
    
    Ok(())
}

/// Combine logs from all children calls into parent's logs (recursive)
fn combine_logs_recursive(call_frame: &mut CallFrame) {
    if let Some(calls) = &mut call_frame.calls {
        for call in calls.iter_mut() {
            // First recurse into children
            combine_logs_recursive(call);
            
            // Then add child's logs to parent
            if let Some(child_logs) = &call.logs {
                if call_frame.logs.is_none() {
                    call_frame.logs = Some(Vec::new());
                }
                
                for log in child_logs {
                    // Add a guid (md5 hash) to each log for uniqueness
                    let log_with_guid = log.clone();
                    // In real implementation, would use md5 hash here
                    call_frame.logs.as_mut().unwrap().push(log_with_guid);
                }
            }
        }
    }
}

/// Analyze calls recursively to find mint/burn patterns
fn analyze_calls_for_mints(
    call_frame: &CallFrame,
    _parent: Option<&CallFrame>,
    tx: &alloy_rpc_types::Transaction,
    hashes_seen: &mut HashSet<String>,
    tx_hash: B256,
) -> Vec<PossibleMint> {
    let mut possible_mints = Vec::new();
    
    if let Some(calls) = &call_frame.calls {
        for call in calls {
            // Recurse into children first
            let child_mints = analyze_calls_for_mints(
                call,
                Some(call_frame),
                tx,
                hashes_seen,
                tx_hash
            );
            possible_mints.extend(child_mints);
            
            // Collect ERC20 events from this call's logs
            let call_erc20_events = extract_erc20_events(call);
            
            // Analyze pairs of ERC20 events for patterns
            for (i, event) in call_erc20_events.iter().enumerate() {
                for other_event in call_erc20_events.iter().skip(i + 1) {
                    if let Some(mint) = analyze_event_pair(
                        event,
                        other_event,
                        call,
                        call_frame,
                        tx,
                        hashes_seen,
                        tx_hash
                    ) {
                        possible_mints.push(mint);
                    }
                }
            }
        }
    }
    
    possible_mints
}

/// Extract ERC20 Transfer events from a call's logs
fn extract_erc20_events(call: &CallFrame) -> Vec<Erc20Event> {
    let mut events = Vec::new();
    
    if let Some(logs) = &call.logs {
        for log in logs {
            // Check if this is an ERC20 Transfer event
            if log.topics.len() == 3 && 
               format!("{:?}", log.topics[0]) == TRANSFER_TOPIC {
                
                let from = Address::from_slice(&log.topics[1][12..]);
                let to = Address::from_slice(&log.topics[2][12..]);
                let token = log.address;
                
                // Parse amount from data
                let amount = if log.data.len() >= 66 {  // "0x" + 64 hex chars
                    let data = log.data.trim_start_matches("0x");
                    U256::from_str_radix(data, 16).unwrap_or(U256::ZERO)
                } else {
                    U256::ZERO
                };
                
                if amount > U256::ZERO {
                    events.push(Erc20Event {
                        from,
                        to,
                        token,
                        amount,
                        guid: format!("{:?}-{}", log.address, log.data),
                    });
                }
            }
        }
    }
    
    events
}

/// Analyze a pair of ERC20 events for mint/burn patterns
fn analyze_event_pair(
    event: &Erc20Event,
    other_event: &Erc20Event,
    call: &CallFrame,
    parent: &CallFrame,
    _tx: &alloy_rpc_types::Transaction,
    hashes_seen: &mut HashSet<String>,
    tx_hash: B256,
) -> Option<PossibleMint> {
    // Skip if same token
    if event.token == other_event.token {
        return None;
    }
    
    // Create hash to avoid duplicates
    let hash = format!("{:?}{}{}", tx_hash, event.guid, other_event.guid);
    if hashes_seen.contains(&hash) {
        return None;
    }
    hashes_seen.insert(hash);
    
    let mint_type;
    let (from_token, to_token, amount_from, amount_to);
    
    // ONLY check for burn pattern (to = 0x0 or dead)
    if event.to == Address::ZERO || 
       format!("{:?}", event.to).contains("dead") {
        
        // Look for transfer TO the event's from address
        if other_event.to == event.from {
            mint_type = "ZeroOrDead".to_string();
            from_token = event.token;
            to_token = other_event.token;
            amount_from = other_event.amount;
            amount_to = event.amount;
        } else {
            return None;
        }
    } else {
        // Skip all other patterns (QuantMatch, SwapLike, etc.)
        return None;
    }
    
    // Generate calldata template
    let call_data = call.input.as_ref()?.clone();
    let modified_call_data = generate_calldata_template(
        &call_data,
        call,
        parent,
        event,
        other_event,
        amount_from,
        amount_to
    );
    
    Some(PossibleMint {
        mint_type,
        tx_hash,
        from_token,
        to_token,
        amount_from,
        amount_to,
        original_call_data: call_data.to_lowercase(),
        modified_call_data,
        call_from: call.from,
        call_to: call.to.unwrap_or_default(),
        from_symbol: None,
        to_symbol: None,
    })
}

/// Check if two quantities match within 85-115% range
fn check_quantity_match(amount_a: U256, amount_b: U256) -> bool {
    let from_quant = amount_a.to_string();
    let to_quant = amount_b.to_string();
    let from_len = from_quant.len();
    let to_len = to_quant.len();
    let shortest_len = from_len.min(to_len);
    let len_to_compare = ((shortest_len as f64) * 0.95).ceil() as usize;
    
    from_quant[..len_to_compare.min(from_len)] == to_quant[..len_to_compare.min(to_len)]
}

/// Generate calldata template with placeholders
fn generate_calldata_template(
    call_data: &str,
    call: &CallFrame,
    parent: &CallFrame,
    event: &Erc20Event,
    other_event: &Erc20Event,
    amount_from: U256,
    amount_to: U256,
) -> String {
    let mut template = call_data.to_lowercase();
    
    // Handle proxy calls (when parent's data matches call's data)
    let (actual_from, actual_to) = if parent.input.as_deref() == Some(call_data) {
        (parent.from, call.from)
    } else {
        (call.from, call.to.unwrap_or_default())
    };
    
    // Replace addresses and amounts with placeholders
    // For now, we'll use the call's from address as tx sender since we can't easily get it from the Transaction type
    // In the TypeScript, tx.from is available directly
    let tx_sender = format!("{:x}", call.from).to_lowercase();
    let call_from = format!("{:x}", actual_from).to_lowercase();
    let call_to = format!("{:x}", actual_to).to_lowercase();
    let quant_from = format!("{:064x}", amount_from);
    let quant_to = format!("{:064x}", amount_to);
    
    template = template.replace(&tx_sender, TXSENDER_PLACEHOLDER);
    template = template.replace(&call_from, CALLFROM_PLACEHOLDER);
    template = template.replace(&call_to, CALLTO_PLACEHOLDER);
    template = template.replace(&quant_from, QUANT_FROM_PLACEHOLDER);
    template = template.replace(&quant_to, QUANT_TO_PLACEHOLDER);
    
    // Replace event addresses
    if other_event.from != Address::ZERO {
        let addr = format!("{:x}", other_event.from).to_lowercase();
        template = template.replace(&addr, OTHER_EVENT_FROM);
    }
    if other_event.to != Address::ZERO {
        let addr = format!("{:x}", other_event.to).to_lowercase();
        template = template.replace(&addr, OTHER_EVENT_TO);
    }
    if event.from != Address::ZERO {
        let addr = format!("{:x}", event.from).to_lowercase();
        template = template.replace(&addr, EVENT_FROM);
    }
    if event.to != Address::ZERO {
        let addr = format!("{:x}", event.to).to_lowercase();
        template = template.replace(&addr, EVENT_TO);
    }
    
    // Split calldata into chunks and look for similar quantities
    let chunks = split_calldata_into_chunks(&template);
    for chunk in chunks {
        if look_for_similar_quantities(amount_to, &chunk) {
            template = template.replace(&chunk, SIMILAR_TO_PLACEHOLDER);
        } else if look_for_similar_quantities(amount_from, &chunk) {
            template = template.replace(&chunk, SIMILAR_FROM_PLACEHOLDER);
        }
    }
    
    template
}

/// Split calldata into 32-byte chunks (after removing function selector)
fn split_calldata_into_chunks(call_data: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let data = call_data.trim_start_matches("0x");
    
    // Skip function selector (first 8 chars)
    let data = if data.len() > 8 {
        &data[8..]
    } else {
        return chunks;
    };
    
    // Split into 32-byte (64 hex char) chunks
    for i in (0..data.len()).step_by(64) {
        let end = (i + 64).min(data.len());
        chunks.push(data[i..end].to_string());
    }
    
    chunks
}

/// Check if a chunk contains a similar quantity (within 85-115% range)
fn look_for_similar_quantities(quantity_to_find: U256, chunk: &str) -> bool {
    if let Ok(chunk_value) = U256::from_str_radix(chunk, 16) {
        if chunk_value == quantity_to_find {
            return true;
        }
        
        // Check if within 85-115% range
        let lower_bound = quantity_to_find.saturating_mul(U256::from(85)) / U256::from(100);
        let upper_bound = quantity_to_find.saturating_mul(U256::from(115)) / U256::from(100);
        
        chunk_value >= lower_bound && chunk_value <= upper_bound
    } else {
        false
    }
}