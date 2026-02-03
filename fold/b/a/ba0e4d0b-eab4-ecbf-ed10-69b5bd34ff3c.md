---
id: ba0e4d0b-eab4-ecbf-ed10-69b5bd34ff3c
title: sessions.rs
author: system
file_path: src/db/sessions.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:05:34.352388Z
updated_at: 2026-02-03T08:05:34.352388Z
---

//! AI session and workspace database queries.
//!
//! Tracks AI agent working sessions and local workspace mappings.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// AI Session Types
// ============================================================================

/// AI session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
    Blocked,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => S