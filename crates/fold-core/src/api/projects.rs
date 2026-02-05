//! Projects Routes
//!
//! CRUD operations for projects in the Fold system.
//!
//! Routes:
//! - GET /projects - List all projects
//! - POST /projects - Create a new project
//! - GET /projects/:id - Get project details
//! - PUT /projects/:id - Update project
//! - DELETE /projects/:id - Delete project

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build project routes.
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_projects).post(create_project))
        .route(
            "/:id",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/:id/stats", get(get_project_stats))
        .route("/:id/status", get(get_project_status))
        .route("/:id/reindex", post(reindex_project))
        .route("/:id/sync", post(sync_project))
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::middleware::require_auth,
        ))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Query parameters for listing projects.
#[derive(Debug, Deserialize, Default)]
pub struct ListProjectsQuery {
    /// Filter by name (partial match)
    #[allow(dead_code)]
    pub name: Option<String>,
    /// Pagination offset
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit (default 20, max 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Sort field
    #[serde(default)]
    #[allow(dead_code)]
    pub sort_by: ProjectSortField,
    /// Sort direction
    #[serde(default)]
    #[allow(dead_code)]
    pub sort_dir: SortDirection,
}

fn default_limit() -> u32 {
    20
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSortField {
    #[default]
    Name,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Request to create a new project.
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    /// Unique slug for the project (URL-friendly)
    pub slug: String,
    /// Human-readable name
    pub name: String,
    /// Project description
    pub description: Option<String>,
    /// Provider type: 'local', 'github', or 'gitlab'
    pub provider: String,
    /// Local path where the project root (and fold/) lives
    pub root_path: String,
    /// Remote repository owner (for github/gitlab)
    pub remote_owner: Option<String>,
    /// Remote repository name (for github/gitlab)
    pub remote_repo: Option<String>,
    /// Remote branch (default: main)
    pub remote_branch: Option<String>,
    /// Access token for remote provider
    pub access_token: Option<String>,
}

/// Request to update a project.
#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    /// Human-readable name
    pub name: Option<String>,
    /// Project description
    pub description: Option<String>,
    /// Author patterns to ignore during webhook processing (prevents loops)
    pub ignored_commit_authors: Option<Vec<String>>,
}

/// Project response.
#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    /// Provider type: 'local', 'github', or 'gitlab'
    pub provider: String,
    /// Local path where the project root (and fold/) lives
    pub root_path: String,
    /// Remote repository owner (for github/gitlab)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_owner: Option<String>,
    /// Remote repository name (for github/gitlab)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_repo: Option<String>,
    /// Remote branch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    pub memory_count: u32,
    /// Author patterns to ignore during webhook processing
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_commit_authors: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List of projects response.
#[derive(Debug, Serialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectResponse>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

/// Project statistics response.
#[derive(Debug, Serialize)]
pub struct ProjectStatsResponse {
    pub project_id: Uuid,
    pub project_slug: String,
    /// Total number of memories in the project.
    pub total_memories: u64,
    /// Memories by type (codebase, session, decision, etc.)
    pub memories_by_type: MemoryTypeCounts,
    /// Memories by source (file, agent, git)
    pub memories_by_source: MemorySourceCounts,
    /// Total number of chunks (code segments).
    pub total_chunks: u64,
    /// Total number of links between memories.
    pub total_links: u64,
    /// Total number of vectors in Qdrant.
    pub total_vectors: u64,
}

/// Memory counts by type.
#[derive(Debug, Serialize, Default)]
pub struct MemoryTypeCounts {
    pub codebase: u64,
    pub session: u64,
    pub decision: u64,
    pub spec: u64,
    pub commit: u64,
    pub pr: u64,
    pub task: u64,
    pub general: u64,
}

/// Memory counts by source.
#[derive(Debug, Serialize, Default)]
pub struct MemorySourceCounts {
    pub file: u64,
    pub agent: u64,
    pub git: u64,
}

// ============================================================================
// Project Status Types (comprehensive status endpoint)
// ============================================================================

/// Comprehensive project status response.
/// Returns detailed status information about all aspects of a project.
#[derive(Debug, Serialize)]
pub struct ProjectStatusResponse {
    /// Project identification
    pub project: ProjectInfo,
    /// Overall health status
    pub health: HealthInfo,
    /// SQLite database statistics
    pub database: DatabaseStats,
    /// Qdrant vector database statistics
    pub vector_db: VectorDbStats,
    /// Job queue statistics for this project
    pub jobs: JobStats,
    /// Recent job history
    pub recent_jobs: Vec<RecentJob>,
    /// File system status (if local project)
    pub filesystem: Option<FilesystemStats>,
    /// Indexing status
    pub indexing: IndexingStatus,
    /// Timestamps
    pub timestamps: ProjectTimestamps,
}

/// Project identification info.
#[derive(Debug, Serialize)]
pub struct ProjectInfo {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub provider: String,
    pub root_path: Option<String>,
    pub remote_owner: Option<String>,
    pub remote_repo: Option<String>,
    pub remote_branch: Option<String>,
}

/// Health status for the project.
#[derive(Debug, Serialize)]
pub struct HealthInfo {
    /// Overall status: healthy, degraded, unhealthy
    pub status: String,
    /// Is the project accessible (root_path exists)?
    pub accessible: bool,
    /// Is the vector collection healthy?
    pub vector_collection_exists: bool,
    /// Are there any failed jobs in the last 24h?
    pub has_recent_failures: bool,
    /// Is indexing currently in progress?
    pub indexing_in_progress: bool,
    /// Issues detected
    pub issues: Vec<String>,
}

/// SQLite database statistics for the project.
#[derive(Debug, Serialize)]
pub struct DatabaseStats {
    /// Total memories in the project
    pub total_memories: u64,
    /// Memories by type
    pub memories_by_type: MemoryTypeCounts,
    /// Memories by source
    pub memories_by_source: MemorySourceCounts,
    /// Total chunks (code segments)
    pub total_chunks: u64,
    /// Total links between memories
    pub total_links: u64,
    /// Total attachments
    pub total_attachments: u64,
    /// Database size in bytes (estimated)
    pub estimated_size_bytes: u64,
}

/// Qdrant vector database statistics.
#[derive(Debug, Serialize)]
pub struct VectorDbStats {
    /// Collection name in Qdrant
    pub collection_name: String,
    /// Whether the collection exists
    pub exists: bool,
    /// Total vectors/points stored
    pub total_vectors: u64,
    /// Vector dimension
    pub dimension: usize,
    /// Sync status: vectors vs memories
    pub sync_status: VectorSyncStatus,
}

/// Vector sync status.
#[derive(Debug, Serialize)]
pub struct VectorSyncStatus {
    /// Number of memories in SQLite
    pub memory_count: u64,
    /// Number of vectors in Qdrant
    pub vector_count: u64,
    /// Whether counts match
    pub in_sync: bool,
    /// Difference (positive = missing vectors, negative = orphan vectors)
    pub difference: i64,
}

/// Job queue statistics for the project.
#[derive(Debug, Serialize)]
pub struct JobStats {
    /// Total jobs ever created for this project
    pub total: u64,
    /// Currently pending jobs
    pub pending: u64,
    /// Currently running jobs
    pub running: u64,
    /// Completed jobs
    pub completed: u64,
    /// Failed jobs
    pub failed: u64,
    /// Paused jobs (waiting for resources)
    pub paused: u64,
    /// Jobs completed in last 24h
    pub completed_24h: u64,
    /// Jobs failed in last 24h
    pub failed_24h: u64,
    /// Jobs by type
    pub by_type: JobTypeCounts,
}

/// Job counts by type.
#[derive(Debug, Serialize, Default)]
pub struct JobTypeCounts {
    pub index_repo: u64,
    pub reindex_repo: u64,
    pub index_history: u64,
    pub sync_metadata: u64,
    pub process_webhook: u64,
    pub generate_summary: u64,
    pub custom: u64,
}

/// Recent job info.
#[derive(Debug, Serialize)]
pub struct RecentJob {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub progress: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

/// Filesystem statistics (for local projects).
#[derive(Debug, Serialize)]
pub struct FilesystemStats {
    /// Whether the root path exists
    pub root_exists: bool,
    /// Whether the fold/ directory exists
    pub fold_dir_exists: bool,
    /// Number of files matching index patterns (estimate)
    pub indexable_files_estimate: u64,
    /// Total size of fold/ directory in bytes
    pub fold_dir_size_bytes: u64,
}

/// Indexing status.
#[derive(Debug, Serialize)]
pub struct IndexingStatus {
    /// Is indexing currently running?
    pub in_progress: bool,
    /// Current job ID if running
    pub current_job_id: Option<String>,
    /// Progress percentage if running
    pub progress: Option<u32>,
    /// Last successful index time
    pub last_indexed_at: Option<DateTime<Utc>>,
    /// Last index job duration in seconds
    pub last_duration_secs: Option<u64>,
}

/// Project timestamps.
#[derive(Debug, Serialize)]
pub struct ProjectTimestamps {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_indexed_at: Option<DateTime<Utc>>,
    pub last_job_completed_at: Option<DateTime<Utc>>,
    pub last_job_failed_at: Option<DateTime<Utc>>,
    pub last_memory_created_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Handlers
// ============================================================================

/// List all projects.
///
/// GET /projects
///
/// Returns a paginated list of projects the user has access to.
/// Users see only projects they have direct or group membership in.
/// Admins see all projects.
#[axum::debug_handler]
async fn list_projects(
    State(state): State<AppState>,
    Query(query): Query<ListProjectsQuery>,
    axum::extract::Extension(auth): axum::extract::Extension<crate::middleware::AuthUser>,
) -> Result<Json<ListProjectsResponse>> {
    let limit = query.limit.min(100);

    // Admin users see all projects
    if auth.is_admin() {
        // Fetch all projects from database
        let projects =
            crate::db::list_projects_paginated(&state.db, limit as i64, query.offset as i64)
                .await?;

        // Get total count
        let total = crate::db::count_projects(&state.db).await.unwrap_or(0) as u32;

        let project_responses: Vec<ProjectResponse> = projects
            .into_iter()
            .map(|p| {
                let ignored_authors = p
                    .ignored_commit_authors
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                ProjectResponse {
                    id: p.id.parse().unwrap_or_default(),
                    slug: p.slug,
                    name: p.name,
                    description: p.description,
                    provider: p.provider,
                    root_path: p.root_path,
                    remote_owner: p.remote_owner,
                    remote_repo: p.remote_repo,
                    remote_branch: p.remote_branch,
                    memory_count: 0,
                    ignored_commit_authors: ignored_authors,
                    created_at: p.created_at.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: p.updated_at.parse().unwrap_or_else(|_| Utc::now()),
                }
            })
            .collect();

        return Ok(Json(ListProjectsResponse {
            projects: project_responses,
            total,
            offset: query.offset,
            limit,
        }));
    }

    // Non-admin users: get projects they have access to
    let perm_service = crate::services::PermissionService::new(state.db.clone());
    let accessible_projects = perm_service
        .get_accessible_projects(&auth.user_id, &auth.role)
        .await?;

    // Fetch all projects and filter to accessible ones
    let all_projects = crate::db::list_projects_paginated(
        &state.db, 10000, // Large number to get all
        0,
    )
    .await?;

    let accessible_project_ids: std::collections::HashSet<String> =
        accessible_projects.into_iter().collect();

    let filtered_projects: Vec<_> = all_projects
        .into_iter()
        .filter(|p| accessible_project_ids.contains(&p.id))
        .collect();

    // Apply pagination to filtered results
    let total = filtered_projects.len() as u32;
    let paginated = filtered_projects
        .into_iter()
        .skip(query.offset as usize)
        .take(limit as usize)
        .collect::<Vec<_>>();

    let project_responses: Vec<ProjectResponse> = paginated
        .into_iter()
        .map(|p| {
            let ignored_authors = p
                .ignored_commit_authors
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            ProjectResponse {
                id: p.id.parse().unwrap_or_default(),
                slug: p.slug,
                name: p.name,
                description: p.description,
                provider: p.provider,
                root_path: p.root_path,
                remote_owner: p.remote_owner,
                remote_repo: p.remote_repo,
                remote_branch: p.remote_branch,
                memory_count: 0,
                ignored_commit_authors: ignored_authors,
                created_at: p.created_at.parse().unwrap_or_else(|_| Utc::now()),
                updated_at: p.updated_at.parse().unwrap_or_else(|_| Utc::now()),
            }
        })
        .collect();

    Ok(Json(ListProjectsResponse {
        projects: project_responses,
        total,
        offset: query.offset,
        limit,
    }))
}

/// Create a new project.
///
/// POST /projects
///
/// Creates a new project with the given details.
/// The creating user is automatically added as a member with write access.
#[axum::debug_handler]
async fn create_project(
    State(state): State<AppState>,
    axum::extract::Extension(auth): axum::extract::Extension<crate::middleware::AuthUser>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>> {
    // Validate slug format
    if !is_valid_slug(&request.slug) {
        return Err(Error::Validation(
            "Slug must be lowercase alphanumeric with hyphens only".into(),
        ));
    }

    // Validate provider
    let valid_providers = ["local", "github", "gitlab"];
    if !valid_providers.contains(&request.provider.as_str()) {
        return Err(Error::Validation(
            "Provider must be 'local', 'github', or 'gitlab'".into(),
        ));
    }

    // For remote providers, require owner and repo
    if request.provider != "local" {
        if request.remote_owner.is_none() || request.remote_repo.is_none() {
            return Err(Error::Validation(
                "remote_owner and remote_repo are required for github/gitlab providers".into(),
            ));
        }
    }

    // Create project in database
    let input = crate::db::CreateProject {
        id: Uuid::new_v4().to_string(),
        slug: request.slug,
        name: request.name,
        description: request.description,
        provider: request.provider,
        root_path: request.root_path,
        remote_owner: request.remote_owner,
        remote_repo: request.remote_repo,
        remote_branch: request.remote_branch,
        access_token: request.access_token,
    };

    let project = crate::db::create_project(&state.db, input).await?;

    // Add the creating user as a member with write access
    let _ = crate::db::add_project_member(
        &state.db,
        &project.id,
        &auth.user_id,
        "member",
        Some(&auth.user_id),
    )
    .await;

    // Initialize Qdrant collection for project (non-blocking)
    match state
        .qdrant
        .create_collection(&project.slug, state.embeddings.dimension().await)
        .await
    {
        Ok(()) => info!(slug = %project.slug, "Created Qdrant collection"),
        Err(e) => {
            warn!(error = %e, slug = %project.slug, "Failed to create Qdrant collection, search unavailable")
        }
    }

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(ProjectResponse {
        id: project.id.parse().unwrap_or_default(),
        slug: project.slug,
        name: project.name,
        description: project.description,
        provider: project.provider,
        root_path: project.root_path,
        remote_owner: project.remote_owner,
        remote_repo: project.remote_repo,
        remote_branch: project.remote_branch,
        memory_count: 0,
        ignored_commit_authors: ignored_authors,
        created_at: project.created_at.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: project.updated_at.parse().unwrap_or_else(|_| Utc::now()),
    }))
}

/// Get a project by ID or slug.
///
/// GET /projects/:id
///
/// Returns the full details for a single project.
#[axum::debug_handler]
async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(ProjectResponse {
        id: project.id.parse().unwrap_or_default(),
        slug: project.slug,
        name: project.name,
        description: project.description,
        provider: project.provider,
        root_path: project.root_path,
        remote_owner: project.remote_owner,
        remote_repo: project.remote_repo,
        remote_branch: project.remote_branch,
        memory_count: 0,
        ignored_commit_authors: ignored_authors,
        created_at: project.created_at.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: project.updated_at.parse().unwrap_or_else(|_| Utc::now()),
    }))
}

/// Update a project.
///
/// PUT /projects/:id
///
/// Updates the project with the given ID.
#[axum::debug_handler]
async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>> {
    // First fetch the project to get its actual ID (in case user passed a slug)
    let existing = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    // Serialize ignored_commit_authors if provided
    let ignored_authors_json = request
        .ignored_commit_authors
        .map(|authors| serde_json::to_string(&authors).ok())
        .flatten();

    let input = crate::db::UpdateProject {
        slug: None, // Don't allow slug changes via this endpoint
        name: request.name,
        description: request.description,
        ignored_commit_authors: ignored_authors_json,
    };

    let project = crate::db::update_project(&state.db, &existing.id, input).await?;

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(ProjectResponse {
        id: project.id.parse().unwrap_or_default(),
        slug: project.slug,
        name: project.name,
        description: project.description,
        provider: project.provider,
        root_path: project.root_path,
        remote_owner: project.remote_owner,
        remote_repo: project.remote_repo,
        remote_branch: project.remote_branch,
        memory_count: 0,
        ignored_commit_authors: ignored_authors,
        created_at: project.created_at.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: project.updated_at.parse().unwrap_or_else(|_| Utc::now()),
    }))
}

