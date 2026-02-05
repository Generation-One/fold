//! Background job and job log database queries.
//!
//! Persistent SQLite-backed job queue with:
//! - Atomic job claiming (prevents duplicate processing)
//! - Priority-based scheduling
//! - Automatic retry with exponential backoff
//! - Stale job recovery
//! - Execution history tracking

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

/// Default lock timeout in seconds (5 minutes)
const LOCK_TIMEOUT_SECS: i64 = 300;

/// Base retry delay in seconds
const BASE_RETRY_DELAY_SECS: i64 = 60;

/// Maximum retry delay in seconds (1 hour)
const MAX_RETRY_DELAY_SECS: i64 = 3600;

// ============================================================================
// Job Types
// ============================================================================

/// Job type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    IndexRepo,
    ReindexRepo,
    IndexHistory,
    SyncMetadata,
    ProcessWebhook,
    GenerateSummary,
    Custom,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IndexRepo => "index_repo",
            Self::ReindexRepo => "reindex_repo",
            Self::IndexHistory => "index_history",
            Self::SyncMetadata => "sync_metadata",
            Self::ProcessWebhook => "process_webhook",
            Self::GenerateSummary => "generate_summary",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "index_repo" => Some(Self::IndexRepo),
            "reindex_repo" => Some(Self::ReindexRepo),
            "index_history" => Some(Self::IndexHistory),
            "sync_metadata" => Some(Self::SyncMetadata),
            "process_webhook" => Some(Self::ProcessWebhook),
            "generate_summary" => Some(Self::GenerateSummary),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Job status enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retry,
    Cancelled,
    /// Job is paused waiting for external resources (e.g., LLM/embedding providers)
    Paused,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Retry => "retry",
            Self::Cancelled => "cancelled",
            Self::Paused => "paused",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "retry" => Some(Self::Retry),
            "cancelled" => Some(Self::Cancelled),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }
}

/// Job priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobPriority {
    Low = 0,
    Normal = 5,
    High = 10,
    Critical = 20,
}

impl JobPriority {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }
}

impl Default for JobPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Job record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    #[sqlx(rename = "type")]
    pub job_type: String,
    pub status: String,
    pub project_id: Option<String>,
    pub total_items: Option<i32>,
    pub processed_items: i32,
    pub failed_items: i32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result: Option<String>, // JSON
    pub error: Option<String>,
    // New fields for enhanced queue
    pub payload: Option<String>,   // JSON payload for job-specific data
    pub priority: Option<i32>,     // Higher = more urgent
    pub max_retries: Option<i32>,  // Maximum retry attempts
    pub retry_count: Option<i32>,  // Current retry count
    pub locked_at: Option<String>, // When job was claimed
    pub locked_by: Option<String>, // Worker ID that claimed it
    pub scheduled_at: Option<String>, // For delayed jobs
    pub last_error: Option<String>, // Last error message for retries
}

impl Job {
    /// Get job type as enum.
    pub fn job_type_enum(&self) -> Option<JobType> {
        JobType::from_str(&self.job_type)
    }

    /// Get status as enum.
    pub fn status_enum(&self) -> Option<JobStatus> {
        JobStatus::from_str(&self.status)
    }

    /// Check if job is finished (completed or failed).
    pub fn is_finished(&self) -> bool {
        self.status == "completed" || self.status == "failed"
    }

    /// Check if job is running.
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    /// Get progress as percentage (0-100).
    pub fn progress_percent(&self) -> Option<f64> {
        self.total_items.map(|total| {
            if total == 0 {
                100.0
            } else {
                (self.processed_items as f64 / total as f64) * 100.0
            }
        })
    }

    /// Parse result JSON.
    pub fn result_json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        self.result
            .as_ref()
            .and_then(|r| serde_json::from_str(r).ok())
    }
}

/// Input for creating a new job.
#[derive(Debug, Clone)]
pub struct CreateJob {
    pub id: String,
    pub job_type: JobType,
    pub project_id: Option<String>,
    pub total_items: Option<i32>,
    pub payload: Option<serde_json::Value>,
    pub priority: Option<JobPriority>,
    pub max_retries: Option<i32>,
    pub scheduled_at: Option<String>,
}

