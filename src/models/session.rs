//! AI session and workspace models

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// AI session status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiSessionStatus {
    #[default]
    Active,
    Paused,
    Completed,
    Blocked,
}

impl AiSessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiSessionStatus::Active => "active",
            AiSessionStatus::Paused => "paused",
            AiSessionStatus::Completed => "completed",
            AiSessionStatus::Blocked => "blocked",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(AiSessionStatus::Active),
            "paused" => Some(AiSessionStatus::Paused),
            "completed" => Some(AiSessionStatus::Completed),
            "blocked" => Some(AiSessionStatus::Blocked),
            _ => None,
        }
    }
}

/// Session note type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionNoteType {
    Decision,
    Blocker,
    Question,
    Progress,
    Finding,
}

impl SessionNoteType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionNoteType::Decision => "decision",
            SessionNoteType::Blocker => "blocker",
            SessionNoteType::Question => "question",
            SessionNoteType::Progress => "progress",
            SessionNoteType::Finding => "finding",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "decision" => Some(SessionNoteType::Decision),
            "blocker" => Some(SessionNoteType::Blocker),
            "question" => Some(SessionNoteType::Question),
            "progress" => Some(SessionNoteType::Progress),
            "finding" => Some(SessionNoteType::Finding),
            _ => None,
        }
    }
}

/// An AI working session
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct AiSession {
    pub id: String,
    pub project_id: String,

    pub task: String,
    /// 'active', 'paused', 'completed', 'blocked'
    pub status: String,

    // Local workspace mapping
    pub local_root: Option<String>,
    pub repository_id: Option<String>,

    // Session data
    pub summary: Option<String>,
    /// JSON array of next steps
    pub next_steps: Option<String>,

    // Tracking
    /// 'claude-code', 'cursor', etc.
    pub agent_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub ended_at: Option<String>,
}

impl AiSession {
    /// Get the typed session status
    pub fn get_status(&self) -> Option<AiSessionStatus> {
        AiSessionStatus::from_str(&self.status)
    }

    /// Parse next steps from JSON string
    pub fn next_steps_vec(&self) -> Vec<String> {
        self.next_steps
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Check if session is still active
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// Check if session has ended
    pub fn has_ended(&self) -> bool {
        self.ended_at.is_some()
    }
}

/// A note within an AI session
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct SessionNote {
    pub id: String,
    pub session_id: String,
    /// 'decision', 'blocker', 'question', 'progress', 'finding'
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub note_type: String,
    pub content: String,
    pub created_at: String,
}

impl SessionNote {
    /// Get the typed note type
    pub fn get_type(&self) -> Option<SessionNoteType> {
        SessionNoteType::from_str(&self.note_type)
    }
}

/// Workspace mapping for local path resolution
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct Workspace {
    pub id: String,
    pub project_id: String,
    pub token_id: String,
    pub local_root: String,
    pub repository_id: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

impl Workspace {
    /// Check if the workspace has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ref expires) = self.expires_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expires) {
                return dt < chrono::Utc::now();
            }
        }
        false
    }
}
