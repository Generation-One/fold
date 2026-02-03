---
id: aa8f57cd-bf17-06ec-cedd-66858aed765b
title: mod.rs
author: system
file_path: src/db/mod.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:05:22.969340700Z
updated_at: 2026-02-03T08:05:22.969340700Z
---

//! Database layer for Fold.
//!
//! Provides SQLite connection pooling and query modules
//! for all domain entities.

mod attachments;
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
/// optimal settings 