/// Delete a project.
///
/// DELETE /projects/:id
///
/// Deletes the project and all associated data:
/// - Cancels all pending/running jobs
/// - Deletes all jobs for the project
/// - Deletes the project (cascades to memories, links, chunks, etc.)
/// - Deletes the Qdrant vector collection
///
/// This action is irreversible.
#[axum::debug_handler]
async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // First fetch the project to get its actual ID (in case user passed a slug)
    let existing = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    info!(project_id = %existing.id, slug = %existing.slug, "Deleting project and all associated data");

    // 1. Cancel all pending/running jobs for this project
    let cancelled_jobs = crate::db::cancel_project_jobs(&state.db, &existing.id).await?;
    if cancelled_jobs > 0 {
        info!(project_id = %existing.id, count = cancelled_jobs, "Cancelled pending jobs");
    }

    // 2. Delete all jobs for this project (including completed/failed)
    let deleted_jobs = crate::db::delete_project_jobs(&state.db, &existing.id).await?;
    if deleted_jobs > 0 {
        info!(project_id = %existing.id, count = deleted_jobs, "Deleted jobs");
    }

    // 3. Delete project and all associated data (cascade handled by DB)
    // This deletes: memories, memory_links, chunks, project_members, etc.
    crate::db::delete_project(&state.db, &existing.id).await?;

    // 4. Delete Qdrant collection (vector database cleanup)
    match state.qdrant.delete_collection(&existing.slug).await {
        Ok(()) => info!(slug = %existing.slug, "Deleted Qdrant collection"),
        Err(e) => warn!(error = %e, slug = %existing.slug, "Failed to delete Qdrant collection"),
    }

    info!(
        project_id = %existing.id,
        slug = %existing.slug,
        "Project deletion complete"
    );

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": existing.id,
        "jobs_cancelled": cancelled_jobs,
        "jobs_deleted": deleted_jobs
    })))
}