impl CreateJob {
    /// Create a simple job with just type and ID.
    pub fn new(id: String, job_type: JobType) -> Self {
        Self {
            id,
            job_type,
            project_id: None,
            total_items: None,
            payload: None,
            priority: None,
            max_retries: None,
            scheduled_at: None,
        }
    }

    /// Set the project ID.
    pub fn with_project(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    /// Set the payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: JobPriority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max_retries: i32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Schedule for later execution.
    pub fn scheduled_at(mut self, at: impl Into<String>) -> Self {
        self.scheduled_at = Some(at.into());
        self
    }

    /// Set total items for progress tracking.
    pub fn with_total_items(mut self, total: i32) -> Self {
        self.total_items = Some(total);
        self
    }
}

/// Job result summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub processed: i32,
    pub failed: i32,
    pub duration_ms: i64,
    pub details: Option<serde_json::Value>,
}

// ============================================================================
// Job Log Types
// ============================================================================

/// Log level enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "warn" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

/// Job log record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct JobLog {
    pub id: i64,
    pub job_id: String,
    pub level: String,
    pub message: String,
    pub metadata: Option<String>, // JSON
    pub created_at: String,
}

impl JobLog {
    /// Get level as enum.
    pub fn level_enum(&self) -> LogLevel {
        LogLevel::from_str(&self.level)
    }

    /// Parse metadata JSON.
    pub fn metadata_json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        self.metadata
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok())
    }
}

/// Input for creating a job log entry.
#[derive(Debug, Clone)]
pub struct CreateJobLog {
    pub job_id: String,
    pub level: LogLevel,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

// ============================================================================
// Webhook Delivery Types
// ============================================================================

/// Webhook delivery record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    #[sqlx(rename = "type")]
    pub delivery_type: String,
    pub target_url: String,
    pub payload: String,
    pub status: String,
    pub attempts: i32,
    pub last_attempt_at: Option<String>,
    pub next_attempt_at: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
}

/// Input for creating a webhook delivery.
#[derive(Debug, Clone)]
pub struct CreateWebhookDelivery {
    pub id: String,
    pub delivery_type: String,
    pub target_url: String,
    pub payload: serde_json::Value,
}

/// Job execution record for tracking attempts.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct JobExecution {
    pub id: i64,
    pub job_id: String,
    pub attempt: i32,
    pub worker_id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
}

// ============================================================================
// Job Queries
// ============================================================================

