//! Fold - Holographic Memory System
//!
//! Library exports for testing and external use.

pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod middleware;
pub mod models;
pub mod services;
pub mod state;

pub use config::config;
pub use error::{Error, Result};
pub use state::AppState;
