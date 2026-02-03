//! Data models for Fold.
//!
//! Defines the core types used throughout the system including
//! memories, projects, users, and various DTOs.

mod job;
mod memory;
mod project;
mod provider;
mod repository;
mod session;
mod team;
mod user;

pub use memory::*;
pub use project::*;
pub use repository::*;
pub use user::*;

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