/// Get project statistics.
///
/// GET /projects/:id/stats
///
/// Returns detailed statistics for a project including memory counts,
/// vector counts, and more.
#[axum::debug_handler]
async fn get_project_stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectStatsResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    // Get total memories
    let total_memories =
        crate::db::count_project_memories(&state.db, &project.id).await? as u64;

    // Get memories by type
    use crate::db::MemoryType;
    let memories_by_type = MemoryTypeCounts {
        codebase: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Codebase).await.unwrap_or(0) as u64,
        session: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Session).await.unwrap_or(0) as u64,
        decision: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Decision).await.unwrap_or(0) as u64,
        spec: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Spec).await.unwrap_or(0) as u64,
        commit: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Commit).await.unwrap_or(0) as u64,
        pr: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Pr).await.unwrap_or(0) as u64,
        task: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Task).await.unwrap_or(0) as u64,
        general: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::General).await.unwrap_or(0) as u64,
    };

    // Get memories by source
    let memories_by_source = MemorySourceCounts {
        file: crate::db::count_project_memories_by_source(&state.db, &project.id, "file").await.unwrap_or(0) as u64,
        agent: crate::db::count_project_memories_by_source(&state.db, &project.id, "agent").await.unwrap_or(0) as u64,
        git: crate::db::count_project_memories_by_source(&state.db, &project.id, "git").await.unwrap_or(0) as u64,
    };

    // Get total chunks
    let total_chunks =
        crate::db::count_chunks_for_project(&state.db, &project.id).await.unwrap_or(0) as u64;

    // Get total links
    let total_links =
        crate::db::count_project_links(&state.db, &project.id).await.unwrap_or(0) as u64;

    // Get vector count from Qdrant
    let total_vectors = state
        .qdrant
        .collection_info(&project.slug)
        .await
        .map(|info| info.points_count)
        .unwrap_or(0);

    Ok(Json(ProjectStatsResponse {
        project_id: project.id.parse().unwrap_or_default(),
        project_slug: project.slug,
        total_memories,
        memories_by_type,
        memories_by_source,
        total_chunks,
        total_links,
        total_vectors,
    }))
}

