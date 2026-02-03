---
id: 2bfd040d-d5d5-fb45-bfcc-1f521ec0fedf
title: search.rs
author: system
file_path: src/api/search.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:05:08.449836700Z
updated_at: 2026-02-03T08:05:08.449836700Z
---

//! Search Routes
//!
//! Unified search and context retrieval endpoints.
//!
//! Routes:
//! - POST /projects/:project_id/search - Unified semantic search
//! - POST /projects/:project_id/context - Get context for a task

use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::MemorySource;
use crate::{db, AppState, Error, Result};

/// Build search routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/:project_id/search", post(search))
        .route("/:project_id/context", post(get_context))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Unified search request.
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    /// Query text for semantic search