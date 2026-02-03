---
id: 74062d8b-8fe0-d9ca-ac2e-e56c9fa20bfe
title: registry.rs
author: system
file_path: src/services/file_source/registry.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:06:27.511047100Z
updated_at: 2026-02-03T08:06:27.511047100Z
---

//! Provider registry for dynamic provider lookup.
//!
//! The registry allows runtime lookup of file source providers by type,
//! enabling dynamic configuration of which providers are available.

use std::collections::HashMap;
use std::sync::Arc;

use super::{FileSourceProvider, GitHubFileSource, GoogleDriveFileSource, LocalFileSource};

/// Registry of available file source providers.
///
/// Allows dynamic lookup of providers by their type identifier.
pub struct ProviderRegistry {
    providers: HashMap<&'static str, Arc<dyn FileSourceProvider>>,
}

impl ProviderRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Create a registry with default providers.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register GitHub provider
        registry.register(Arc::new(GitHubFileSource::new()));

        // Register Go