/// Get comprehensive project status.
///
/// GET /projects/:id/status
///
/// Returns complete status information including:
/// - Project info and health
/// - SQLite database statistics (memories, chunks, links, attachments)
/// - Qdrant vector database statistics
/// - Job queue status and recent jobs
/// - Filesystem status (for local projects)
/// - Indexing status
/// - All relevant timestamps
#[axum::debug_handler]
async fn get_project_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectStatusResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    // ========================================================================
    // Project Info
    // ========================================================================
    let project_info = ProjectInfo {
        id: project.id.clone(),
        slug: project.slug.clone(),
        name: project.name.clone(),
        description: project.description.clone(),
        provider: project.provider.clone(),
        root_path: Some(project.root_path.clone()),
        remote_owner: project.remote_owner.clone(),
        remote_repo: project.remote_repo.clone(),
        remote_branch: project.remote_branch.clone(),
    };

    // ========================================================================
    // Database Stats
    // ========================================================================
    let total_memories =
        crate::db::count_project_memories(&state.db, &project.id).await? as u64;

    use crate::db::MemoryType;
    let memories_by_type = MemoryTypeCounts {
        codebase: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Codebase).await.unwrap_or(0) as u64,
        session: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Session).await.unwrap_or(0) as u64,
        decision: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Decision).await.unwrap_or(0) as u64,
        spec: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Spec).await.unwrap_or(0) as u64,
        commit: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Commit).await.unwrap_or(0) as u64,
        pr: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Pr).await.unwrap_or(0) as u64,
        task: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Task).await.unwrap_or(0) as u64,
        general: crate::db::count_project_memories_by_type(&state.db, &project.id, MemoryType::General).await.unwrap_or(0) as u64,
    };

    let memories_by_source = MemorySourceCounts {
        file: crate::db::count_project_memories_by_source(&state.db, &project.id, "file").await.unwrap_or(0) as u64,
        agent: crate::db::count_project_memories_by_source(&state.db, &project.id, "agent").await.unwrap_or(0) as u64,
        git: crate::db::count_project_memories_by_source(&state.db, &project.id, "git").await.unwrap_or(0) as u64,
    };

    let total_chunks =
        crate::db::count_chunks_for_project(&state.db, &project.id).await.unwrap_or(0) as u64;
    let total_links =
        crate::db::count_project_links(&state.db, &project.id).await.unwrap_or(0) as u64;
    let total_attachments =
        crate::db::count_project_attachments(&state.db, &project.id).await.unwrap_or(0) as u64;

    // Estimate database size (rough calculation based on row counts)
    let estimated_db_size = (total_memories * 2048) + (total_chunks * 512) + (total_links * 128);

    let database_stats = DatabaseStats {
        total_memories,
        memories_by_type,
        memories_by_source,
        total_chunks,
        total_links,
        total_attachments,
        estimated_size_bytes: estimated_db_size,
    };

    // ========================================================================
    // Vector DB Stats
    // ========================================================================
    let collection_info = state.qdrant.collection_info(&project.slug).await;
    let (collection_exists, total_vectors, dimension) = match &collection_info {
        Ok(info) => (info.exists, info.points_count, info.dimension),
        Err(_) => (false, 0, 0),
    };

    let vector_sync_status = VectorSyncStatus {
        memory_count: total_memories,
        vector_count: total_vectors,
        in_sync: total_memories == total_vectors,
        difference: total_memories as i64 - total_vectors as i64,
    };

    let vector_db_stats = VectorDbStats {
        collection_name: format!("fold_{}", project.slug),
        exists: collection_exists,
        total_vectors,
        dimension,
        sync_status: vector_sync_status,
    };

    // ========================================================================
    // Job Stats
    // ========================================================================
    let job_stats_raw = crate::db::get_project_job_stats(&state.db, &project.id).await?;

    use crate::db::JobType;
    let jobs_by_type = JobTypeCounts {
        index_repo: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::IndexRepo).await.unwrap_or(0) as u64,
        reindex_repo: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::ReindexRepo).await.unwrap_or(0) as u64,
        index_history: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::IndexHistory).await.unwrap_or(0) as u64,
        sync_metadata: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::SyncMetadata).await.unwrap_or(0) as u64,
        process_webhook: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::ProcessWebhook).await.unwrap_or(0) as u64,
        generate_summary: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::GenerateSummary).await.unwrap_or(0) as u64,
        custom: crate::db::count_project_jobs_by_type(&state.db, &project.id, JobType::Custom).await.unwrap_or(0) as u64,
    };

    let job_stats = JobStats {
        total: job_stats_raw.total as u64,
        pending: job_stats_raw.pending as u64,
        running: job_stats_raw.running as u64,
        completed: job_stats_raw.completed as u64,
        failed: job_stats_raw.failed as u64,
        paused: job_stats_raw.paused as u64,
        completed_24h: job_stats_raw.completed_24h as u64,
        failed_24h: job_stats_raw.failed_24h as u64,
        by_type: jobs_by_type,
    };

    // Get recent jobs
    let recent_jobs_raw = crate::db::list_project_jobs(&state.db, &project.id, 10, 0).await?;
    let recent_jobs: Vec<RecentJob> = recent_jobs_raw
        .into_iter()
        .map(|j| {
            let progress = j.total_items.map(|total| {
                if total == 0 {
                    100
                } else {
                    ((j.processed_items as f64 / total as f64) * 100.0) as u32
                }
            });
            RecentJob {
                id: j.id,
                job_type: j.job_type,
                status: j.status,
                progress,
                created_at: parse_datetime(&j.created_at),
                completed_at: j.completed_at.map(|s| parse_datetime(&s)),
                error: j.error,
            }
        })
        .collect();

    // ========================================================================
    // Filesystem Stats (for local projects)
    // ========================================================================
    let filesystem = {
        let root_path = &project.root_path;
        let root = std::path::Path::new(root_path);
        let root_exists = root.exists();
        let fold_dir = root.join("fold");
        let fold_dir_exists = fold_dir.exists();

        // Estimate indexable files (quick count, not exhaustive)
        let indexable_estimate = if root_exists {
            // Count common source files in root (limited depth)
            count_indexable_files(root_path, 3).unwrap_or(0)
        } else {
            0
        };

        // Get fold/ directory size
        let fold_size = if fold_dir_exists {
            dir_size(&fold_dir).unwrap_or(0)
        } else {
            0
        };

        Some(FilesystemStats {
            root_exists,
            fold_dir_exists,
            indexable_files_estimate: indexable_estimate,
            fold_dir_size_bytes: fold_size,
        })
    };

    // ========================================================================
    // Indexing Status
    // ========================================================================
    let running_index_job = crate::db::get_running_project_job(
        &state.db,
        &project.id,
        JobType::ReindexRepo,
    )
    .await?;

    let (indexing_in_progress, current_job_id, index_progress) = match &running_index_job {
        Some(job) => {
            let progress = job.total_items.map(|total| {
                if total == 0 {
                    0
                } else {
                    ((job.processed_items as f64 / total as f64) * 100.0) as u32
                }
            });
            (true, Some(job.id.clone()), progress)
        }
        None => (false, None, None),
    };

    // Get last successful index job
    let last_index_job = crate::db::list_project_jobs(&state.db, &project.id, 50, 0)
        .await?
        .into_iter()
        .find(|j| (j.job_type == "reindex_repo" || j.job_type == "index_repo") && j.status == "completed");

    let (last_indexed_at, last_duration_secs) = match last_index_job {
        Some(job) => {
            let completed = job.completed_at.map(|s| parse_datetime(&s));
            let started = job.started_at.map(|s| parse_datetime(&s));
            let duration = match (started, completed) {
                (Some(s), Some(c)) => Some((c - s).num_seconds() as u64),
                _ => None,
            };
            (completed, duration)
        }
        None => (None, None),
    };

    let indexing_status = IndexingStatus {
        in_progress: indexing_in_progress,
        current_job_id,
        progress: index_progress,
        last_indexed_at,
        last_duration_secs,
    };

    // ========================================================================
    // Health Assessment
    // ========================================================================
    let mut issues = Vec::new();
    let accessible = std::path::Path::new(&project.root_path).exists();

    if !accessible {
        issues.push("Project root path does not exist".to_string());
    }
    if !collection_exists {
        issues.push("Vector collection not found in Qdrant".to_string());
    }
    if !vector_db_stats.sync_status.in_sync {
        issues.push(format!(
            "Vector sync mismatch: {} memories, {} vectors",
            total_memories, total_vectors
        ));
    }
    if job_stats_raw.failed_24h > 0 {
        issues.push(format!("{} failed jobs in last 24h", job_stats_raw.failed_24h));
    }
    if job_stats_raw.paused > 0 {
        issues.push(format!("{} jobs paused (waiting for resources)", job_stats_raw.paused));
    }

    let health_status = if issues.is_empty() {
        "healthy"
    } else if issues.len() == 1 && job_stats_raw.paused > 0 {
        "degraded"
    } else if !accessible || !collection_exists {
        "unhealthy"
    } else {
        "degraded"
    };

    let health = HealthInfo {
        status: health_status.to_string(),
        accessible,
        vector_collection_exists: collection_exists,
        has_recent_failures: job_stats_raw.failed_24h > 0,
        indexing_in_progress,
        issues,
    };

    // ========================================================================
    // Timestamps
    // ========================================================================
    // Get last memory created timestamp
    let last_memory = crate::db::get_latest_project_memory(&state.db, &project.id).await.ok();
    let last_memory_created_at = last_memory.map(|m| parse_datetime(&m.created_at));

    let timestamps = ProjectTimestamps {
        created_at: parse_datetime(&project.created_at),
        updated_at: parse_datetime(&project.updated_at),
        last_indexed_at,
        last_job_completed_at: job_stats_raw.last_completed_at.map(|s| parse_datetime(&s)),
        last_job_failed_at: job_stats_raw.last_failed_at.map(|s| parse_datetime(&s)),
        last_memory_created_at,
    };

    Ok(Json(ProjectStatusResponse {
        project: project_info,
        health,
        database: database_stats,
        vector_db: vector_db_stats,
        jobs: job_stats,
        recent_jobs,
        filesystem,
        indexing: indexing_status,
        timestamps,
    }))
}

