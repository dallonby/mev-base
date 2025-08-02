use reth_metrics::{
    metrics::{Counter, Gauge, Histogram},
    Metrics,
};

/// MEV metrics for tracking lifecycle timings and performance
#[derive(Metrics)]
#[metrics(scope = "mev")]
pub struct MevMetrics {
    /// Total number of flashblocks received
    pub flashblocks_received_total: Counter,
    
    /// Total number of MEV opportunities found
    pub opportunities_found_total: Counter,
    
    /// Total number of MEV opportunities above threshold
    pub opportunities_profitable_total: Counter,
    
    /// Total number of transactions submitted
    pub transactions_submitted_total: Counter,
    
    /// Current active MEV workers
    pub active_workers: Gauge,
    
    /// Flashblock processing latency (websocket to processing start)
    pub flashblock_queue_latency_seconds: Histogram,
    
    /// Flashblock execution time
    pub flashblock_execution_duration_seconds: Histogram,
    
    /// State export duration
    pub state_export_duration_seconds: Histogram,
    
    /// Strategy analysis duration
    pub strategy_analysis_duration_seconds: Histogram,
    
    /// Total flashblock processing time (websocket to workers spawned)
    pub flashblock_total_duration_seconds: Histogram,
}

/// Per-strategy MEV metrics
#[derive(Metrics, Clone)]
#[metrics(scope = "mev.strategy")]
pub struct MevStrategyMetrics {
    /// Number of times this strategy was triggered
    pub triggered_total: Counter,
    
    /// Number of profitable opportunities found
    pub profitable_total: Counter,
    
    /// Total worker execution time (including setup)
    pub worker_duration_seconds: Histogram,
    
    /// Gradient optimization duration
    pub gradient_duration_seconds: Histogram,
    
    /// Transaction simulation duration
    pub simulation_duration_seconds: Histogram,
    
    /// Profit amount in wei (as histogram to track distribution)
    pub profit_wei: Histogram,
}

/// Global MEV metrics instance
pub static MEV_METRICS: std::sync::LazyLock<MevMetrics> = std::sync::LazyLock::new(MevMetrics::default);

/// Get or create strategy-specific metrics
pub fn get_strategy_metrics(strategy_name: &str) -> MevStrategyMetrics {
    static STRATEGY_METRICS: std::sync::LazyLock<dashmap::DashMap<String, MevStrategyMetrics>> = 
        std::sync::LazyLock::new(dashmap::DashMap::new);
    
    STRATEGY_METRICS
        .entry(strategy_name.to_string())
        .or_insert_with(MevStrategyMetrics::default)
        .clone()
}