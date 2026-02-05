//! Event types for Server-Sent Events (SSE).
//!
//! These events are broadcast to connected clients for real-time updates
//! on job progress, indexing status, and system health.

use serde::{Deserialize, Serialize};

/// All possible events that can be broadcast via SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum FoldEvent {
    /// Job has been claimed and started processing
    JobStarted(JobEvent),
    /// Job processing progress update
    JobProgress(JobProgressEvent),
    /// Job completed successfully
    JobCompleted(JobEvent),
    /// Job failed with an error
    JobFailed(JobFailedEvent),
    /// Job paused waiting for resources
    JobPaused(JobEvent),
    /// Job resumed after resources became available
    JobResumed(JobEvent),

    /// Indexing started for a project
    IndexingStarted(IndexingEvent),
    /// Indexing progress update
    IndexingProgress(IndexingProgressEvent),
    /// Indexing completed for a project
    IndexingCompleted(IndexingEvent),

    /// LLM/embedding provider became available
    ProviderAvailable(ProviderEvent),
    /// LLM/embedding provider became unavailable
    ProviderUnavailable(ProviderEvent),

    /// System health status changed
    HealthStatusChanged(HealthEvent),

    /// Heartbeat to keep connection alive
    Heartbeat(HeartbeatEvent),

    /// Job log entry (admin-only)
    JobLog(JobLogEvent),
}

impl FoldEvent {
    /// Get the SSE event type name for this event.
    pub fn event_type(&self) -> &'static str {
        match self {
            FoldEvent::JobStarted(_) => "job:started",
            FoldEvent::JobProgress(_) => "job:progress",
            FoldEvent::JobCompleted(_) => "job:completed",
            FoldEvent::JobFailed(_) => "job:failed",
            FoldEvent::JobPaused(_) => "job:paused",
            FoldEvent::JobResumed(_) => "job:resumed",
            FoldEvent::IndexingStarted(_) => "indexing:started",
            FoldEvent::IndexingProgress(_) => "indexing:progress",
            FoldEvent::IndexingCompleted(_) => "indexing:completed",
            FoldEvent::ProviderAvailable(_) => "provider:available",
            FoldEvent::ProviderUnavailable(_) => "provider:unavailable",
            FoldEvent::HealthStatusChanged(_) => "health:changed",
            FoldEvent::Heartbeat(_) => "heartbeat",
            FoldEvent::JobLog(_) => "job:log",
        }
    }

    /// Get the project ID associated with this event, if any.
    pub fn project_id(&self) -> Option<&str> {
        match self {
            FoldEvent::JobStarted(e) => e.project_id.as_deref(),
            FoldEvent::JobProgress(e) => e.project_id.as_deref(),
            FoldEvent::JobCompleted(e) => e.project_id.as_deref(),
            FoldEvent::JobFailed(e) => e.project_id.as_deref(),
            FoldEvent::JobPaused(e) => e.project_id.as_deref(),
            FoldEvent::JobResumed(e) => e.project_id.as_deref(),
            FoldEvent::IndexingStarted(e) => Some(&e.project_id),
            FoldEvent::IndexingProgress(e) => Some(&e.project_id),
            FoldEvent::IndexingCompleted(e) => Some(&e.project_id),
            FoldEvent::JobLog(e) => e.project_id.as_deref(),
            // Provider and health events are global
            FoldEvent::ProviderAvailable(_) => None,
            FoldEvent::ProviderUnavailable(_) => None,
            FoldEvent::HealthStatusChanged(_) => None,
            FoldEvent::Heartbeat(_) => None,
        }
    }

    /// Check if this event is global (should be sent to all users).
    pub fn is_global(&self) -> bool {
        matches!(
            self,
            FoldEvent::ProviderAvailable(_)
                | FoldEvent::ProviderUnavailable(_)
                | FoldEvent::HealthStatusChanged(_)
                | FoldEvent::Heartbeat(_)
        )
    }

    /// Check if this event requires admin privileges.
    pub fn is_admin_only(&self) -> bool {
        matches!(self, FoldEvent::JobLog(_))
    }
}

/// Basic job event with minimal information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEvent {
    pub job_id: String,
    pub job_type: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub timestamp: String,
}

/// Job progress event with completion metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgressEvent {
    pub job_id: String,
    pub job_type: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub processed: i32,
    pub failed: i32,
    pub total: Option<i32>,
    pub percent: Option<f64>,
    pub timestamp: String,
}

/// Job failure event with error details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFailedEvent {
    pub job_id: String,
    pub job_type: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub error: String,
    pub timestamp: String,
}

/// Indexing event for project-level updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingEvent {
    pub project_id: String,
    pub project_name: String,
    pub timestamp: String,
}

/// Indexing progress event with file counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingProgressEvent {
    pub project_id: String,
    pub project_name: String,
    pub files_indexed: i32,
    pub files_total: Option<i32>,
    pub current_file: Option<String>,
    pub timestamp: String,
}

/// Provider availability event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEvent {
    /// Provider type: "llm" or "embedding"
    pub provider_type: String,
    pub provider_name: String,
    pub available: bool,
    pub timestamp: String,
}

/// System health status event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEvent {
    /// Status: "healthy", "degraded", or "unhealthy"
    pub status: String,
    pub component: Option<String>,
    pub message: Option<String>,
    pub timestamp: String,
}

/// Heartbeat event to keep connection alive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatEvent {
    pub timestamp: String,
}

impl HeartbeatEvent {
    pub fn now() -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Job log event for real-time log streaming (admin-only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLogEvent {
    pub job_id: String,
    pub job_type: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    /// Log level: "debug", "info", "warn", "error"
    pub level: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: String,
}