/// Parse a datetime string to DateTime<Utc>.
fn parse_datetime(s: &str) -> DateTime<Utc> {
    use chrono::NaiveDateTime;
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|ndt| ndt.and_utc()))
        .unwrap_or_else(|_| Utc::now())
}

/// Count indexable files in a directory (limited depth).
fn count_indexable_files(root: &str, max_depth: usize) -> Option<u64> {
    let extensions = ["rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "cpp", "c", "h", "md"];
    let mut count = 0u64;

    fn visit_dir(path: &std::path::Path, extensions: &[&str], count: &mut u64, depth: usize, max_depth: usize) {
        if depth > max_depth {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    // Skip common non-source directories
                    if !["node_modules", ".git", "target", "dist", "build", "__pycache__", "vendor"].contains(&name) {
                        visit_dir(&path, extensions, count, depth + 1, max_depth);
                    }
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if extensions.contains(&ext) {
                        *count += 1;
                    }
                }
            }
        }
    }

    visit_dir(std::path::Path::new(root), &extensions, &mut count, 0, max_depth);
    Some(count)
}

/// Calculate directory size in bytes.
fn dir_size(path: &std::path::Path) -> Option<u64> {
    let mut size = 0u64;

    fn visit(path: &std::path::Path, size: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, size);
                } else if let Ok(meta) = path.metadata() {
                    *size += meta.len();
                }
            }
        }
    }

    visit(path, &mut size);
    Some(size)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a project ID from either UUID or slug format.
