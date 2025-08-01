use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use std::env;

/// Initialize the logging system with environment-based configuration
/// 
/// The log level can be configured via environment variables:
/// - `RUST_LOG`: Standard Rust logging configuration (e.g., "debug", "info", "warn", "error")
/// - `MEV_LOG`: MEV-specific logging configuration (overrides RUST_LOG if set)
/// 
/// Examples:
/// - `MEV_LOG=debug` - Show all debug messages
/// - `MEV_LOG=info` - Show info and above (default)
/// - `MEV_LOG=warn` - Show warnings and errors only
/// - `MEV_LOG=error` - Show errors only
/// - `MEV_LOG=trace` - Show everything including trace messages
/// 
/// You can also use module-specific filtering:
/// - `MEV_LOG=mevbase=debug,revm=trace` - Debug for mevbase, trace for revm
/// - `MEV_LOG=info,mevbase::mev_task_worker=debug` - Info globally, debug for mev_task_worker
pub fn init_logging() {
    // Try to load .env file if it exists
    let _ = dotenv::dotenv();
    
    // Check for MEV_LOG first, then fall back to RUST_LOG
    let filter = match env::var("MEV_LOG") {
        Ok(mev_log) => mev_log,
        Err(_) => env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
    };
    
    // Build the filter
    let env_filter = EnvFilter::try_new(&filter)
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    // Configure the formatter
    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_level(true)
        .with_ansi(true)
        .compact();
    
    // Initialize the subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
    
    // Log the initialized configuration
    tracing::info!(
        log_filter = %filter,
        "Logging system initialized"
    );
}