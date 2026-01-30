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
use tracing::{info, warn};
use uuid::Uuid;

use crate::{config, db, AppState, Error, Result};

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
    /// Access token for the repository (GitHub PAT, GitLab token, etc.)
    pub access_token: Option<String>,
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
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
) -> Result<Json<ListRepositoriesResponse>> {
    // Get project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Fetch repositories from database
    let repos = db::list_project_repositories(&state.db, &project.id).await?;

    let repositories: Vec<RepositoryResponse> = repos
        .into_iter()
        .map(|r| RepositoryResponse {
            id: Uuid::parse_str(&r.id).unwrap_or_else(|_| Uuid::new_v4()),
            project_id: Uuid::parse_str(&r.project_id).unwrap_or_else(|_| Uuid::new_v4()),
            provider: match r.provider.as_str() {
                "gitlab" => RepositoryProvider::GitLab,
                _ => RepositoryProvider::GitHub,
            },
            owner: r.owner.clone(),
            name: r.repo.clone(),
            full_name: r.full_name(),
            default_branch: r.branch,
            status: if r.last_indexed_at.is_some() {
                RepositoryStatus::Connected
            } else {
                RepositoryStatus::Syncing
            },
            auto_index: r.webhook_id.is_some(),
            last_indexed_at: r.last_indexed_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
            webhook_id: r.webhook_id,
            created_at: DateTime::parse_from_rfc3339(&r.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&r.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
        .collect();

    let total = repositories.len() as u32;

    Ok(Json(ListRepositoriesResponse { repositories, total }))
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
    // Validate owner and name
    if request.owner.is_empty() || request.name.is_empty() {
        return Err(Error::Validation("Owner and name are required".into()));
    }

    // Get project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Convert provider
    let db_provider = match request.provider {
        RepositoryProvider::GitHub => db::GitProvider::GitHub,
        RepositoryProvider::GitLab => db::GitProvider::GitLab,
    };

    let branch = request.default_branch.unwrap_or_else(|| "main".to_string());

    // Check if repository already connected
    if let Some(_existing) = db::get_repository_by_path(
        &state.db,
        &project.id,
        &db_provider,
        &request.owner,
        &request.name,
        &branch,
    )
    .await?
    {
        return Err(Error::AlreadyExists(format!(
            "Repository {}/{} branch {} already connected",
            request.owner, request.name, branch
        )));
    }

    // Create repository record in database
    let repo_id = crate::models::new_id();
    let access_token = request.access_token.unwrap_or_default();

    let mut repo = db::create_repository(
        &state.db,
        db::CreateRepository {
            id: repo_id.clone(),
            project_id: project.id.clone(),
            provider: db_provider.clone(),
            owner: request.owner.clone(),
            repo: request.name.clone(),
            branch: branch.clone(),
            access_token: access_token.clone(),
        },
    )
    .await?;

    // Set up webhook if auto_index is enabled (GitHub only for now)
    let mut webhook_id: Option<String> = None;
    if request.auto_index && matches!(request.provider, RepositoryProvider::GitHub) {
        // Generate webhook secret
        let webhook_secret = nanoid::nanoid!(32);

        // Build webhook URL
        let config = config::config();
        let webhook_url = format!(
            "{}/webhooks/github/{}",
            config.server.public_url.trim_end_matches('/'),
            repo_id
        );

        // Register webhook with GitHub
        match state.github.register_webhook(
            &request.owner,
            &request.name,
            &webhook_url,
            &webhook_secret,
            vec!["push".to_string(), "pull_request".to_string()],
            &access_token,
        ).await {
            Ok(webhook) => {
                info!(
                    repo = %repo.full_name(),
                    webhook_id = webhook.id,
                    "Registered GitHub webhook"
                );

                // Update repository with webhook info
                repo = db::update_repository(
                    &state.db,
                    &repo_id,
                    db::UpdateRepository {
                        webhook_id: Some(webhook.id.to_string()),
                        webhook_secret: Some(webhook_secret),
                        ..Default::default()
                    },
                ).await?;

                webhook_id = Some(webhook.id.to_string());
            }
            Err(e) => {
                // Log warning but don't fail the connection
                warn!(
                    repo = %repo.full_name(),
                    error = %e,
                    "Failed to register GitHub webhook - repository connected without auto-indexing"
                );
            }
        }
    }

    let now = Utc::now();
    let full_name = repo.full_name();
    Ok(Json(RepositoryResponse {
        id: Uuid::parse_str(&repo.id).unwrap_or_else(|_| Uuid::new_v4()),
        project_id: Uuid::parse_str(&project.id).unwrap_or_else(|_| Uuid::new_v4()),
        provider: request.provider,
        owner: repo.owner,
        name: repo.repo,
        full_name,
        default_branch: repo.branch,
        status: RepositoryStatus::Connected,
        auto_index: webhook_id.is_some(),
        last_indexed_at: None,
        webhook_id,
        created_at: now,
        updated_at: now,
    }))
}

/// Get a repository by ID.
///
/// GET /projects/:project_id/repositories/:repo_id
#[axum::debug_handler]
async fn get_repository(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<RepositoryResponse>> {
    let repo = db::get_repository(&state.db, &path.repo_id.to_string()).await?;

    Ok(Json(RepositoryResponse {
        id: path.repo_id,
        project_id: Uuid::parse_str(&repo.project_id).unwrap_or_else(|_| Uuid::new_v4()),
        provider: match repo.provider.as_str() {
            "gitlab" => RepositoryProvider::GitLab,
            _ => RepositoryProvider::GitHub,
        },
        owner: repo.owner.clone(),
        name: repo.repo.clone(),
        full_name: repo.full_name(),
        default_branch: repo.branch,
        status: if repo.last_indexed_at.is_some() {
            RepositoryStatus::Connected
        } else {
            RepositoryStatus::Syncing
        },
        auto_index: repo.webhook_id.is_some(),
        last_indexed_at: repo.last_indexed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }),
        webhook_id: repo.webhook_id,
        created_at: DateTime::parse_from_rfc3339(&repo.created_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&repo.created_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }))
}

/// Disconnect a repository from the project.
///
/// DELETE /projects/:project_id/repositories/:repo_id
///
/// Removes the repository connection and deletes any webhooks.
/// Does not delete indexed memories.
#[axum::debug_handler]
async fn disconnect_repository(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<serde_json::Value>> {
    let repo_id = path.repo_id.to_string();

    // Get repository to check it exists
    let repo = db::get_repository(&state.db, &repo_id).await?;

    // Delete webhook if exists (GitHub only for now)
    if let Some(webhook_id) = &repo.webhook_id {
        match repo.provider.as_str() {
            "github" => {
                // Parse webhook ID as i64
                if let Ok(wh_id) = webhook_id.parse::<i64>() {
                    match state.github.delete_webhook(
                        &repo.owner,
                        &repo.repo,
                        wh_id,
                        &repo.access_token,
                    ).await {
                        Ok(_) => {
                            info!(
                                repo = %repo.full_name(),
                                webhook_id = %webhook_id,
                                "Deleted GitHub webhook"
                            );
                        }
                        Err(e) => {
                            // Log warning but don't fail the disconnection
                            warn!(
                                repo = %repo.full_name(),
                                error = %e,
                                "Failed to delete GitHub webhook - proceeding with disconnect"
                            );
                        }
                    }
                }
            }
            "gitlab" => {
                // GitLab webhook deletion not implemented yet
                warn!(
                    repo = %repo.full_name(),
                    "GitLab webhook deletion not implemented - proceeding with disconnect"
                );
            }
            _ => {}
        }
    }

    // Delete the repository record
    db::delete_repository(&state.db, &repo_id).await?;

    Ok(Json(serde_json::json!({
        "message": "Repository disconnected",
        "id": repo_id,
        "full_name": repo.full_name()
    })))
}

/// Trigger a reindex of the repository.
///
/// POST /projects/:project_id/repositories/:repo_id/reindex
///
/// Starts a background job to re-index all files in the repository.
#[axum::debug_handler]
async fn reindex_repository(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<ReindexResponse>> {
    let repo_id = path.repo_id.to_string();

    // Verify repository exists
    let repo = db::get_repository(&state.db, &repo_id).await?;
    let full_name = repo.full_name();

    // Create background job for reindexing
    let job_id = crate::models::new_id();
    let job = db::create_job(
        &state.db,
        db::CreateJob {
            id: job_id.clone(),
            job_type: db::JobType::ReindexRepo,
            project_id: Some(repo.project_id),
            repository_id: Some(repo_id.clone()),
            total_items: None,
        },
    )
    .await?;

    Ok(Json(ReindexResponse {
        job_id: Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4()),
        status: job.status,
        message: format!("Reindex job queued for repository {}", full_name),
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
    let repo_id = path.repo_id.to_string();
    let repo = db::get_repository(&state.db, &repo_id).await?;

    let page = query.page.max(1);
    let per_page = query.per_page.min(100);
    let offset = ((page - 1) * per_page) as i64;

    // Fetch commits from database (already indexed)
    let db_commits = db::list_repository_commits(&state.db, &repo_id, per_page as i64 + 1, offset).await?;

    let has_more = db_commits.len() > per_page as usize;
    let commits: Vec<CommitInfo> = db_commits
        .into_iter()
        .take(per_page as usize)
        .map(|c| {
            let url = match repo.provider.as_str() {
                "gitlab" => format!("https://gitlab.com/{}/{}/-/commit/{}", repo.owner, repo.repo, c.sha),
                _ => format!("https://github.com/{}/{}/commit/{}", repo.owner, repo.repo, c.sha),
            };
            CommitInfo {
                sha: c.sha,
                message: c.message,
                author_name: c.author_name.unwrap_or_default(),
                author_email: c.author_email.unwrap_or_default(),
                committed_at: DateTime::parse_from_rfc3339(&c.committed_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                url,
            }
        })
        .collect();

    Ok(Json(ListCommitsResponse {
        commits,
        page,
        per_page,
        has_more,
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
    let repo_id = path.repo_id.to_string();
    let repo = db::get_repository(&state.db, &repo_id).await?;

    let page = query.page.max(1);
    let per_page = query.per_page.min(100);
    let offset = ((page - 1) * per_page) as i64;

    // Fetch PRs from database (already indexed)
    let db_prs = db::list_repository_pull_requests(&state.db, &repo_id, per_page as i64 + 1, offset).await?;

    let has_more = db_prs.len() > per_page as usize;
    let pull_requests: Vec<PullRequestInfo> = db_prs
        .into_iter()
        .take(per_page as usize)
        .filter(|pr| match query.state {
            PullRequestState::All => true,
            PullRequestState::Open => pr.state == "open",
            PullRequestState::Closed => pr.state == "closed",
            PullRequestState::Merged => pr.state == "merged",
        })
        .map(|pr| {
            let url = match repo.provider.as_str() {
                "gitlab" => format!("https://gitlab.com/{}/{}/-/merge_requests/{}", repo.owner, repo.repo, pr.number),
                _ => format!("https://github.com/{}/{}/pull/{}", repo.owner, repo.repo, pr.number),
            };
            PullRequestInfo {
                number: pr.number as u32,
                title: pr.title,
                state: match pr.state.as_str() {
                    "closed" => PullRequestState::Closed,
                    "merged" => PullRequestState::Merged,
                    _ => PullRequestState::Open,
                },
                author: pr.author.unwrap_or_default(),
                head_branch: pr.source_branch.unwrap_or_default(),
                base_branch: pr.target_branch.unwrap_or_default(),
                created_at: DateTime::parse_from_rfc3339(&pr.created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&pr.indexed_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                merged_at: pr.merged_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
                url,
            }
        })
        .collect();

    Ok(Json(ListPullsResponse {
        pull_requests,
        page,
        per_page,
        has_more,
    }))
}