#[allow(dead_code)]
fn parse_project_id(id: &str) -> Result<ProjectIdent> {
    if let Ok(uuid) = Uuid::parse_str(id) {
        Ok(ProjectIdent::Id(uuid))
    } else {
        Ok(ProjectIdent::Slug(id.to_string()))
    }
}

/// Project identifier - either UUID or slug.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ProjectIdent {
    Id(Uuid),
    Slug(String),
}

/// Check if a string is a valid project slug.
fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

// ============================================================================
// Project Members Routes
// ============================================================================

/// Build project members routes.
///
/// These routes use the full path pattern /:project_id/members
pub fn members_routes() -> Router<AppState> {
    Router::new()
        .route("/:project_id/members", get(list_members).post(add_member))
        .route(
            "/:project_id/members/:user_id",
            get(get_member).put(update_member).delete(remove_member),
        )
}

// ============================================================================
// Member Request/Response Types
// ============================================================================

/// Request to add a member to a project.
#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    /// User ID to add
    pub user_id: String,
    /// Role: "member" (read/write) or "viewer" (read-only)
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "viewer".to_string()
}

/// Request to update a member's role.
#[derive(Debug, Deserialize)]
pub struct UpdateMemberRequest {
    /// Role: "member" (read/write) or "viewer" (read-only)
    pub role: String,
}

/// Member response.
#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub project_id: String,
    pub role: String,
    pub can_write: bool,
    pub added_by: Option<String>,
    pub created_at: DateTime<Utc>,
    // User info (when available)
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// List members response.
#[derive(Debug, Serialize)]
pub struct ListMembersResponse {
    pub members: Vec<MemberResponse>,
    pub total: u32,
}

// ============================================================================
// Member Handlers
// ============================================================================

