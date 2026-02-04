//! Data models for Fold.
//!
//! Defines the core types used throughout the system including
//! memories, projects, users, and various DTOs.
//!
//! This crate can be used with or without sqlx support:
//! - Default: No database dependencies, pure data structures
//! - With `sqlx` feature: Adds `FromRow` derive for database mapping

mod chunk;
mod memory;
mod project;
mod provider;
mod repository;
mod session;
mod team;
mod user;

pub use chunk::*;
pub use memory::*;
pub use project::*;
pub use provider::*;
pub use repository::*;
pub use session::*;
pub use team::*;
pub use user::*;

// Type aliases for backwards compatibility
// The legacy types were renamed to avoid conflicts with db::links types
pub type LinkType = LegacyLinkType;
pub type MemoryLink = LegacyMemoryLink;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Generate a new UUID
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Current UTC timestamp
pub fn now() -> DateTime<Utc> {
    Utc::now()
}
