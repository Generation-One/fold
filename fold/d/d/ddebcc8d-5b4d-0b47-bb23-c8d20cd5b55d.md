---
id: ddebcc8d-5b4d-0b47-bb23-c8d20cd5b55d
title: projects.rs
author: system
file_path: src/api/projects.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:05:02.879237200Z
updated_at: 2026-02-03T08:05:02.879237200Z
---

//! Projects Routes
//!
//! CRUD operations for projects in the Fold system.
//!
//! Routes:
//! - GET /projects - List all projects
//! - POST /projects - Create a new project
//! - GET /projects/:id - Get project details
//! - PUT /projects/:id - Update project
//! - DELETE /projects/:id - Delete project

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build project routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_projects).post(create_project))
        .route(
            "/:id",
            get(get_project).put(update_project).delete(delete_project),
        )
}

// ============================================================================
// Request/Response Types
// ===============================================