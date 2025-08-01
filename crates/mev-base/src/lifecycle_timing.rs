use std::time::Instant;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Tracks timing through the entire MEV pipeline lifecycle
#[derive(Clone)]
pub struct LifecycleTiming {
    /// When the flashblock was sent by the server (if available from websocket)
    pub server_sent_time: Option<Instant>,
    /// When we received the websocket message
    pub websocket_received: Instant,
    /// When flashblock processing started
    pub processing_started: Option<Instant>,
    /// When flashblock execution completed
    pub execution_completed: Option<Instant>,
    /// When state export completed
    pub state_export_completed: Option<Instant>,
    /// When MEV strategy analysis completed
    pub strategy_analysis_completed: Option<Instant>,
    /// When MEV workers were spawned
    pub workers_spawned: Option<Instant>,
    /// When gradient optimization started (per worker)
    pub gradient_started: Option<Instant>,
    /// When gradient optimization completed (per worker)
    pub gradient_completed: Option<Instant>,
    /// Block and flashblock info
    pub block_number: u64,
    pub flashblock_index: u32,
}

impl LifecycleTiming {
    pub fn new(websocket_received: Instant, block_number: u64, flashblock_index: u32) -> Self {
        Self {
            server_sent_time: None,
            websocket_received,
            processing_started: None,
            execution_completed: None,
            state_export_completed: None,
            strategy_analysis_completed: None,
            workers_spawned: None,
            gradient_started: None,
            gradient_completed: None,
            block_number,
            flashblock_index,
        }
    }
    
    /// Generate a comprehensive timing report
    pub fn generate_report(&self) -> String {
        let mut report = format!("\n📊 Lifecycle Timing Report - Block {} Flashblock {}\n", 
            self.block_number, self.flashblock_index);
        report.push_str("═══════════════════════════════════════════════════════\n");
        
        let base_time = self.websocket_received;
        
        // Network latency (if server timestamp available)
        if let Some(server_time) = self.server_sent_time {
            let network_latency = self.websocket_received.duration_since(server_time).as_secs_f64() * 1000.0;
            report.push_str(&format!("📡 Network Latency:          {:.2}ms\n", network_latency));
        }
        
        // Processing stages
        if let Some(proc_start) = self.processing_started {
            let queue_time = proc_start.duration_since(base_time).as_secs_f64() * 1000.0;
            report.push_str(&format!("⏳ Queue Time:               {:.2}ms\n", queue_time));
            
            if let Some(exec_complete) = self.execution_completed {
                let exec_time = exec_complete.duration_since(proc_start).as_secs_f64() * 1000.0;
                report.push_str(&format!("🔥 Flashblock Execution:     {:.2}ms\n", exec_time));
            }
            
            if let Some(export_complete) = self.state_export_completed {
                let export_time = export_complete.duration_since(
                    self.execution_completed.unwrap_or(proc_start)
                ).as_secs_f64() * 1000.0;
                report.push_str(&format!("📸 State Export:             {:.2}ms\n", export_time));
            }
            
            if let Some(analysis_complete) = self.strategy_analysis_completed {
                let analysis_time = analysis_complete.duration_since(
                    self.state_export_completed.unwrap_or(proc_start)
                ).as_secs_f64() * 1000.0;
                report.push_str(&format!("🎯 Strategy Analysis:        {:.2}ms\n", analysis_time));
            }
            
            if let Some(workers_spawn) = self.workers_spawned {
                let spawn_time = workers_spawn.duration_since(
                    self.strategy_analysis_completed.unwrap_or(proc_start)
                ).as_secs_f64() * 1000.0;
                report.push_str(&format!("🚀 Worker Spawn:             {:.2}ms\n", spawn_time));
            }
            
            if let Some(grad_start) = self.gradient_started {
                let pre_gradient = grad_start.duration_since(base_time).as_secs_f64() * 1000.0;
                report.push_str(&format!("⏱️  Pre-Gradient Total:       {:.2}ms\n", pre_gradient));
                
                if let Some(grad_complete) = self.gradient_completed {
                    let gradient_time = grad_complete.duration_since(grad_start).as_secs_f64() * 1000.0;
                    report.push_str(&format!("📈 Gradient Optimization:    {:.2}ms\n", gradient_time));
                    
                    let total_time = grad_complete.duration_since(base_time).as_secs_f64() * 1000.0;
                    report.push_str(&format!("✅ Total Pipeline:           {:.2}ms\n", total_time));
                }
            }
        }
        
        report.push_str("═══════════════════════════════════════════════════════\n");
        report
    }
}

/// Global timing tracker for the current flashblock
pub type TimingTracker = Arc<Mutex<Option<LifecycleTiming>>>;

pub fn create_timing_tracker() -> TimingTracker {
    Arc::new(Mutex::new(None))
}