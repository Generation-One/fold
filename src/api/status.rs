//! Status Routes
//!
//! Health checks, status endpoints, and metrics.
//!
//! Routes:
//! - GET /health - Basic health check
//! - GET /health/ready - Readiness check (all dependencies up)
//! - GET /health/live - Liveness check (server responding)
//! - GET /status - Detailed system status
//! - GET /status/jobs - List background jobs
//! - GET /status/jobs/:id - Get job details
//! - GET /metrics - Prometheus metrics endpoint

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, Error, Result};

// Global metrics (simple counters)
static REQUEST_COUNT: AtomicU64 = AtomicU64::new(0);
static ERROR_COUNT: AtomicU64 = AtomicU64::new(0);

/// Increment request counter.
pub fn inc_request_count() {
    REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Increment error counter.
pub fn inc_error_count() {
    ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Build status routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/ready", get(readiness_check))
        .route("/health/live", get(liveness_check))
        .route("/status", get(system_status))
        .route("/status/jobs", get(list_jobs))
        .route("/status/jobs/:job_id", get(get_job))
        .route("/metrics", get(prometheus_metrics))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Readiness check response.
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub checks: Vec<DependencyCheck>,
}

#[derive(Debug, Serialize)]
pub struct DependencyCheck {
    pub name: String,
    pub status: HealthStatus,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

/// System status response.
#[derive(Debug, Serialize)]
pub struct SystemStatusResponse {
    pub status: HealthStatus,
    pub version: String,
    pub uptime_seconds: u64,
    pub database: DatabaseStatus,
    pub qdrant: QdrantStatus,
    pub embeddings: EmbeddingStatus,
    pub jobs: JobsStatus,
    pub metrics: SystemMetrics,
}

#[derive(Debug, Serialize)]
pub struct DatabaseStatus {
    pub connected: bool,
    pub pool_size: u32,
    pub active_connections: u32,
}

#[derive(Debug, Serialize)]
pub struct QdrantStatus {
    pub connected: bool,
    pub collections: u32,
    pub total_points: u64,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingStatus {
    pub model: String,
    pub loaded: bool,
    pub dimension: u32,
}

#[derive(Debug, Serialize)]
pub struct JobsStatus {
    pub pending: u32,
    pub running: u32,
    pub failed_24h: u32,
}

#[derive(Debug, Serialize)]
pub struct SystemMetrics {
    pub total_requests: u64,
    pub total_errors: u64,
    pub memory_usage_mb: u64,
}

/// Background job information.
#[derive(Debug, Serialize)]
pub struct JobInfo {
    pub id: Uuid,
    pub job_type: String,
    pub status: JobStatus,
    pub progress: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retry,
    Cancelled,
}

/// Query parameters for listing jobs.
#[derive(Debug, Deserialize, Default)]
pub struct ListJobsQuery {
    pub status: Option<JobStatus>,
    pub job_type: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

/// List jobs response.
#[derive(Debug, Serialize)]
pub struct ListJobsResponse {
    pub jobs: Vec<JobInfo>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct JobPath {
    pub job_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// Basic health check.
///
/// GET /health
///
/// Returns 200 if the server is running. Used by load balancers
/// for basic availability checking.
#[axum::debug_handler]
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: HealthStatus::Healthy,
        version: env!("CARGO_PKG_VERSION").into(),
        timestamp: Utc::now(),
    })
}

/// Readiness check.
///
/// GET /health/ready
///
/// Checks if all dependencies are available and the service
/// is ready to handle requests. Returns 503 if not ready.
#[axum::debug_handler]
async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    let mut checks = Vec::new();
    let mut all_healthy = true;

    // Check database
    let db_check = check_database(&state).await;
    if db_check.status != HealthStatus::Healthy {
        all_healthy = false;
    }
    checks.push(db_check);

    // Check Qdrant
    let qdrant_check = check_qdrant(&state).await;
    if qdrant_check.status != HealthStatus::Healthy {
        all_healthy = false;
    }
    checks.push(qdrant_check);

    // Check embeddings model
    let embedding_check = check_embeddings(&state).await;
    if embedding_check.status != HealthStatus::Healthy {
        all_healthy = false;
    }
    checks.push(embedding_check);

    let response = ReadinessResponse {
        ready: all_healthy,
        checks,
    };

    let status = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(response))
}

/// Liveness check.
///
/// GET /health/live
///
/// Simple check that the server is responding.
/// Used by Kubernetes for restart decisions.
#[axum::debug_handler]
async fn liveness_check() -> StatusCode {
    StatusCode::OK
}

/// Detailed system status.
///
/// GET /status
///
/// Returns comprehensive status information including
/// database stats, queue status, and metrics.
#[axum::debug_handler]
async fn system_status(State(state): State<AppState>) -> Result<Json<SystemStatusResponse>> {
    // TODO: Calculate actual uptime
    let uptime_seconds = 0u64;

    // Get database status
    let database = get_database_status(&state).await?;

    // Get Qdrant status
    let qdrant = get_qdrant_status(&state).await?;

    // Get embedding status
    let embeddings = get_embedding_status(&state)?;

    // Get jobs status
    let jobs = get_jobs_status(&state.db).await?;

    // Get metrics
    let metrics = SystemMetrics {
        total_requests: REQUEST_COUNT.load(Ordering::Relaxed),
        total_errors: ERROR_COUNT.load(Ordering::Relaxed),
        memory_usage_mb: get_memory_usage_mb(),
    };

    // Determine overall status
    let status = if database.connected && qdrant.connected {
        HealthStatus::Healthy
    } else if database.connected || qdrant.connected {
        HealthStatus::Degraded
    } else {
        HealthStatus::Unhealthy
    };

    Ok(Json(SystemStatusResponse {
        status,
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_seconds,
        database,
        qdrant,
        embeddings,
        jobs,
        metrics,
    }))
}

/// List background jobs.
///
/// GET /status/jobs
#[axum::debug_handler]
async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<ListJobsResponse>> {
    let limit = query.limit.min(100) as i64;
    let offset = query.offset as i64;

    // Convert query status to db status
    let db_status = query.status.map(|s| match s {
        JobStatus::Pending => crate::db::JobStatus::Pending,
        JobStatus::Running => crate::db::JobStatus::Running,
        JobStatus::Completed => crate::db::JobStatus::Completed,
        JobStatus::Failed => crate::db::JobStatus::Failed,
        JobStatus::Retry => crate::db::JobStatus::Retry,
        JobStatus::Cancelled => crate::db::JobStatus::Cancelled,
    });

    // Convert query job_type to db job_type
    let db_job_type = query.job_type.as_ref().and_then(|t| crate::db::JobType::from_str(t));

    // Fetch jobs from database
    let jobs = crate::db::list_jobs(&state.db, db_status, db_job_type, limit, offset).await?;

    // Convert to API response format
    let job_infos: Vec<JobInfo> = jobs.into_iter().map(|j| JobInfo {
        id: Uuid::parse_str(&j.id).unwrap_or_else(|_| Uuid::new_v4()),
        job_type: j.job_type,
        status: match j.status.as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "retry" => JobStatus::Retry,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending,
        },
        progress: j.total_items.map(|total| {
            if total == 0 { 100 } else { ((j.processed_items as f64 / total as f64) * 100.0) as u32 }
        }),
        created_at: j.created_at.parse().unwrap_or_else(|_| Utc::now()),
        started_at: j.started_at.and_then(|s| s.parse().ok()),
        completed_at: j.completed_at.and_then(|s| s.parse().ok()),
        error: j.error,
        metadata: j.payload.and_then(|p| serde_json::from_str(&p).ok()).unwrap_or(serde_json::json!({})),
    }).collect();

    // Get total count for pagination
    let total = crate::db::count_jobs_by_status(&state.db, db_status.unwrap_or(crate::db::JobStatus::Pending))
        .await
        .unwrap_or(job_infos.len() as i64) as u32;

    Ok(Json(ListJobsResponse {
        jobs: job_infos,
        total,
        offset: query.offset,
        limit: limit as u32,
    }))
}