/// Create a new job.
pub async fn create_job(pool: &DbPool, input: CreateJob) -> Result<Job> {
    let payload_json = input
        .payload
        .map(|p| serde_json::to_string(&p).unwrap_or_default());
    let priority = input
        .priority
        .map(|p| p.as_i32())
        .unwrap_or(JobPriority::Normal.as_i32());

    sqlx::query_as::<_, Job>(
        r#"
        INSERT INTO jobs (id, type, project_id, total_items, payload, priority, max_retries, scheduled_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(input.job_type.as_str())
    .bind(&input.project_id)
    .bind(input.total_items)
    .bind(&payload_json)
    .bind(priority)
    .bind(input.max_retries.unwrap_or(3))
    .bind(&input.scheduled_at)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Enqueue a job with builder pattern.
pub async fn enqueue_job(
    pool: &DbPool,
    job_type: JobType,
    payload: Option<serde_json::Value>,
) -> Result<Job> {
    let id = nanoid::nanoid!();
    create_job(
        pool,
        CreateJob::new(id, job_type).with_payload(payload.unwrap_or(serde_json::json!({}))),
    )
    .await
}

/// Get a job by ID.
pub async fn get_job(pool: &DbPool, id: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Get a job by ID (optional).
pub async fn get_job_optional(pool: &DbPool, id: &str) -> Result<Option<Job>> {
    sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Start a job (set to running) - simple version without locking.
pub async fn start_job(pool: &DbPool, id: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'running',
            started_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Atomically claim a job for processing.
///
/// Uses SQLite's atomic UPDATE to claim a pending/retry job.
/// Returns None if no jobs available or if claim failed due to race.
pub async fn claim_job(pool: &DbPool, worker_id: &str) -> Result<Option<Job>> {
    // Atomically claim the next available job
    // Jobs are selected by: status (pending/retry), scheduled time, priority, created_at
    let job = sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'running',
            started_at = datetime('now'),
            locked_at = datetime('now'),
            locked_by = ?
        WHERE id = (
            SELECT id FROM jobs
            WHERE status IN ('pending', 'retry')
            AND (scheduled_at IS NULL OR datetime(scheduled_at) <= datetime('now'))
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
        )
        AND status IN ('pending', 'retry')
        RETURNING *
        "#,
    )
    .bind(worker_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?;

    Ok(job)
}

/// Atomically claim a specific job by ID.
pub async fn claim_job_by_id(pool: &DbPool, job_id: &str, worker_id: &str) -> Result<Option<Job>> {
    let job = sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'running',
            started_at = datetime('now'),
            locked_at = datetime('now'),
            locked_by = ?
        WHERE id = ?
        AND status IN ('pending', 'retry')
        RETURNING *
        "#,
    )
    .bind(worker_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?;

    Ok(job)
}

/// Release a job lock without completing it (e.g., on worker shutdown).
pub async fn release_job(pool: &DbPool, job_id: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'pending',
            locked_at = NULL,
            locked_by = NULL
        WHERE id = ?
        AND status = 'running'
        RETURNING *
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found or not running: {}", job_id)))
}

/// Schedule a job for retry with exponential backoff.
pub async fn retry_job(pool: &DbPool, job_id: &str, error: &str) -> Result<Option<Job>> {
    // First, get the job to check retry count
    let job = get_job(pool, job_id).await?;

    let retry_count = job.retry_count.unwrap_or(0);
    let max_retries = job.max_retries.unwrap_or(3);

    if retry_count >= max_retries {
        // Max retries exceeded, mark as failed
        fail_job(pool, job_id, error).await?;
        return Ok(None);
    }

    // Calculate exponential backoff delay
    let delay_secs =
        (BASE_RETRY_DELAY_SECS * 2_i64.pow(retry_count as u32)).min(MAX_RETRY_DELAY_SECS);

    let job = sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'retry',
            retry_count = COALESCE(retry_count, 0) + 1,
            last_error = ?,
            scheduled_at = datetime('now', '+' || ? || ' seconds'),
            locked_at = NULL,
            locked_by = NULL
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(error)
    .bind(delay_secs)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?;

    Ok(job)
}

/// Recover stale jobs that have been locked too long.
/// Returns number of jobs recovered.
pub async fn recover_stale_jobs(pool: &DbPool, timeout_secs: Option<i64>) -> Result<u64> {
    let timeout = timeout_secs.unwrap_or(LOCK_TIMEOUT_SECS);

    let result = sqlx::query(
        r#"
        UPDATE jobs SET
            status = 'retry',
            locked_at = NULL,
            locked_by = NULL,
            last_error = 'Worker timeout - job recovered'
        WHERE status = 'running'
        AND locked_at IS NOT NULL
        AND datetime(locked_at, '+' || ? || ' seconds') < datetime('now')
        "#,
    )
    .bind(timeout)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Refresh the lock on a job (heartbeat).
pub async fn heartbeat_job(pool: &DbPool, job_id: &str, worker_id: &str) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE jobs SET locked_at = datetime('now')
        WHERE id = ? AND locked_by = ? AND status = 'running'
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Update job progress.
pub async fn update_job_progress(
    pool: &DbPool,
    id: &str,
    processed: i32,
    failed: i32,
) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            processed_items = ?,
            failed_items = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(processed)
    .bind(failed)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Increment job progress.
pub async fn increment_job_progress(
    pool: &DbPool,
    id: &str,
    processed_delta: i32,
    failed_delta: i32,
) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            processed_items = processed_items + ?,
            failed_items = failed_items + ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(processed_delta)
    .bind(failed_delta)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Update job metadata/result field mid-execution.
pub async fn update_job_metadata(
    pool: &DbPool,
    id: &str,
    metadata: &serde_json::Value,
) -> Result<Job> {
    let metadata_json = serde_json::to_string(metadata).unwrap_or_default();

    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            result = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(&metadata_json)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Complete a job successfully.
pub async fn complete_job(pool: &DbPool, id: &str, result: Option<&JobResult>) -> Result<Job> {
    let result_json = result.map(|r| serde_json::to_string(r).unwrap_or_default());

    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'completed',
            completed_at = datetime('now'),
            locked_at = NULL,
            locked_by = NULL,
            result = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(&result_json)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Fail a job permanently (no more retries).
pub async fn fail_job(pool: &DbPool, id: &str, error: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'failed',
            completed_at = datetime('now'),
            locked_at = NULL,
            locked_by = NULL,
            error = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(error)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found: {}", id)))
}

/// Cancel a job.
pub async fn cancel_job(pool: &DbPool, id: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'cancelled',
            completed_at = datetime('now'),
            locked_at = NULL,
            locked_by = NULL
        WHERE id = ?
        AND status IN ('pending', 'retry')
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found or not cancellable: {}", id)))
}

/// Cancel all pending/running/retry jobs for a project.
///
/// Returns the number of jobs cancelled.
/// Running jobs are marked as cancelled but may still be executing until they check status.
pub async fn cancel_project_jobs(pool: &DbPool, project_id: &str) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE jobs SET
            status = 'cancelled',
            completed_at = datetime('now'),
            locked_at = NULL,
            locked_by = NULL,
            last_error = 'Project deleted'
        WHERE project_id = ?
        AND status IN ('pending', 'running', 'retry', 'paused')
        "#,
    )
    .bind(project_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Delete all jobs for a project (including completed/failed).
///
/// Returns the number of jobs deleted.
pub async fn delete_project_jobs(pool: &DbPool, project_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM jobs WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Pause a job (waiting for external resources like LLM/embedding providers).
pub async fn pause_job(pool: &DbPool, id: &str, reason: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'paused',
            last_error = ?,
            locked_at = NULL,
            locked_by = NULL
        WHERE id = ?
        AND status = 'running'
        RETURNING *
        "#,
    )
    .bind(reason)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found or not running: {}", id)))
}

/// Resume all paused jobs (when providers become available).
/// Returns the number of jobs resumed.
pub async fn resume_paused_jobs(pool: &DbPool) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE jobs SET
            status = 'pending',
            last_error = NULL
        WHERE status = 'paused'
        "#,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Resume a specific paused job by ID.
pub async fn resume_job(pool: &DbPool, id: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'pending',
            last_error = NULL
        WHERE id = ?
        AND status = 'paused'
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Job not found or not paused: {}", id)))
}

/// List all paused jobs.
pub async fn list_paused_jobs(pool: &DbPool) -> Result<Vec<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE status = 'paused'
        ORDER BY priority DESC, created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Count paused jobs.
pub async fn count_paused_jobs(pool: &DbPool) -> Result<i64> {
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = 'paused'")
        .fetch_one(pool)
        .await
        .map_err(Error::Database)?;

    Ok(result.0)
}

// ============================================================================
// Job Execution Tracking
// ============================================================================

/// Record a job execution attempt.
pub async fn create_job_execution(
    pool: &DbPool,
    job_id: &str,
    attempt: i32,
    worker_id: &str,
) -> Result<JobExecution> {
    sqlx::query_as::<_, JobExecution>(
        r#"
        INSERT INTO job_executions (job_id, attempt, worker_id, status)
        VALUES (?, ?, ?, 'running')
        RETURNING *
        "#,
    )
    .bind(job_id)
    .bind(attempt)
    .bind(worker_id)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Complete a job execution record.
pub async fn complete_job_execution(
    pool: &DbPool,
    execution_id: i64,
    status: &str,
    error: Option<&str>,
    duration_ms: i64,
) -> Result<JobExecution> {
    sqlx::query_as::<_, JobExecution>(
        r#"
        UPDATE job_executions SET
            completed_at = datetime('now'),
            status = ?,
            error = ?,
            duration_ms = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(status)
    .bind(error)
    .bind(duration_ms)
    .bind(execution_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Execution not found: {}", execution_id)))
}

/// List executions for a job.
pub async fn list_job_executions(pool: &DbPool, job_id: &str) -> Result<Vec<JobExecution>> {
    sqlx::query_as::<_, JobExecution>(
        r#"
        SELECT * FROM job_executions
        WHERE job_id = ?
        ORDER BY attempt ASC
        "#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Delete a job.
pub async fn delete_job(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Job not found: {}", id)));
    }

    Ok(())
}

/// List jobs with optional filters.
pub async fn list_jobs(
    pool: &DbPool,
    status: Option<JobStatus>,
    job_type: Option<JobType>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Job>> {
    let mut conditions = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(s) = status {
        conditions.push("status = ?");
        bindings.push(s.as_str().to_string());
    }

    if let Some(t) = job_type {
        conditions.push("type = ?");
        bindings.push(t.as_str().to_string());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let query = format!(
        r#"
        SELECT * FROM jobs
        {}
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
        where_clause
    );

    let mut q = sqlx::query_as::<_, Job>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(limit).bind(offset);

    q.fetch_all(pool).await.map_err(Error::Database)
}

/// List jobs by status.
/// Uses idx_jobs_status index.
pub async fn list_jobs_by_status(pool: &DbPool, status: JobStatus) -> Result<Vec<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE status = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(status.as_str())
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List pending/retry jobs ready for processing.
pub async fn list_pending_jobs(pool: &DbPool, limit: i64) -> Result<Vec<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE status IN ('pending', 'retry')
        AND (scheduled_at IS NULL OR datetime(scheduled_at) <= datetime('now'))
        ORDER BY priority DESC, created_at ASC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Get queue depth (number of pending/retry jobs).
pub async fn get_queue_depth(pool: &DbPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM jobs
        WHERE status IN ('pending', 'retry')
        AND (scheduled_at IS NULL OR datetime(scheduled_at) <= datetime('now'))
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Get queue statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: i64,
    pub running: i64,
    pub retry: i64,
    pub completed_24h: i64,
    pub failed_24h: i64,
}

pub async fn get_queue_stats(pool: &DbPool) -> Result<QueueStats> {
    let (pending,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = 'pending'")
        .fetch_one(pool)
        .await?;

    let (running,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = 'running'")
        .fetch_one(pool)
        .await?;

    let (retry,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = 'retry'")
        .fetch_one(pool)
        .await?;

    let (completed_24h,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM jobs WHERE status = 'completed' AND completed_at >= datetime('now', '-1 day')",
    )
    .fetch_one(pool)
    .await?;

    let (failed_24h,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND completed_at >= datetime('now', '-1 day')",
    )
    .fetch_one(pool)
    .await?;

    Ok(QueueStats {
        pending,
        running,
        retry,
        completed_24h,
        failed_24h,
    })
}

/// List jobs for a project.
/// Uses idx_jobs_project index.
pub async fn list_project_jobs(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE project_id = ?
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Get running job for project (to prevent duplicate indexing).
pub async fn get_running_project_job(
    pool: &DbPool,
    project_id: &str,
    job_type: JobType,
) -> Result<Option<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE project_id = ? AND type = ? AND status = 'running'
        "#,
    )
    .bind(project_id)
    .bind(job_type.as_str())
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Count jobs by status.
pub async fn count_jobs_by_status(pool: &DbPool, status: JobStatus) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = ?")
        .bind(status.as_str())
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Delete old completed/failed jobs.
pub async fn cleanup_old_jobs(pool: &DbPool, days: i64) -> Result<u64> {
    let result = sqlx::query(
        r#"
        DELETE FROM jobs
        WHERE status IN ('completed', 'failed')
        AND datetime(completed_at, '+' || ? || ' days') < datetime('now')
        "#,
    )
    .bind(days)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Job Log Queries
// ============================================================================

/// Create a job log entry.
pub async fn create_job_log(pool: &DbPool, input: CreateJobLog) -> Result<JobLog> {
    let metadata_json = input
        .metadata
        .map(|m| serde_json::to_string(&m).unwrap_or_default());

    sqlx::query_as::<_, JobLog>(
        r#"
        INSERT INTO job_logs (job_id, level, message, metadata)
        VALUES (?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.job_id)
    .bind(input.level.as_str())
    .bind(&input.message)
    .bind(&metadata_json)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// List logs for a job.
/// Uses idx_job_logs_job index.
pub async fn list_job_logs(pool: &DbPool, job_id: &str) -> Result<Vec<JobLog>> {
    sqlx::query_as::<_, JobLog>(
        r#"
        SELECT * FROM job_logs
        WHERE job_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List logs for a job with level filter.
pub async fn list_job_logs_by_level(
    pool: &DbPool,
    job_id: &str,
    min_level: LogLevel,
) -> Result<Vec<JobLog>> {
    let levels = match min_level {
        LogLevel::Info => vec!["info", "warn", "error"],
        LogLevel::Warn => vec!["warn", "error"],
        LogLevel::Error => vec!["error"],
    };

    let placeholders: Vec<&str> = levels.iter().map(|_| "?").collect();
    let query = format!(
        r#"
        SELECT * FROM job_logs
        WHERE job_id = ? AND level IN ({})
        ORDER BY created_at ASC
        "#,
        placeholders.join(", ")
    );

    let mut q = sqlx::query_as::<_, JobLog>(&query);
    q = q.bind(job_id);
    for level in &levels {
        q = q.bind(*level);
    }

    q.fetch_all(pool).await.map_err(Error::Database)
}

/// Count errors in job logs.
pub async fn count_job_errors(pool: &DbPool, job_id: &str) -> Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM job_logs WHERE job_id = ? AND level = 'error'")
            .bind(job_id)
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Get latest logs for a job.
pub async fn get_latest_job_logs(pool: &DbPool, job_id: &str, limit: i64) -> Result<Vec<JobLog>> {
    sqlx::query_as::<_, JobLog>(
        r#"
        SELECT * FROM job_logs
        WHERE job_id = ?
        ORDER BY created_at DESC
        LIMIT ?
        "#,
    )
    .bind(job_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// Webhook Delivery Queries
// ============================================================================

/// Create a webhook delivery.
pub async fn create_webhook_delivery(
    pool: &DbPool,
    input: CreateWebhookDelivery,
) -> Result<WebhookDelivery> {
    let payload_json = serde_json::to_string(&input.payload)?;

    sqlx::query_as::<_, WebhookDelivery>(
        r#"
        INSERT INTO webhook_deliveries (id, type, target_url, payload)
        VALUES (?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.delivery_type)
    .bind(&input.target_url)
    .bind(&payload_json)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get a webhook delivery by ID.
pub async fn get_webhook_delivery(pool: &DbPool, id: &str) -> Result<WebhookDelivery> {
    sqlx::query_as::<_, WebhookDelivery>("SELECT * FROM webhook_deliveries WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Webhook delivery not found: {}", id)))
}

/// Update webhook delivery after attempt.
pub async fn update_webhook_delivery_attempt(
    pool: &DbPool,
    id: &str,
    success: bool,
    error: Option<&str>,
    next_attempt_at: Option<&str>,
) -> Result<WebhookDelivery> {
    let _status = if success { "success" } else { "failed" };

    sqlx::query_as::<_, WebhookDelivery>(
        r#"
        UPDATE webhook_deliveries SET
            status = CASE WHEN ? THEN 'success' ELSE status END,
            attempts = attempts + 1,
            last_attempt_at = datetime('now'),
            next_attempt_at = ?,
            error = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(success)
    .bind(next_attempt_at)
    .bind(error)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Webhook delivery not found: {}", id)))
}

/// List pending webhook deliveries ready for retry.
/// Uses idx_webhook_deliveries_next index.
pub async fn list_pending_webhook_deliveries(
    pool: &DbPool,
    limit: i64,
) -> Result<Vec<WebhookDelivery>> {
    sqlx::query_as::<_, WebhookDelivery>(
        r#"
        SELECT * FROM webhook_deliveries
        WHERE status = 'pending'
        AND (next_attempt_at IS NULL OR next_attempt_at <= datetime('now'))
        ORDER BY created_at ASC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Delete old successful webhook deliveries.
pub async fn cleanup_old_webhook_deliveries(pool: &DbPool, days: i64) -> Result<u64> {
    let result = sqlx::query(
        r#"
        DELETE FROM webhook_deliveries
        WHERE status = 'success'
        AND datetime(created_at, '+' || ? || ' days') < datetime('now')
        "#,
    )
    .bind(days)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_project, init_pool, migrate, CreateProject};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        create_project(
            &pool,
            CreateProject {
                id: "proj-1".to_string(),
                slug: "test".to_string(),
                name: "Test".to_string(),
                description: None,
            },
        )
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_job_lifecycle() {
        let pool = setup_test_db().await;

        // Create job using builder pattern
        let job = create_job(
            &pool,
            CreateJob::new("job-1".to_string(), JobType::IndexRepo)
                .with_project("proj-1")
                .with_total_items(100),
        )
        .await
        .unwrap();

        assert_eq!(job.status, "pending");
        assert_eq!(job.processed_items, 0);

        // Start job
        let job = start_job(&pool, "job-1").await.unwrap();
        assert!(job.is_running());
        assert!(job.started_at.is_some());

        // Update progress
        let job = update_job_progress(&pool, "job-1", 50, 2).await.unwrap();
        assert_eq!(job.processed_items, 50);
        assert_eq!(job.failed_items, 2);
        assert_eq!(job.progress_percent(), Some(50.0));

        // Complete job
        let result = JobResult {
            processed: 98,
            failed: 2,
            duration_ms: 5000,
            details: None,
        };
        let job = complete_job(&pool, "job-1", Some(&result)).await.unwrap();
        assert!(job.is_finished());
        assert!(job.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_atomic_claim_job() {
        let pool = setup_test_db().await;

        // Create jobs with different priorities
        create_job(
            &pool,
            CreateJob::new("job-low".to_string(), JobType::IndexRepo)
                .with_priority(JobPriority::Low),
        )
        .await
        .unwrap();

        create_job(
            &pool,
            CreateJob::new("job-high".to_string(), JobType::IndexRepo)
                .with_priority(JobPriority::High),
        )
        .await
        .unwrap();

        create_job(
            &pool,
            CreateJob::new("job-normal".to_string(), JobType::IndexRepo)
                .with_priority(JobPriority::Normal),
        )
        .await
        .unwrap();

        // Claim should get highest priority first
        let claimed = claim_job(&pool, "worker-1").await.unwrap();
        assert!(claimed.is_some());
        assert_eq!(claimed.as_ref().unwrap().id, "job-high");
        assert_eq!(
            claimed.as_ref().unwrap().locked_by.as_deref(),
            Some("worker-1")
        );

        // Next claim should get normal priority
        let claimed = claim_job(&pool, "worker-2").await.unwrap();
        assert!(claimed.is_some());
        assert_eq!(claimed.as_ref().unwrap().id, "job-normal");

        // And finally low priority
        let claimed = claim_job(&pool, "worker-3").await.unwrap();
        assert!(claimed.is_some());
        assert_eq!(claimed.as_ref().unwrap().id, "job-low");

        // No more jobs available
        let claimed = claim_job(&pool, "worker-4").await.unwrap();
        assert!(claimed.is_none());
    }

    #[tokio::test]
    async fn test_retry_job() {
        let pool = setup_test_db().await;

        // Create job with max 2 retries
        create_job(
            &pool,
            CreateJob::new("job-1".to_string(), JobType::IndexRepo).with_max_retries(2),
        )
        .await
        .unwrap();

        // Claim and fail
        claim_job(&pool, "worker-1").await.unwrap();

        // First retry
        let job = retry_job(&pool, "job-1", "Error 1").await.unwrap();
        assert!(job.is_some());
        assert_eq!(job.as_ref().unwrap().status, "retry");
        assert_eq!(job.as_ref().unwrap().retry_count, Some(1));

        // Claim again and fail
        // Note: We need to wait for scheduled_at or skip it for testing
        // For now, just update the job to be ready
        sqlx::query("UPDATE jobs SET scheduled_at = NULL WHERE id = 'job-1'")
            .execute(&pool)
            .await
            .unwrap();

        claim_job(&pool, "worker-2").await.unwrap();

        // Second retry
        let job = retry_job(&pool, "job-1", "Error 2").await.unwrap();
        assert!(job.is_some());
        assert_eq!(job.as_ref().unwrap().retry_count, Some(2));

        // Reset scheduled_at
        sqlx::query("UPDATE jobs SET scheduled_at = NULL WHERE id = 'job-1'")
            .execute(&pool)
            .await
            .unwrap();

        claim_job(&pool, "worker-3").await.unwrap();

        // Third retry - should fail permanently (max_retries=2)
        let job = retry_job(&pool, "job-1", "Error 3").await.unwrap();
        assert!(job.is_none()); // None means max retries exceeded

        // Verify job is failed
        let job = get_job(&pool, "job-1").await.unwrap();
        assert_eq!(job.status, "failed");
    }

    #[tokio::test]
    async fn test_release_job() {
        let pool = setup_test_db().await;

        create_job(
            &pool,
            CreateJob::new("job-1".to_string(), JobType::IndexRepo),
        )
        .await
        .unwrap();

        // Claim job
        let claimed = claim_job(&pool, "worker-1").await.unwrap();
        assert!(claimed.is_some());
        assert_eq!(claimed.unwrap().status, "running");

        // Release job
        let released = release_job(&pool, "job-1").await.unwrap();
        assert_eq!(released.status, "pending");
        assert!(released.locked_by.is_none());
        assert!(released.locked_at.is_none());

        // Job should be claimable again
        let claimed = claim_job(&pool, "worker-2").await.unwrap();
        assert!(claimed.is_some());
    }

    #[tokio::test]
    async fn test_queue_stats() {
        let pool = setup_test_db().await;

        // Create various jobs
        for i in 1..=3 {
            create_job(
                &pool,
                CreateJob::new(format!("pending-{}", i), JobType::IndexRepo),
            )
            .await
            .unwrap();
        }

        // Claim one (makes it running)
        claim_job(&pool, "worker-1").await.unwrap();

        // Complete one
        complete_job(&pool, "pending-2", None).await.ok();

        // Force claim another then fail it
        claim_job(&pool, "worker-2").await.unwrap();
        fail_job(&pool, "pending-3", "test error").await.unwrap();

        let stats = get_queue_stats(&pool).await.unwrap();
        assert_eq!(stats.running, 1);
        // pending-2 was completed, pending-3 failed, so we should check actual counts
    }

    #[tokio::test]
    async fn test_job_logs() {
        let pool = setup_test_db().await;

        create_job(
            &pool,
            CreateJob::new("job-1".to_string(), JobType::IndexRepo),
        )
        .await
        .unwrap();

        // Add logs
        create_job_log(
            &pool,
            CreateJobLog {
                job_id: "job-1".to_string(),
                level: LogLevel::Info,
                message: "Starting job".to_string(),
                metadata: None,
            },
        )
        .await
        .unwrap();

        create_job_log(
            &pool,
            CreateJobLog {
                job_id: "job-1".to_string(),
                level: LogLevel::Error,
                message: "Failed to process file".to_string(),
                metadata: Some(serde_json::json!({"file": "test.rs"})),
            },
        )
        .await
        .unwrap();

        let logs = list_job_logs(&pool, "job-1").await.unwrap();
        assert_eq!(logs.len(), 2);

        let errors = count_job_errors(&pool, "job-1").await.unwrap();
        assert_eq!(errors, 1);
    }

    #[tokio::test]
    async fn test_list_pending_jobs() {
        let pool = setup_test_db().await;

        for i in 1..=3 {
            create_job(
                &pool,
                CreateJob::new(format!("job-{}", i), JobType::IndexRepo),
            )
            .await
            .unwrap();
        }

        // Start one job
        start_job(&pool, "job-2").await.unwrap();

        let pending = list_pending_jobs(&pool, 10).await.unwrap();
        assert_eq!(pending.len(), 2);

        let running = list_jobs_by_status(&pool, JobStatus::Running)
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
    }

    #[tokio::test]
    async fn test_job_with_payload() {
        let pool = setup_test_db().await;

        let payload = serde_json::json!({
            "files": ["src/main.rs", "src/lib.rs"],
            "branch": "main",
            "commit_sha": "abc123"
        });

        let job = create_job(
            &pool,
            CreateJob::new("job-1".to_string(), JobType::IndexRepo)
                .with_payload(payload.clone())
                .with_priority(JobPriority::High),
        )
        .await
        .unwrap();

        assert!(job.payload.is_some());
        let stored_payload: serde_json::Value =
            serde_json::from_str(job.payload.as_ref().unwrap()).unwrap();
        assert_eq!(stored_payload["branch"], "main");
        assert_eq!(job.priority, Some(10)); // High = 10
    }

    #[tokio::test]
    async fn test_enqueue_job() {
        let pool = setup_test_db().await;

        let job = enqueue_job(
            &pool,
            JobType::SyncMetadata,
            Some(serde_json::json!({"repo": "test"})),
        )
        .await
        .unwrap();

        assert!(!job.id.is_empty());
        assert_eq!(job.job_type, "sync_metadata");
        assert_eq!(job.status, "pending");
    }
}
