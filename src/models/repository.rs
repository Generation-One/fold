//! Repository and git-related models

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::GitProvider;

/// Pull request state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

impl PrState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrState::Open => "open",
            PrState::Closed => "closed",
            PrState::Merged => "merged",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(PrState::Open),
            "closed" => Some(PrState::Closed),
            "merged" => Some(PrState::Merged),
            _ => None,
        }
    }
}

/// A connected git repository
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct Repository {
    pub id: String,
    pub project_id: String,
    /// 'github', 'gitlab', or 'local'
    pub provider: String,
    pub owner: String,
    pub repo: String,
    /// Single branch to monitor
    pub branch: String,

    // Webhook
    pub webhook_id: Option<String>,
    pub webhook_secret: Option<String>,

    /// Encrypted access token
    pub access_token: String,

    /// Local filesystem path (for local provider or cloned repos)
    pub local_path: Option<String>,

    // Status
    pub last_indexed_at: Option<String>,
    pub last_commit_sha: Option<String>,

    pub created_at: String,
}

impl Repository {
    /// Get the typed git provider
    pub fn get_provider(&self) -> Option<GitProvider> {
        GitProvider::from_str(&self.provider)
    }

    /// Get the full repository identifier (owner/repo)
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// Get the repository URL based on provider
    pub fn url(&self) -> String {
        match self.provider.as_str() {
            "github" => format!("https://github.com/{}/{}", self.owner, self.repo),
            "gitlab" => format!("https://gitlab.com/{}/{}", self.owner, self.repo),
            "local" => self.local_path.clone().unwrap_or_else(|| format!("file://{}/{}", self.owner, self.repo)),
            _ => format!("https://unknown/{}/{}", self.owner, self.repo),
        }
    }
}

/// File change info for a commit
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileChange {
    pub path: String,
    pub status: String,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
}

/// A git commit record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct GitCommit {
    pub id: String,
    pub repository_id: String,
    pub sha: String,
    pub message: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    /// JSON array of FileChange
    pub files_changed: Option<String>,
    pub insertions: Option<i32>,
    pub deletions: Option<i32>,
    pub committed_at: String,
    pub indexed_at: String,
    /// Links to LLM-generated summary memory
    pub summary_memory_id: Option<String>,
}

impl GitCommit {
    /// Parse files changed from JSON string
    pub fn files_changed_vec(&self) -> Vec<FileChange> {
        self.files_changed
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Get a short SHA (first 7 characters)
    pub fn short_sha(&self) -> &str {
        if self.sha.len() >= 7 {
            &self.sha[..7]
        } else {
            &self.sha
        }
    }

    /// Get the first line of the commit message
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or(&self.message)
    }
}

/// A git pull request
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct GitPullRequest {
    pub id: String,
    pub repository_id: String,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
    /// 'open', 'closed', or 'merged'
    pub state: String,
    pub author: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub created_at: String,
    pub merged_at: Option<String>,
    pub indexed_at: String,
    /// Links to PR memory
    pub memory_id: Option<String>,
}

impl GitPullRequest {
    /// Get the typed PR state
    pub fn get_state(&self) -> Option<PrState> {
        PrState::from_str(&self.state)
    }

    /// Check if the PR is open
    pub fn is_open(&self) -> bool {
        self.state == "open"
    }

    /// Check if the PR is merged
    pub fn is_merged(&self) -> bool {
        self.state == "merged"
    }
}