/// Get job details.
///
/// GET /status/jobs/:job_id
#[axum::debug_handler]
async fn get_job(
    State(state): State<AppState>,
    Path(path): Path<JobPath>,
) -> Result<Json<JobInfo>> {
    let job_id = path.job_id.to_string();
    let job = crate::db::get_job(&state.db, &job_id).await?;

    Ok(Json(JobInfo {
        id: path.job_id,
        job_type: job.job_type,
        status: match job.status.as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "retry" => JobStatus::Retry,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending,
        },
        progress: job.total_items.map(|total| {
            if total == 0 { 100 } else { ((job.processed_items as f64 / total as f64) * 100.0) as u32 }
        }),
        created_at: job.created_at.parse().unwrap_or_else(|_| Utc::now()),
        started_at: job.started_at.and_then(|s| s.parse().ok()),
        completed_at: job.completed_at.and_then(|s| s.parse().ok()),
        error: job.error,
        metadata: job.payload.and_then(|p| serde_json::from_str(&p).ok()).unwrap_or(serde_json::json!({})),
    }))
}

/// Prometheus metrics endpoint.
///
/// GET /metrics
///
/// Returns metrics in Prometheus format.
#[axum::debug_handler]
async fn prometheus_metrics(State(_state): State<AppState>) -> impl IntoResponse {
    let total_requests = REQUEST_COUNT.load(Ordering::Relaxed);
    let total_errors = ERROR_COUNT.load(Ordering::Relaxed);
    let memory_mb = get_memory_usage_mb();

    let metrics = format!(
        r#"# HELP fold_requests_total Total number of HTTP requests
# TYPE fold_requests_total counter
fold_requests_total {}

# HELP fold_errors_total Total number of errors
# TYPE fold_errors_total counter
fold_errors_total {}

# HELP fold_memory_usage_bytes Current memory usage in bytes
# TYPE fold_memory_usage_bytes gauge
fold_memory_usage_bytes {}

# HELP fold_up Whether the service is up
# TYPE fold_up gauge
fold_up 1
"#,
        total_requests,
        total_errors,
        memory_mb * 1024 * 1024
    );

    (
        StatusCode::OK,
        [(
            "Content-Type",
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics,
    )
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check database connectivity.
async fn check_database(state: &AppState) -> DependencyCheck {
    let start = Instant::now();

    // TODO: Run a simple query to check connectivity
    let connected = true; // Placeholder

    let latency_ms = start.elapsed().as_millis() as u64;

    DependencyCheck {
        name: "database".into(),
        status: if connected {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        },
        latency_ms: Some(latency_ms),
        message: None,
    }
}

/// Check Qdrant connectivity.
async fn check_qdrant(state: &AppState) -> DependencyCheck {
    let start = Instant::now();

    // TODO: Check Qdrant connection
    let connected = true; // Placeholder

    let latency_ms = start.elapsed().as_millis() as u64;

    DependencyCheck {
        name: "qdrant".into(),
        status: if connected {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        },
        latency_ms: Some(latency_ms),
        message: None,
    }
}

/// Check embeddings model.
async fn check_embeddings(state: &AppState) -> DependencyCheck {
    let start = Instant::now();

    // TODO: Check if model is loaded
    let loaded = true; // Placeholder

    let latency_ms = start.elapsed().as_millis() as u64;

    DependencyCheck {
        name: "embeddings".into(),
        status: if loaded {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        },
        latency_ms: Some(latency_ms),
        message: None,
    }
}

/// Get database status.
async fn get_database_status(_state: &AppState) -> Result<DatabaseStatus> {
    // TODO: Get actual stats from connection pool
    Ok(DatabaseStatus {
        connected: true,
        pool_size: 10,
        active_connections: 0,
    })
}

/// Get Qdrant status.
async fn get_qdrant_status(_state: &AppState) -> Result<QdrantStatus> {
    // TODO: Get actual stats from Qdrant
    Ok(QdrantStatus {
        connected: true,
        collections: 0,
        total_points: 0,
    })
}

/// Get embedding model status.
fn get_embedding_status(state: &AppState) -> Result<EmbeddingStatus> {
    let config = crate::config();

    // Get model name from first provider, or indicate placeholder mode
    let model = config
        .embedding
        .providers
        .first()
        .map(|p| format!("{}/{}", p.name, p.model))
        .unwrap_or_else(|| "hash-placeholder".to_string());

    Ok(EmbeddingStatus {
        model,
        loaded: true,
        dimension: state.embeddings.dimension() as u32,
    })
}

/// Get jobs status.
async fn get_jobs_status(db: &crate::db::DbPool) -> Result<JobsStatus> {
    let stats = crate::db::get_queue_stats(db).await?;
    Ok(JobsStatus {
        pending: stats.pending as u32 + stats.retry as u32,
        running: stats.running as u32,
        failed_24h: stats.failed_24h as u32,
    })
}

/// Get current memory usage in MB.
fn get_memory_usage_mb() -> u64 {
    // This is a simplified implementation
    // In production, you might use jemalloc stats or /proc/self/status on Linux
    0
}
