//! Job and job log models

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Job status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(JobStatus::Pending),
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            _ => None,
        }
    }
}

/// Job type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    IndexRepo,
    ReindexRepo,
    IndexHistory,
    SyncMetadata,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobType::IndexRepo => "index_repo",
            JobType::ReindexRepo => "reindex_repo",
            JobType::IndexHistory => "index_history",
            JobType::SyncMetadata => "sync_metadata",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "index_repo" => Some(JobType::IndexRepo),
            "reindex_repo" => Some(JobType::ReindexRepo),
            "index_history" => Some(JobType::IndexHistory),
            "sync_metadata" => Some(JobType::SyncMetadata),
            _ => None,
        }
    }
}

/// Log level for job logs
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

/// A background job
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct Job {
    pub id: String,
    /// 'index_repo', 'reindex_repo', 'index_history', 'sync_metadata'
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub job_type: String,
    /// 'pending', 'running', 'completed', 'failed'
    pub status: String,
    pub project_id: Option<String>,
    pub repository_id: Option<String>,

    // Progress
    pub total_items: Option<i32>,
    pub processed_items: i32,
    pub failed_items: i32,

    // Timing
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,

    // Results
    /// JSON with summary
    pub result: Option<String>,
    pub error: Option<String>,
}

impl Job {
    /// Get the typed job status
    pub fn get_status(&self) -> Option<JobStatus> {
        JobStatus::from_str(&self.status)
    }

    /// Get the typed job type
    pub fn get_type(&self) -> Option<JobType> {
        JobType::from_str(&self.job_type)
    }

    /// Check if the job is still running
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    /// Check if the job has completed (successfully or with failure)
    pub fn is_finished(&self) -> bool {
        self.status == "completed" || self.status == "failed"
    }

    /// Check if the job failed
    pub fn has_failed(&self) -> bool {
        self.status == "failed"
    }

    /// Get progress percentage (0-100)
    pub fn progress_percent(&self) -> Option<f64> {
        self.total_items.map(|total| {
            if total == 0 {
                100.0
            } else {
                (self.processed_items as f64 / total as f64) * 100.0
            }
        })
    }

    /// Parse result from JSON string
    pub fn result_json(&self) -> Option<serde_json::Value> {
        self.result
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }
}

/// A log entry for a job
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct JobLog {
    pub id: i64,
    pub job_id: String,
    /// 'info', 'warn', 'error'
    pub level: String,
    pub message: String,
    /// JSON metadata
    pub metadata: Option<String>,
    pub created_at: String,
}

impl JobLog {
    /// Get the typed log level
    pub fn get_level(&self) -> Option<LogLevel> {
        LogLevel::from_str(&self.level)
    }

    /// Parse metadata from JSON string
    pub fn metadata_json(&self) -> Option<serde_json::Value> {
        self.metadata
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }
}
