//! Repositories Routes
//!
//! Git repository integration for projects.
//!
//! Routes:
//! - GET /projects/:project_id/repositories - List connected repositories
//! - POST /projects/:project_id/repositories - Connect a repository
//! - DELETE /projects/:project_id/repositories/:id - Disconnect repository
//! - POST /projects/:project_id/repositories/:id/reindex - Trigger reindex
//! - GET /projects/:project_id/repositories/:id/commits - List recent commits
//! - GET /projects/:project_id/repositories/:id/pulls - List pull requests

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build repository routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_repositories).post(connect_repository))
        .route("/:repo_id", get(get_repository).delete(disconnect_repository))
        .route("/:repo_id/reindex", post(reindex_repository))
        .route("/:repo_id/commits", get(list_commits))
        .route("/:repo_id/pulls", get(list_pull_requests))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Repository provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryProvider {
    GitHub,
    GitLab,
}

/// Repository status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryStatus {
    Connected,
    Syncing,
    Error,
    Disconnected,
}

/// Request to connect a repository.
#[derive(Debug, Deserialize)]
pub struct ConnectRepositoryRequest {
    /// Repository provider
    pub provider: RepositoryProvider,
    /// Repository owner (user or organization)
    pub owner: String,
    /// Repository name
    pub name: String,
    /// Default branch to track
    pub default_branch: Option<String>,
    /// Whether to automatically index on changes
    #[serde(default = "default_auto_index")]
    pub auto_index: bool,
}

fn default_auto_index() -> bool {
    true
}

