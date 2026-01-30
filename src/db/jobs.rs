//! Background job and job log database queries.
//!
//! Tracks background indexing jobs, metadata sync, and other async operations.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

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
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IndexRepo => "index_repo",
            Self::ReindexRepo => "reindex_repo",
            Self::IndexHistory => "index_history",
            Self::SyncMetadata => "sync_metadata",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "index_repo" => Some(Self::IndexRepo),
            "reindex_repo" => Some(Self::ReindexRepo),
            "index_history" => Some(Self::IndexHistory),
            "sync_metadata" => Some(Self::SyncMetadata),
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
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
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
    pub repository_id: Option<String>,
    pub total_items: Option<i32>,
    pub processed_items: i32,
    pub failed_items: i32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result: Option<String>,  // JSON
    pub error: Option<String>,
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
        self.result.as_ref().and_then(|r| serde_json::from_str(r).ok())
    }
}

/// Input for creating a new job.
#[derive(Debug, Clone)]
pub struct CreateJob {
    pub id: String,
    pub job_type: JobType,
    pub project_id: Option<String>,
    pub repository_id: Option<String>,
    pub total_items: Option<i32>,
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
    pub metadata: Option<String>,  // JSON
    pub created_at: String,
}

impl JobLog {
    /// Get level as enum.
    pub fn level_enum(&self) -> LogLevel {
        LogLevel::from_str(&self.level)
    }

    /// Parse metadata JSON.
    pub fn metadata_json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        self.metadata.as_ref().and_then(|m| serde_json::from_str(m).ok())
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

// ============================================================================
// Job Queries
// ============================================================================

/// Create a new job.
pub async fn create_job(pool: &DbPool, input: CreateJob) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        INSERT INTO jobs (id, type, project_id, repository_id, total_items)
        VALUES (?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(input.job_type.as_str())
    .bind(&input.project_id)
    .bind(&input.repository_id)
    .bind(input.total_items)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
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

/// Start a job (set to running).
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

/// Complete a job successfully.
pub async fn complete_job(pool: &DbPool, id: &str, result: Option<&JobResult>) -> Result<Job> {
    let result_json = result.map(|r| serde_json::to_string(r).unwrap_or_default());

    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'completed',
            completed_at = datetime('now'),
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

/// Fail a job.
pub async fn fail_job(pool: &DbPool, id: &str, error: &str) -> Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET
            status = 'failed',
            completed_at = datetime('now'),
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

/// List pending jobs (for job runner).
pub async fn list_pending_jobs(pool: &DbPool, limit: i64) -> Result<Vec<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE status = 'pending'
        ORDER BY created_at ASC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
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

/// Get running job for repository (to prevent duplicate indexing).
pub async fn get_running_repo_job(
    pool: &DbPool,
    repository_id: &str,
    job_type: JobType,
) -> Result<Option<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT * FROM jobs
        WHERE repository_id = ? AND type = ? AND status = 'running'
        "#,
    )
    .bind(repository_id)
    .bind(job_type.as_str())
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Count jobs by status.
pub async fn count_jobs_by_status(pool: &DbPool, status: JobStatus) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM jobs WHERE status = ?",
    )
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
    let metadata_json = input.metadata.map(|m| serde_json::to_string(&m).unwrap_or_default());

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
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM job_logs WHERE job_id = ? AND level = 'error'",
    )
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
pub async fn create_webhook_delivery(pool: &DbPool, input: CreateWebhookDelivery) -> Result<WebhookDelivery> {
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
    let status = if success { "success" } else { "failed" };

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
pub async fn list_pending_webhook_deliveries(pool: &DbPool, limit: i64) -> Result<Vec<WebhookDelivery>> {
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
    use crate::db::{init_pool, migrate, create_project, CreateProject};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        create_project(&pool, CreateProject {
            id: "proj-1".to_string(),
            slug: "test".to_string(),
            name: "Test".to_string(),
            description: None,
        }).await.unwrap();

        pool
    }

    #[tokio::test]
    async fn test_job_lifecycle() {
        let pool = setup_test_db().await;

        // Create job
        let job = create_job(&pool, CreateJob {
            id: "job-1".to_string(),
            job_type: JobType::IndexRepo,
            project_id: Some("proj-1".to_string()),
            repository_id: None,
            total_items: Some(100),
        }).await.unwrap();

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
    async fn test_job_logs() {
        let pool = setup_test_db().await;

        create_job(&pool, CreateJob {
            id: "job-1".to_string(),
            job_type: JobType::IndexRepo,
            project_id: None,
            repository_id: None,
            total_items: None,
        }).await.unwrap();

        // Add logs
        create_job_log(&pool, CreateJobLog {
            job_id: "job-1".to_string(),
            level: LogLevel::Info,
            message: "Starting job".to_string(),
            metadata: None,
        }).await.unwrap();

        create_job_log(&pool, CreateJobLog {
            job_id: "job-1".to_string(),
            level: LogLevel::Error,
            message: "Failed to process file".to_string(),
            metadata: Some(serde_json::json!({"file": "test.rs"})),
        }).await.unwrap();

        let logs = list_job_logs(&pool, "job-1").await.unwrap();
        assert_eq!(logs.len(), 2);

        let errors = count_job_errors(&pool, "job-1").await.unwrap();
        assert_eq!(errors, 1);
    }

    #[tokio::test]
    async fn test_list_pending_jobs() {
        let pool = setup_test_db().await;

        for i in 1..=3 {
            create_job(&pool, CreateJob {
                id: format!("job-{}", i),
                job_type: JobType::IndexRepo,
                project_id: None,
                repository_id: None,
                total_items: None,
            }).await.unwrap();
        }

        // Start one job
        start_job(&pool, "job-2").await.unwrap();

        let pending = list_pending_jobs(&pool, 10).await.unwrap();
        assert_eq!(pending.len(), 2);

        let running = list_jobs_by_status(&pool, JobStatus::Running).await.unwrap();
        assert_eq!(running.len(), 1);
    }
}
