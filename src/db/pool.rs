//! SQLite connection pool configuration and utilities.
//!
//! This module provides additional pool management utilities
//! beyond the basic init_pool() in mod.rs.

use crate::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::str::FromStr;
use std::time::Duration;

/// Pool configuration options.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of connections to maintain.
    pub min_connections: u32,
    /// Timeout for acquiring a connection.
    pub acquire_timeout: Duration,
    /// Maximum idle time before a connection is closed.
    pub idle_timeout: Option<Duration>,
    /// Maximum lifetime of a connection.
    pub max_lifetime: Option<Duration>,
    /// SQLite busy timeout.
    pub busy_timeout: Duration,
    /// Cache size in KB (negative values).
    pub cache_size_kb: i64,
    /// Memory-mapped I/O size in bytes.
    pub mmap_size: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(600)),
            max_lifetime: Some(Duration::from_secs(1800)),
            busy_timeout: Duration::from_secs(30),
            cache_size_kb: 64000,
            mmap_size: 268_435_456, // 256MB
        }
    }
}

impl PoolConfig {
    /// Create a new pool configuration with sensible defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure for high-concurrency workloads.
    pub fn high_concurrency() -> Self {
        Self {
            max_connections: 20,
            min_connections: 5,
            acquire_timeout: Duration::from_secs(10),
            busy_timeout: Duration::from_secs(60),
            cache_size_kb: 128000,
            mmap_size: 536_870_912, // 512MB
            ..Default::default()
        }
    }

    /// Configure for low-memory environments.
    pub fn low_memory() -> Self {
        Self {
            max_connections: 5,
            min_connections: 1,
            cache_size_kb: 16000,
            mmap_size: 67_108_864, // 64MB
            ..Default::default()
        }
    }

    /// Configure for testing (in-memory, minimal connections).
    pub fn test() -> Self {
        Self {
            max_connections: 1,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: None,
            max_lifetime: None,
            busy_timeout: Duration::from_secs(5),
            cache_size_kb: 8000,
            mmap_size: 0,
        }
    }

    /// Build the connection options for SQLite.
    pub fn build_connect_options(&self, path: &str) -> Result<SqliteConnectOptions> {
        let options = SqliteConnectOptions::from_str(path)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(self.busy_timeout)
            .foreign_keys(true)
            .pragma("cache_size", format!("-{}", self.cache_size_kb))
            .pragma("mmap_size", self.mmap_size.to_string())
            .pragma("temp_store", "memory");

        Ok(options)
    }

    /// Build the pool options.
    pub fn build_pool_options(&self) -> SqlitePoolOptions {
        let mut opts = SqlitePoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(self.acquire_timeout);

        if let Some(idle) = self.idle_timeout {
            opts = opts.idle_timeout(idle);
        }

        if let Some(lifetime) = self.max_lifetime {
            opts = opts.max_lifetime(lifetime);
        }

        opts
    }
}

/// Create a pool with custom configuration.
pub async fn create_pool_with_config(path: &str, config: PoolConfig) -> Result<super::DbPool> {
    // Create parent directories if they don't exist
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let options = config.build_connect_options(path)?;
    let pool_opts = config.build_pool_options();

    let pool = pool_opts.connect_with(options).await?;

    Ok(pool)
}

/// Health check for the database connection.
pub async fn health_check(pool: &super::DbPool) -> Result<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// Get pool statistics.
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub size: u32,
    pub idle: u32,
    pub max_connections: u32,
}

pub fn get_pool_stats(pool: &super::DbPool) -> PoolStats {
    PoolStats {
        size: pool.size(),
        idle: pool.num_idle() as u32,
        max_connections: pool.options().get_max_connections(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_config_default() {
        let config = PoolConfig::default();
        let pool = create_pool_with_config(":memory:", config).await.unwrap();
        assert!(pool.size() > 0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let pool = create_pool_with_config(":memory:", PoolConfig::test()).await.unwrap();
        health_check(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let pool = create_pool_with_config(":memory:", PoolConfig::test()).await.unwrap();
        let stats = get_pool_stats(&pool);
        assert_eq!(stats.max_connections, 1);
    }
}
