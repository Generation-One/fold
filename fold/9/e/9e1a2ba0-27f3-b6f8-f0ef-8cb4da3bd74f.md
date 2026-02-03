---
id: 9e1a2ba0-27f3-b6f8-f0ef-8cb4da3bd74f
title: config.rs
author: system
file_path: src/config.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:05:13.608537900Z
updated_at: 2026-02-03T08:05:13.608537900Z
---

//! Configuration management for Fold.
//!
//! Loads configuration from environment variables with support for:
//! - Multiple OIDC auth providers via AUTH_PROVIDER_{NAME}_{FIELD} pattern
//! - Multiple LLM providers with fallback priority
//! - Database and vector store connections

use std::collections::HashMap;
use std::env;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Global configuration instance
static CONFIG: OnceLock<Config> = OnceLock::new();

/// Get the global configuration
pub fn config() -> &'static Config {
    CONFIG.get_or_init(Config::from_env)
}

/// Initialize configuration (call once at startup)
pub fn init() -> &'static Config {
    config()
}

#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub qdrant: QdrantConfig,
    pub embedding: EmbeddingConfig,
    pub auth: AuthConfig,
    pub llm: LlmConfig,
    pub session: SessionConfig,
    pub stor