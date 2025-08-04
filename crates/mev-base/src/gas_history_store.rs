use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client as RedisClient};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};
use alloy_primitives::Address;

/// Store for tracking gas usage history per target address
pub struct GasHistoryStore {
    redis_conn: Arc<RwLock<Option<ConnectionManager>>>,
    key_prefix: String,
}

impl GasHistoryStore {
    /// Create a new gas history store
    pub async fn new(redis_host: &str, redis_port: u16, redis_password: &str) -> Self {
        let store = Self {
            redis_conn: Arc::new(RwLock::new(None)),
            key_prefix: "mev:gas:".to_string(),
        };

        // Initialize Redis connection asynchronously
        let redis_conn_clone = store.redis_conn.clone();
        let redis_url = if redis_password.is_empty() {
            format!("redis://{}:{}/", redis_host, redis_port)
        } else {
            format!("redis://:{}@{}:{}/", redis_password, redis_host, redis_port)
        };

        tokio::spawn(async move {
            match RedisClient::open(redis_url) {
                Ok(client) => {
                    match ConnectionManager::new(client).await {
                        Ok(conn) => {
                            debug!("Successfully connected to Redis for gas history");
                            *redis_conn_clone.write().await = Some(conn);
                        }
                        Err(e) => {
                            warn!("Failed to create Redis connection manager for gas history: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to connect to Redis for gas history: {}", e);
                }
            }
        });

        store
    }

    /// Get filtered gas value for a target address
    pub async fn get_filtered_gas(&self, target: &Address) -> Option<u64> {
        let conn_guard = self.redis_conn.read().await;
        if let Some(conn) = conn_guard.as_ref() {
            let mut conn = conn.clone();
            let key = format!("{}{:?}", self.key_prefix, target);
            match conn.get::<_, Option<String>>(&key).await {
                Ok(Some(value)) => {
                    match value.parse::<u64>() {
                        Ok(gas) => {
                            debug!(target = %target, filtered_gas = gas, "Retrieved gas history from Redis");
                            Some(gas)
                        }
                        Err(e) => {
                            warn!(target = %target, error = %e, "Failed to parse gas value from Redis");
                            None
                        }
                    }
                }
                Ok(None) => {
                    debug!(target = %target, "No gas history found in Redis");
                    None
                }
                Err(e) => {
                    warn!(target = %target, error = %e, "Failed to get gas history from Redis");
                    None
                }
            }
        } else {
            None
        }
    }

    /// Set filtered gas value for a target address with TTL of 24 hours
    pub async fn set_filtered_gas(&self, target: &Address, filtered_gas: u64) {
        let conn_guard = self.redis_conn.read().await;
        if let Some(conn) = conn_guard.as_ref() {
            let mut conn = conn.clone();
            let key = format!("{}{:?}", self.key_prefix, target);
            let value = filtered_gas.to_string();
            
            // Set with 24 hour TTL (86400 seconds)
            match conn.set_ex::<_, _, ()>(&key, value, 86400).await {
                Ok(_) => {
                    debug!(target = %target, filtered_gas = filtered_gas, "Stored gas history in Redis");
                }
                Err(e) => {
                    warn!(target = %target, error = %e, "Failed to store gas history in Redis");
                }
            }
        }
    }
}