//! Team status model

use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlx")]
use sqlx::FromRow;

/// Team member status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    #[default]
    Idle,
    Away,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Idle => "idle",
            Status::Away => "away",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Status::Active),
            "idle" => Some(Status::Idle),
            "away" => Some(Status::Away),
            _ => None,
        }
    }
}

/// Team member status record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(FromRow))]
#[serde(rename_all = "snake_case")]
pub struct TeamStatus {
    pub id: String,
    pub project_id: String,
    pub username: String,
    /// 'active', 'idle', or 'away'
    pub status: String,
    pub current_task: Option<String>,
    /// JSON array of file paths
    pub current_files: Option<String>,
    pub last_seen: String,
    pub session_start: Option<String>,
}

impl TeamStatus {
    /// Get the typed status
    pub fn get_status(&self) -> Option<Status> {
        Status::from_str(&self.status)
    }

    /// Parse current files from JSON string
    pub fn current_files_vec(&self) -> Vec<String> {
        self.current_files
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Check if the user is currently active
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}