/// Query parameters for listing commits.
#[derive(Debug, Deserialize, Default)]
pub struct ListCommitsQuery {
    /// Branch to list commits from
    pub branch: Option<String>,
    /// Pagination
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_per_page() -> u32 {
    30
}

/// Query parameters for listing pull requests.
#[derive(Debug, Deserialize, Default)]
pub struct ListPullsQuery {
    /// Filter by state
    #[serde(default)]
    pub state: PullRequestState,
    /// Pagination
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestState {
    #[default]
    Open,
    Closed,
    Merged,
    All,
}

/// Repository response.
#[derive(Debug, Serialize)]
pub struct RepositoryResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub provider: RepositoryProvider,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub default_branch: String,
    pub status: RepositoryStatus,
    pub auto_index: bool,
    pub last_indexed_at: Option<DateTime<Utc>>,
    pub webhook_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List repositories response.
#[derive(Debug, Serialize)]
pub struct ListRepositoriesResponse {
    pub repositories: Vec<RepositoryResponse>,
    pub total: u32,
}

/// Commit information.
#[derive(Debug, Serialize)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub committed_at: DateTime<Utc>,
    pub url: String,
}

/// List commits response.
#[derive(Debug, Serialize)]
pub struct ListCommitsResponse {
    pub commits: Vec<CommitInfo>,
    pub page: u32,
    pub per_page: u32,
    pub has_more: bool,
}

/// Pull request information.
#[derive(Debug, Serialize)]
pub struct PullRequestInfo {
    pub number: u32,
    pub title: String,
    pub state: PullRequestState,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub merged_at: Option<DateTime<Utc>>,
    pub url: String,
}

/// List pull requests response.
#[derive(Debug, Serialize)]
pub struct ListPullsResponse {
    pub pull_requests: Vec<PullRequestInfo>,
    pub page: u32,
    pub per_page: u32,
    pub has_more: bool,
}

/// Reindex response.
#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    pub job_id: Uuid,
    pub status: String,
    pub message: String,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RepositoryPath {
    pub project_id: String,
    pub repo_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// List connected repositories for a project.
///
/// GET /projects/:project_id/repositories
#[axum::debug_handler]
async fn list_repositories(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
) -> Result<Json<ListRepositoriesResponse>> {
    // TODO: Fetch repositories from database

    Ok(Json(ListRepositoriesResponse {
        repositories: vec![],
        total: 0,
    }))
}

/// Connect a repository to the project.
///
/// POST /projects/:project_id/repositories
///
/// Connects a GitHub or GitLab repository and optionally sets up
/// webhooks for automatic indexing on changes.
#[axum::debug_handler]
async fn connect_repository(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ConnectRepositoryRequest>,
) -> Result<Json<RepositoryResponse>> {
    let _project_id = &path.project_id;

    // Validate owner and name
    if request.owner.is_empty() || request.name.is_empty() {
        return Err(Error::Validation("Owner and name are required".into()));
    }

    // TODO: Verify repository exists and we have access
    // let repo_info = match request.provider {
    //     RepositoryProvider::GitHub => {
    //         state.github.get_repository(&request.owner, &request.name).await?
    //     }
    //     RepositoryProvider::GitLab => {
    //         state.gitlab.get_repository(&request.owner, &request.name).await?
    //     }
    // };

    // TODO: Create repository record in database
    // TODO: Set up webhook if auto_index is enabled

    let now = Utc::now();
    Ok(Json(RepositoryResponse {
        id: Uuid::new_v4(),
        project_id: Uuid::new_v4(), // TODO: Parse from path
        provider: request.provider,
        owner: request.owner.clone(),
        name: request.name.clone(),
        full_name: format!("{}/{}", request.owner, request.name),
        default_branch: request
            .default_branch
            .unwrap_or_else(|| "main".to_string()),
        status: RepositoryStatus::Connected,
        auto_index: request.auto_index,
        last_indexed_at: None,
        webhook_id: None,
        created_at: now,
        updated_at: now,
    }))
}

/// Get a repository by ID.
///
/// GET /projects/:project_id/repositories/:repo_id
#[axum::debug_handler]
async fn get_repository(
    State(_state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<RepositoryResponse>> {
    // TODO: Fetch repository from database

    Err(Error::NotFound(format!("Repository: {}", path.repo_id)))
}

/// Disconnect a repository from the project.
///
/// DELETE /projects/:project_id/repositories/:repo_id
///
/// Removes the repository connection and deletes any webhooks.
/// Does not delete indexed memories.
#[axum::debug_handler]
async fn disconnect_repository(
    State(_state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Fetch repository from database
    // TODO: Delete webhook if exists
    // TODO: Mark repository as disconnected or delete

    Err(Error::NotFound(format!("Repository: {}", path.repo_id)))
}

/// Trigger a reindex of the repository.
///
/// POST /projects/:project_id/repositories/:repo_id/reindex
///
/// Starts a background job to re-index all files in the repository.
#[axum::debug_handler]
async fn reindex_repository(
    State(_state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<ReindexResponse>> {
    // TODO: Verify repository exists
    // TODO: Create background job for reindexing

    let job_id = Uuid::new_v4();

    Ok(Json(ReindexResponse {
        job_id,
        status: "queued".into(),
        message: format!("Reindex job {} queued for repository {}", job_id, path.repo_id),
    }))
}

/// List recent commits from the repository.
///
/// GET /projects/:project_id/repositories/:repo_id/commits
#[axum::debug_handler]
async fn list_commits(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
    Query(query): Query<ListCommitsQuery>,
) -> Result<Json<ListCommitsResponse>> {
    // TODO: Fetch repository from database to get provider info
    // For now, assume GitHub

    // TODO: Implement list_commits via GitHub service
    // let _commits = state.github.list_commits(...).await?;

    Ok(Json(ListCommitsResponse {
        commits: vec![],
        page: query.page,
        per_page: query.per_page,
        has_more: false,
    }))
}

/// List pull requests from the repository.
///
/// GET /projects/:project_id/repositories/:repo_id/pulls
#[axum::debug_handler]
async fn list_pull_requests(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
    Query(query): Query<ListPullsQuery>,
) -> Result<Json<ListPullsResponse>> {
    // TODO: Fetch repository from database to get provider info
    // For now, assume GitHub

    // TODO: Implement list_pull_requests via GitHub service
    // let _pulls = state.github.list_pull_requests(...).await?;

    Ok(Json(ListPullsResponse {
        pull_requests: vec![],
        page: query.page,
        per_page: query.per_page,
        has_more: false,
    }))
}
