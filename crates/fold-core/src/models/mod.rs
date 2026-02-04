//! Data models for Fold.
//!
//! Defines the core types used throughout the system including
//! memories, projects, users, and various DTOs.
//!
//! Models are now defined in the `fold-models` crate and re-exported here
//! for backwards compatibility.

// Job model stays local as it wasn't extracted
mod job;

// Re-export everything from fold-models
pub use fold_models::*;

// Re-export job types (kept local)
pub use job::*;
