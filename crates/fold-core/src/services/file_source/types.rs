//! Types for the FileSourceProvider abstraction.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Configuration for connecting to a file source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Owner/organization (for Git providers).
    pub owner: Option<String>,

    /// Source name (repository name, folder ID, etc.).
    pub name: String,

    /// Path prefix to limit indexing scope.
    pub path_prefix: Option<String>,

    /// Branch name (for Git providers).
    pub branch: Option<String>,

    /// Provider-specific configuration as JSON.
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl SourceConfig {
    /// Create a new source config for a Git repository.
    pub fn git(owner: &str, name: &str, branch: Option<&str>) -> Self {
        Self {
            owner: Some(owner.to_string()),
            name: name.to_string(),
            path_prefix: None,
            branch: branch.map(String::from),
            extra: serde_json::Value::Null,
        }
    }

    /// Create a new source config for a cloud folder.
    pub fn folder(folder_id: &str) -> Self {
        Self {
            owner: None,
            name: folder_id.to_string(),
            path_prefix: None,
            branch: None,
            extra: serde_json::Value::Null,
        }
    }
}

/// Information about a connected file source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Provider-specific source identifier.
    pub id: String,

    /// Display name for the source.
    pub name: String,

    /// Full name including owner if applicable (e.g., "owner/repo").
    pub full_name: String,

    /// URL to the source (if available).
    pub url: Option<String>,

    /// Default branch/version.
    pub default_version: Option<String>,

    /// Whether the source is private.
    pub is_private: bool,

    /// Owner/organization (for Git providers).
    pub owner: Option<String>,

    /// Provider-specific metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl SourceInfo {
    /// Get the owner, panics if not a Git source.
    pub fn owner_or_empty(&self) -> &str {
        self.owner.as_deref().unwrap_or("")
    }
}

/// File content retrieved from a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    /// File path relative to source root.
    pub path: String,

    /// File name.
    pub name: String,

    /// File content as string (if text).
    pub content: Option<String>,

    /// Raw bytes (if binary).
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,

    /// Content hash (SHA, MD5, etc.).
    pub hash: Option<String>,

    /// File size in bytes.
    pub size: i64,

    /// MIME type if known.
    pub mime_type: Option<String>,

    /// Last modified timestamp.
    pub modified_at: Option<DateTime<Utc>>,
}

/// Information about a file in a listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// File path relative to source root.
    pub path: String,

    /// File name.
    pub name: String,

    /// Whether this is a directory.
    pub is_directory: bool,

    /// File size in bytes (0 for directories).
    pub size: i64,

    /// Content hash if available.
    pub hash: Option<String>,

    /// Last modified timestamp.
    pub modified_at: Option<DateTime<Utc>>,
}

/// Configuration for change notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// Notification type.
    pub notification_type: NotificationType,

    /// Provider-specific notification ID (webhook ID, subscription ID, etc.).
    pub notification_id: String,

    /// Events registered for.
    pub events: Vec<String>,

    /// Polling interval in seconds (for polling providers).
    pub poll_interval_secs: Option<u64>,

    /// Expiration time (some webhooks expire).
    pub expires_at: Option<DateTime<Utc>>,
}

/// Type of notification mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    /// Real-time webhook notifications.
    Webhook,
    /// Periodic polling for changes.
    Polling,
    /// Push notifications (mobile/desktop).
    Push,
}

/// A change event from any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChangeEvent {
    /// A file was created.
    FileCreated {
        path: String,
        author: Option<String>,
        hash: Option<String>,
    },

    /// A file was modified.
    FileModified {
        path: String,
        author: Option<String>,
        hash: Option<String>,
        previous_hash: Option<String>,
    },

    /// A file was deleted.
    FileDeleted {
        path: String,
        author: Option<String>,
    },

    /// A file was moved/renamed.
    FileMoved {
        old_path: String,
        new_path: String,
        author: Option<String>,
    },

    /// A Git commit (contains multiple file changes).
    Commit {
        sha: String,
        message: String,
        author: String,
        author_email: Option<String>,
        timestamp: DateTime<Utc>,
        files: Vec<CommitFile>,
        stats: Option<CommitStats>,
    },

    /// A pull request / merge request event.
    PullRequest {
        number: u32,
        action: PullRequestAction,
        title: String,
        author: String,
        source_branch: Option<String>,
        target_branch: Option<String>,
        is_merged: bool,
    },

    /// A branch was created.
    BranchCreated {
        branch: String,
        base_sha: Option<String>,
    },

    /// A branch was deleted.
    BranchDeleted { branch: String },

    /// A tag was created.
    TagCreated { tag: String, sha: String },

    /// A sync/refresh completed (for polling providers).
    SyncCompleted {
        files_added: u32,
        files_modified: u32,
        files_deleted: u32,
    },
}

/// File change within a commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitFile {
    /// File path.
    pub path: String,

    /// Change status (added, modified, deleted, renamed).
    pub status: FileChangeStatus,

    /// Previous path (for renames).
    pub previous_path: Option<String>,

    /// Patch/diff content.
    pub patch: Option<String>,

    /// Lines added.
    pub additions: i32,

    /// Lines deleted.
    pub deletions: i32,
}

/// Status of a file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Changed,
}

impl FileChangeStatus {
    /// Parse from string (GitHub/GitLab format).
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "added" | "new" => Self::Added,
            "modified" | "changed" => Self::Modified,
            "deleted" | "removed" => Self::Deleted,
            "renamed" | "moved" => Self::Renamed,
            "copied" => Self::Copied,
            _ => Self::Changed,
        }
    }
}

/// Commit statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitStats {
    pub additions: i32,
    pub deletions: i32,
    pub total: i32,
}

/// Pull request action type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestAction {
    Opened,
    Closed,
    Merged,
    Reopened,
    Synchronized,
    Edited,
    ReviewRequested,
    Approved,
    ChangesRequested,
    Commented,
}

impl PullRequestAction {
    /// Parse from string (GitHub/GitLab format).
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "opened" | "open" => Self::Opened,
            "closed" | "close" => Self::Closed,
            "merged" | "merge" => Self::Merged,
            "reopened" | "reopen" => Self::Reopened,
            "synchronize" | "synchronized" | "update" => Self::Synchronized,
            "edited" | "edit" => Self::Edited,
            "review_requested" => Self::ReviewRequested,
            "approved" | "approve" => Self::Approved,
            "changes_requested" => Self::ChangesRequested,
            "commented" | "comment" => Self::Commented,
            _ => Self::Edited,
        }
    }
}

/// Result of change detection (for polling providers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeDetectionResult {
    /// Detected change events.
    pub events: Vec<ChangeEvent>,

    /// New cursor/token for next detection.
    pub next_cursor: Option<String>,

    /// Whether there are more changes to fetch.
    pub has_more: bool,
}

/// Error specific to file source operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceError {
    /// Error code.
    pub code: SourceErrorCode,

    /// Human-readable message.
    pub message: String,

    /// Whether the operation can be retried.
    pub retryable: bool,
}

/// File source error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceErrorCode {
    /// Authentication failed.
    Unauthorized,
    /// Resource not found.
    NotFound,
    /// Access denied.
    Forbidden,
    /// Rate limited.
    RateLimited,
    /// Provider API error.
    ProviderError,
    /// Invalid configuration.
    InvalidConfig,
    /// Network error.
    NetworkError,
    /// Unknown error.
    Unknown,
}