/// List all members of a project.
///
/// GET /projects/:project_id/members
#[axum::debug_handler]
async fn list_members(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<ListMembersResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Get members with user details
    let members = crate::db::list_project_members_with_users(&state.db, &project.id).await?;

    let member_responses: Vec<MemberResponse> = members
        .into_iter()
        .map(|m| MemberResponse {
            user_id: m.user_id,
            project_id: m.project_id,
            role: m.role.clone(),
            can_write: m.role == "member",
            added_by: m.added_by,
            created_at: m.created_at.parse().unwrap_or_else(|_| Utc::now()),
            email: m.email,
            display_name: m.display_name,
            avatar_url: m.avatar_url,
        })
        .collect();

    let total = member_responses.len() as u32;

    Ok(Json(ListMembersResponse {
        members: member_responses,
        total,
    }))
}

/// Add a member to a project.
///
/// POST /projects/:project_id/members
#[axum::debug_handler]
async fn add_member(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    axum::Extension(auth): axum::Extension<crate::middleware::AuthContext>,
    Json(request): Json<AddMemberRequest>,
) -> Result<Json<MemberResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Verify user exists
    let _ = crate::db::get_user(&state.db, &request.user_id).await?;

    // Validate role
    if request.role != "member" && request.role != "viewer" {
        return Err(Error::Validation(
            "Role must be 'member' or 'viewer'".into(),
        ));
    }

    let added_by = Some(auth.user_id.as_str());

    let member = crate::db::add_project_member(
        &state.db,
        &project.id,
        &request.user_id,
        &request.role,
        added_by,
    )
    .await?;

    Ok(Json(MemberResponse {
        user_id: member.user_id,
        project_id: member.project_id,
        role: member.role.clone(),
        can_write: member.role == "member",
        added_by: member.added_by,
        created_at: member.created_at.parse().unwrap_or_else(|_| Utc::now()),
        email: None,
        display_name: None,
        avatar_url: None,
    }))
}

/// Get a specific member of a project.
///
/// GET /projects/:project_id/members/:user_id
#[axum::debug_handler]
async fn get_member(
    State(state): State<AppState>,
    Path((project_id, user_id)): Path<(String, String)>,
) -> Result<Json<MemberResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    let member = crate::db::get_project_member(&state.db, &project.id, &user_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Member not found: {}", user_id)))?;

    Ok(Json(MemberResponse {
        user_id: member.user_id,
        project_id: member.project_id,
        role: member.role.clone(),
        can_write: member.role == "member",
        added_by: member.added_by,
        created_at: member.created_at.parse().unwrap_or_else(|_| Utc::now()),
        email: None,
        display_name: None,
        avatar_url: None,
    }))
}

/// Update a member's role.
///
/// PUT /projects/:project_id/members/:user_id
#[axum::debug_handler]
async fn update_member(
    State(state): State<AppState>,
    Path((project_id, user_id)): Path<(String, String)>,
    Json(request): Json<UpdateMemberRequest>,
) -> Result<Json<MemberResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Validate role
    if request.role != "member" && request.role != "viewer" {
        return Err(Error::Validation(
            "Role must be 'member' or 'viewer'".into(),
        ));
    }

    let member =
        crate::db::update_project_member_role(&state.db, &project.id, &user_id, &request.role)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Member not found: {}", user_id)))?;

    Ok(Json(MemberResponse {
        user_id: member.user_id,
        project_id: member.project_id,
        role: member.role.clone(),
        can_write: member.role == "member",
        added_by: member.added_by,
        created_at: member.created_at.parse().unwrap_or_else(|_| Utc::now()),
        email: None,
        display_name: None,
        avatar_url: None,
    }))
}

/// Remove a member from a project.
///
/// DELETE /projects/:project_id/members/:user_id
#[axum::debug_handler]
async fn remove_member(
    State(state): State<AppState>,
    Path((project_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    let removed = crate::db::remove_project_member(&state.db, &project.id, &user_id).await?;

    if !removed {
        return Err(Error::NotFound(format!("Member not found: {}", user_id)));
    }

    Ok(Json(serde_json::json!({
        "removed": true,
        "user_id": user_id,
        "project_id": project.id
    })))
}

// ============================================================================
// Algorithm Configuration Routes
// ============================================================================

/// Build algorithm configuration routes.
///
/// These are mounted under /projects/:project_id/config
pub fn config_routes() -> Router<AppState> {
    Router::new().route(
        "/algorithm",
        get(get_algorithm_config).put(update_algorithm_config),
    )
}

// ============================================================================
// Algorithm Config Request/Response Types
// ============================================================================

/// Algorithm configuration response.
#[derive(Debug, Serialize)]
pub struct AlgorithmConfigResponse {
    /// Weight for retrieval strength vs semantic similarity (0.0-1.0)
    /// 0.0 = pure semantic, 1.0 = pure strength-based
    pub strength_weight: f64,
    /// Half-life in days for memory decay
    /// Shorter values favour very recent memories
    pub decay_half_life_days: f64,
    /// Author patterns to ignore during webhook processing
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_commit_authors: Vec<String>,
}

/// Request to update algorithm configuration.
#[derive(Debug, Deserialize)]
pub struct UpdateAlgorithmConfigRequest {
    /// Weight for retrieval strength vs semantic similarity (0.0-1.0)
    pub strength_weight: Option<f64>,
    /// Half-life in days for memory decay
    pub decay_half_life_days: Option<f64>,
    /// Author patterns to ignore during webhook processing
    pub ignored_commit_authors: Option<Vec<String>>,
}

// ============================================================================
// Algorithm Config Handlers
// ============================================================================

/// Get algorithm configuration for a project.
///
/// GET /projects/:project_id/config/algorithm
///
/// Returns the decay algorithm parameters and ignored commit authors.
#[axum::debug_handler]
async fn get_algorithm_config(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<AlgorithmConfigResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(AlgorithmConfigResponse {
        strength_weight: project.decay_strength_weight.unwrap_or(0.3),
        decay_half_life_days: project.decay_half_life_days.unwrap_or(30.0),
        ignored_commit_authors: ignored_authors,
    }))
}

/// Update algorithm configuration for a project.
///
/// PUT /projects/:project_id/config/algorithm
///
/// Updates the decay algorithm parameters and/or ignored commit authors.
#[axum::debug_handler]
async fn update_algorithm_config(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateAlgorithmConfigRequest>,
) -> Result<Json<AlgorithmConfigResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Validate strength_weight range
    if let Some(weight) = request.strength_weight {
        if !(0.0..=1.0).contains(&weight) {
            return Err(Error::Validation(
                "strength_weight must be between 0.0 and 1.0".into(),
            ));
        }
    }

    // Validate decay_half_life_days
    if let Some(half_life) = request.decay_half_life_days {
        if half_life < 1.0 {
            return Err(Error::Validation(
                "decay_half_life_days must be at least 1.0".into(),
            ));
        }
    }

    // Build update
    let input = crate::db::UpdateAlgorithmConfig {
        decay_strength_weight: request.strength_weight,
        decay_half_life_days: request.decay_half_life_days,
        ignored_commit_authors: request
            .ignored_commit_authors
            .map(|authors| serde_json::to_string(&authors).ok())
            .flatten(),
    };

    let updated = crate::db::update_algorithm_config(&state.db, &project.id, input).await?;

    let ignored_authors = updated
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(AlgorithmConfigResponse {
        strength_weight: updated.decay_strength_weight.unwrap_or(0.3),
        decay_half_life_days: updated.decay_half_life_days.unwrap_or(30.0),
        ignored_commit_authors: ignored_authors,
    }))
}

