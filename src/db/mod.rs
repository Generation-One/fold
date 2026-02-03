//! Database layer for Fold.
//!
//! Provides SQLite connection pooling and query modules
//! for all domain entities.

mod attachments;
mod chunks;
mod jobs;
mod links;
mod memories;
mod pool;
mod projects;
mod providers;
mod repositories;
mod sessions;
mod users;

// Re-export Qdrant client (actual implementation in services)
pub mod qdrant;

// Re-export all query modules
pub use attachments::*;
pub use chunks::*;
pub use jobs::*;
pub use links::*;
pub use memories::*;
pub use projects::*;
pub use providers::*;
pub use repositories::*;
pub use sessions::*;
pub use users::*;

use crate::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::path::Path;
use std::str::FromStr;
use tracing::info;

/// Type alias for the SQLite connection pool.
pub type DbPool = sqlx::SqlitePool;

/// Initialize the database connection pool.
///
/// Creates parent directories if needed and configures SQLite with
/// optimal settings for concurrent access.
pub async fn init_pool(path: &str) -> Result<DbPool> {
    // Create parent directories if they don't exist
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    // Configure connection options with pragmas for performance
    let options = SqliteConnectOptions::from_str(path)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(30))
        // Enable foreign keys
        .foreign_keys(true)
        // Increase cache size (negative = KB)
        .pragma("cache_size", "-64000")
        // Memory-mapped I/O (256MB)
        .pragma("mmap_size", "268435456")
        // Temp store in memory
        .pragma("temp_store", "memory");

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(options)
        .await?;

    info!("Database pool initialized: {}", path);

    Ok(pool)
}

/// Initialize the database schema.
///
/// Applies the complete schema from schema.sql. Uses IF NOT EXISTS
/// clauses so it's safe to run multiple times.
pub async fn initialize_schema(pool: &DbPool) -> Result<()> {
    let schema = include_str!("../../schema.sql");

    info!("Initializing database schema");

    // Execute schema SQL (contains multiple statements)
    // Split by semicolons and execute each statement
    for statement in schema.split(';') {
        // Strip comment lines, keeping only actual SQL
        let clean_stmt: String = statement
            .lines()
            .filter(|line| !line.trim().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");
        let clean_stmt = clean_stmt.trim();
        if clean_stmt.is_empty() {
            continue;
        }
        sqlx::query(clean_stmt).execute(pool).await?;
    }

    info!("Database schema initialized successfully");

    Ok(())
}

/// Alias for initialize_schema for backward compatibility.
/// Deprecated: Use initialize_schema instead.
pub async fn migrate(pool: &DbPool) -> Result<()> {
    initialize_schema(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_pool_in_memory() {
        let pool = init_pool(":memory:").await.unwrap();
        assert!(pool.size() > 0);
    }

    #[tokio::test]
    async fn test_schema_initialization() {
        let pool = init_pool(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        // Verify core tables exist
        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let table_names: Vec<&str> = tables.iter().map(|(n,)| n.as_str()).collect();

        // Check core required tables
        assert!(table_names.contains(&"projects"), "projects table missing");
        assert!(table_names.contains(&"repositories"), "repositories table missing");
        assert!(table_names.contains(&"memories"), "memories table missing");
        assert!(table_names.contains(&"memory_links"), "memory_links table missing");
        assert!(table_names.contains(&"jobs"), "jobs table missing");
        assert!(table_names.contains(&"users"), "users table missing");
        assert!(table_names.contains(&"sessions"), "sessions table missing");
        assert!(table_names.contains(&"api_tokens"), "api_tokens table missing");

        // Verify we have at least the core tables
        assert!(table_names.len() >= 9, "Expected at least 9 tables, got {}: {:?}", table_names.len(), table_names);
    }
}
