use std::sync::Arc;
use tokio::sync::mpsc;
use deadpool_postgres::{Config, Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use tracing::{info, error, warn, debug};
use chrono::{DateTime, Utc};
use alloy_primitives::TxHash;

/// Transaction log entry to be inserted into the database
#[derive(Debug, Clone)]
pub struct TransactionLog {
    pub hash: TxHash,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub block_number: u64,
}

/// Message types for the database actor
#[derive(Debug)]
enum DatabaseMessage {
    LogBatch(Vec<TransactionLog>),
    Shutdown,
}

/// Database service that runs in its own thread
#[derive(Clone)]
pub struct DatabaseService {
    sender: mpsc::Sender<DatabaseMessage>,
}

impl DatabaseService {
    /// Create a new database service and spawn the worker thread
    pub async fn new() -> eyre::Result<Self> {
        // Create channel for communication
        let (tx, rx) = mpsc::channel(1000);
        
        // Spawn the database worker thread
        tokio::spawn(database_worker(rx));
        
        Ok(Self { sender: tx })
    }
    
    /// Log a batch of transactions asynchronously
    pub async fn log_transactions(&self, logs: Vec<TransactionLog>) -> eyre::Result<()> {
        if logs.is_empty() {
            return Ok(());
        }
        
        self.sender.send(DatabaseMessage::LogBatch(logs)).await
            .map_err(|_| eyre::eyre!("Database service channel closed"))?;
        Ok(())
    }
    
    /// Shutdown the database service
    pub async fn shutdown(self) -> eyre::Result<()> {
        self.sender.send(DatabaseMessage::Shutdown).await
            .map_err(|_| eyre::eyre!("Failed to send shutdown signal"))?;
        Ok(())
    }
}

/// The main database worker that runs in its own thread
async fn database_worker(mut rx: mpsc::Receiver<DatabaseMessage>) {
    info!("Database worker thread started");
    
    // Try to initialize the connection pool
    let pool = match create_pool().await {
        Ok(pool) => {
            info!("PostgreSQL connection pool initialized");
            Some(pool)
        }
        Err(e) => {
            error!("Failed to initialize PostgreSQL pool: {}. Transaction logging disabled.", e);
            None
        }
    };
    
    // Process messages
    while let Some(msg) = rx.recv().await {
        match msg {
            DatabaseMessage::LogBatch(logs) => {
                if let Some(ref pool) = pool {
                    if let Err(e) = insert_transaction_batch(pool, logs).await {
                        error!("Failed to insert transaction batch: {}", e);
                    }
                }
            }
            DatabaseMessage::Shutdown => {
                info!("Database worker shutting down");
                break;
            }
        }
    }
    
    info!("Database worker thread stopped");
}

/// Create PostgreSQL connection pool
async fn create_pool() -> eyre::Result<Pool> {
    // Load configuration from environment
    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("POSTGRES_PORT")
        .unwrap_or_else(|_| "5432".to_string())
        .parse::<u16>()
        .unwrap_or(5432);
    let user = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "backrunner".to_string());
    let password = std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "backrunner_password".to_string());
    let dbname = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "backrunner_db".to_string());
    let pool_size = std::env::var("POSTGRES_POOL_SIZE")
        .unwrap_or_else(|_| "10".to_string())
        .parse::<usize>()
        .unwrap_or(10);
    
    debug!(
        host = %host,
        port = port,
        user = %user,
        dbname = %dbname,
        pool_size = pool_size,
        "Connecting to PostgreSQL"
    );
    
    // Create pool configuration
    let mut cfg = Config::new();
    cfg.host = Some(host);
    cfg.port = Some(port);
    cfg.user = Some(user);
    cfg.password = Some(password);
    cfg.dbname = Some(dbname);
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    
    // Create the pool
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    
    // Test the connection
    let client = pool.get().await?;
    client.query_one("SELECT 1", &[]).await?;
    
    Ok(pool)
}

/// Insert a batch of transactions into the database
async fn insert_transaction_batch(pool: &Pool, logs: Vec<TransactionLog>) -> eyre::Result<()> {
    let start = std::time::Instant::now();
    let batch_size = logs.len();
    
    // Get a connection from the pool
    let mut client = pool.get().await?;
    
    // Start a transaction
    let tx = client.transaction().await?;
    
    // Prepare the statement once
    let stmt = tx.prepare(
        "INSERT INTO transaction_logs (hash, source, timestamp, block_number, sources)
         VALUES ($1, $2::character varying(50), $3, $4, ARRAY[$2]::character varying(50)[])
         ON CONFLICT (hash) DO UPDATE
         SET sources = CASE 
           WHEN $2 = ANY(transaction_logs.sources) THEN transaction_logs.sources
           ELSE array_append(transaction_logs.sources, $2::character varying(50))
         END"
    ).await?;
    
    // Execute batch insert
    for log in logs {
        let hash_str = format!("{:?}", log.hash);
        tx.execute(&stmt, &[
            &hash_str,
            &log.source,
            &log.timestamp,
            &(log.block_number as i64),
        ]).await?;
    }
    
    // Commit the transaction
    tx.commit().await?;
    
    let elapsed = start.elapsed();
    debug!(
        batch_size = batch_size,
        elapsed_ms = elapsed.as_millis(),
        "Inserted transaction batch"
    );
    
    Ok(())
}