//! Database layer for Fold.
//!
//! Provides SQLite connection pooling, migrations, and query modules
//! for all domain entities.

mod attachments;
mod jobs;
mod links;
mod memories;
mod pool;
mod projects;
mod repositories;
mod sessions;
mod users;

// Re-export Qdrant client (actual implementation in services)
pub mod qdrant;

// Re-export all query modules
pub use attachments::*;
pub use jobs::*;
pub use links::*;
pub use memories::*;
pub use pool::*;
pub use projects::*;
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

/// Run all database migrations in order.
///
/// Migrations are embedded in the binary and run sequentially.
/// Each migration is tracked in a `_migrations` table.
pub async fn migrate(pool: &DbPool) -> Result<()> {
    // Create migrations tracking table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Define migrations in order
    let migrations: &[(&str, &str)] = &[
        ("001_initial", include_str!("../../migrations/001_initial.sql")),
        ("002_repositories", include_str!("../../migrations/002_repositories.sql")),
        ("003_jobs", include_str!("../../migrations/003_jobs.sql")),
        ("004_schema_fixes", include_str!("../../migrations/004_schema_fixes.sql")),
    ];

    for (name, sql) in migrations {
        // Check if migration already applied
        let applied: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM _migrations WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        if applied.is_some() {
            continue;
        }

        info!("Running migration: {}", name);

        // Execute migration SQL (may contain multiple statements)
        // Split by semicolons and execute each statement
        for statement in sql.split(';') {
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
            info!("Executing: {}", &clean_stmt[..clean_stmt.len().min(80)]);
            sqlx::query(clean_stmt).execute(pool).await?;
        }

        // Record migration as applied
        sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
            .bind(name)
            .execute(pool)
            .await?;

        info!("Migration completed: {}", name);
    }

    info!("All migrations applied successfully");

    Ok(())
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
    async fn test_migrate() {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        // Verify migrations table exists and has entries
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(count.0 >= 3);
    }
}