// ============================================================================
// Reindex and Sync Endpoints
// ============================================================================

/// Response for reindex operation.
#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    pub job_id: Uuid,
    pub status: String,
    pub message: String,
}

/// Response for sync operation.
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub project_id: Uuid,
    pub new_commits: usize,
    pub job_id: Option<Uuid>,
    pub message: String,
}

/// Reindex all files in a project.
///
/// POST /projects/:id/reindex
///
/// Starts a background job to scan and re-index all files in the project's root path.
#[axum::debug_handler]
async fn reindex_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReindexResponse>> {
    // Verify project exists
    let project = crate::db::get_project(&state.db, &id).await?;

    // Create background job for reindexing
    let job_id = crate::models::new_id();
    let job = crate::db::create_job(
        &state.db,
        crate::db::CreateJob::new(job_id.clone(), crate::db::JobType::ReindexRepo)
            .with_project(&project.id),
    )
    .await?;

    info!(
        project_id = %project.id,
        project_slug = %project.slug,
        job_id = %job.id,
        "Queued reindex job for project"
    );

    Ok(Json(ReindexResponse {
        job_id: Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4()),
        status: job.status,
        message: format!("Reindex job queued for project {}", project.slug),
    }))
}

/// Sync project with remote (for github/gitlab providers).
///
/// POST /projects/:id/sync
///
/// For remote providers: fetches new commits and queues indexing.
/// For local providers: triggers a file scan.
#[axum::debug_handler]
async fn sync_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SyncResponse>> {
    let project = crate::db::get_project(&state.db, &id).await?;
    let project_uuid = Uuid::parse_str(&project.id).unwrap_or_else(|_| Uuid::new_v4());

    match project.provider.as_str() {
        "local" => {
            // For local projects, just trigger a reindex (file scan)
            let job_id = crate::models::new_id();
            let job = crate::db::create_job(
                &state.db,
                crate::db::CreateJob::new(job_id.clone(), crate::db::JobType::IndexRepo)
                    .with_project(&project.id),
            )
            .await?;

            info!(
                project_id = %project.id,
                project_slug = %project.slug,
                job_id = %job.id,
                "Queued index job for local project"
            );

            Ok(Json(SyncResponse {
                project_id: project_uuid,
                new_commits: 0,
                job_id: Some(Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4())),
                message: format!("Scan job queued for local project {}", project.slug),
            }))
        }
        "github" | "gitlab" => {
            // For remote providers, sync with the remote
            let owner = project.remote_owner.as_deref().ok_or_else(|| {
                Error::Validation("Remote owner not configured".to_string())
            })?;
            let repo = project.remote_repo.as_deref().ok_or_else(|| {
                Error::Validation("Remote repo not configured".to_string())
            })?;
            let branch = project.remote_branch.as_deref().unwrap_or("main");
            let token = project.access_token.as_deref().unwrap_or("");

            // Fetch commits since last sync
            let since_sha = project.last_commit_sha.clone();
            let commits = state
                .github
                .get_commits(owner, repo, Some(branch), since_sha.as_deref(), 100, token)
                .await
                .map_err(|e| Error::Internal(format!("Failed to fetch commits: {}", e)))?;

            let new_commit_count = commits.len();

            if new_commit_count == 0 {
                return Ok(Json(SyncResponse {
                    project_id: project_uuid,
                    new_commits: 0,
                    job_id: None,
                    message: format!("No new commits in {}", project.slug),
                }));
            }

            // Update last_commit_sha
            if let Some(newest) = commits.first() {
                crate::db::update_project_sync(&state.db, &project.id, Some(&newest.sha), None).await?;
            }

            // Queue index job
            let job_id = crate::models::new_id();
            let job = crate::db::create_job(
                &state.db,
                crate::db::CreateJob::new(job_id.clone(), crate::db::JobType::IndexRepo)
                    .with_project(&project.id),
            )
            .await?;

            info!(
                project_id = %project.id,
                project_slug = %project.slug,
                job_id = %job.id,
                new_commits = new_commit_count,
                "Queued index job for remote project"
            );

            Ok(Json(SyncResponse {
                project_id: project_uuid,
                new_commits: new_commit_count,
                job_id: Some(Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4())),
                message: format!(
                    "Found {} new commits, index job queued for {}",
                    new_commit_count, project.slug
                ),
            }))
        }
        _ => Err(Error::Validation(format!(
            "Unknown provider: {}",
            project.provider
        ))),
    }
}
