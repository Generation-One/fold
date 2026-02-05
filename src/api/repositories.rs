//! Repositories Routes
//!
//! File source integration for projects.
//!
//! Supports multiple file source providers (GitHub, Google Drive, etc.)
//! through a unified interface.
//!
//! Routes:
//! - GET /projects/:project_id/repositories - List connected file sources
//! - POST /projects/:project_id/repositories - Connect a file source
//! - DELETE /projects/:project_id/repositories/:id - Disconnect file source
//! - POST /projects/:project_id/repositories/:id/reindex - Trigger reindex
//! - POST /projects/:project_id/repositories/:id/sync - Sync commits from GitHub
//! - POST /projects/:project_id/repositories/:id/sync-fold - Sync memories from fold/ directory
//! - GET /projects/:project_id/repositories/:id/commits - List recent commits (git only)
//! - GET /projects/:project_id/repositories/:id/pulls - List pull requests (git only)
//! - GET /file-sources/providers - List available file source providers

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

/// Build repository routes (under /projects/:project_id/repositories).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_repositories).post(connect_repository))
        .route("/:repo_id", get(get_repository).delete(disconnect_repository).patch(update_repository))
        .route("/:repo_id/reindex", post(reindex_repository))
        .route("/:repo_id/sync", post(sync_repository))
        .route("/:repo_id/sync-fold", post(sync_fold))
        .route("/:repo_id/commits", get(list_commits))
        .route("/:repo_id/pulls", get(list_pull_requests))
}

/// Build file source provider routes (under /file-sources).
pub fn file_source_routes() -> Router<AppState> {
    Router::new()
        .route("/providers", get(list_providers))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// File source provider type.
///
/// Represents the different types of file sources that can be connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepositoryProvider {
    /// GitHub repositories
    GitHub,
    /// GitLab repositories
    GitLab,
    /// Google Drive folders (future)
    GoogleDrive,
    /// OneDrive folders (future)
    OneDrive,
    /// Local filesystem (future)
    Local,
}

impl RepositoryProvider {
    /// Get the provider type string for the file source abstraction.
    pub fn as_source_type(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::GoogleDrive => "google-drive",
            Self::OneDrive => "onedrive",
            Self::Local => "local",
        }
    }

    /// Parse from a source type string.
    pub fn from_source_type(s: &str) -> Option<Self> {
        match s {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            "google-drive" => Some(Self::GoogleDrive),
            "onedrive" => Some(Self::OneDrive),
            "local" => Some(Self::Local),
            _ => None,
        }
    }
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
///
/// Can use either:
/// - `url`: A full repository URL (e.g., "https://github.com/owner/repo")
/// - `provider` + `owner` + `name`: Explicit fields
/// - `provider: local` + `local_path`: For local filesystem repositories
///
/// If `url` is provided, it takes precedence and the provider/owner/name
/// are parsed from it automatically.
#[derive(Debug, Deserialize)]
pub struct ConnectRepositoryRequest {
    /// Repository URL (e.g., "https://github.com/owner/repo")
    /// If provided, provider/owner/name are parsed from it.
    pub url: Option<String>,
    /// Repository provider (required if url not provided)
    pub provider: Option<RepositoryProvider>,
    /// Repository owner (required if url not provided)
    pub owner: Option<String>,
    /// Repository name (required if url not provided)
    pub name: Option<String>,
    /// Default branch to track
    pub default_branch: Option<String>,
    /// Whether to automatically index on changes
    #[serde(default = "default_auto_index")]
    pub auto_index: bool,
    /// Access token for the repository (GitHub PAT, GitLab token, etc.)
    pub access_token: Option<String>,
    /// Local filesystem path (required for local provider)
    pub local_path: Option<String>,
}

/// Parsed repository information from a URL or explicit fields.
#[derive(Debug)]
struct ParsedRepository {
    provider: RepositoryProvider,
    owner: String,
    name: String,
}

impl ConnectRepositoryRequest {
    /// Parse the repository info from either URL or explicit fields.
    fn parse(&self) -> std::result::Result<ParsedRepository, String> {
        // If URL is provided, parse it
        if let Some(url) = &self.url {
            return parse_repository_url(url);
        }

        // Otherwise, require explicit fields
        let provider = self.provider.ok_or("provider is required when url is not provided")?;

        // For Local provider, derive owner/name from local_path if not provided
        if matches!(provider, RepositoryProvider::Local) {
            let local_path = self.local_path.as_ref()
                .ok_or("local_path is required for local provider")?;

            // Derive name from the last path component if not provided
            let name = self.name.clone().unwrap_or_else(|| {
                std::path::Path::new(local_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "local-repo".to_string())
            });

            // Use "local" as owner if not provided
            let owner = self.owner.clone().unwrap_or_else(|| "local".to_string());

            return Ok(ParsedRepository { provider, owner, name });
        }

        let owner = self.owner.clone().ok_or("owner is required when url is not provided")?;
        let name = self.name.clone().ok_or("name is required when url is not provided")?;

        if owner.is_empty() {
            return Err("owner cannot be empty".into());
        }
        if name.is_empty() {
            return Err("name cannot be empty".into());
        }

        Ok(ParsedRepository { provider, owner, name })
    }
}

/// Parse a repository URL into provider, owner, and name.
///
/// Supports formats:
/// - https://github.com/owner/repo
/// - https://github.com/owner/repo.git
/// - git@github.com:owner/repo.git
/// - https://gitlab.com/owner/repo
/// - https://gitlab.com/group/subgroup/repo
fn parse_repository_url(url: &str) -> std::result::Result<ParsedRepository, String> {
    let url = url.trim();

    // Handle SSH format: git@github.com:owner/repo.git
    if url.starts_with("git@") {
        return parse_ssh_url(url);
    }

    // Handle HTTPS format
    let url = url
        .trim_end_matches('/')
        .trim_end_matches(".git");

    // Parse as URL
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL: {}", e))?;

    let host = parsed.host_str()
        .ok_or("URL must have a host")?;

    // Determine provider from host
    let provider = if host.contains("github") {
        RepositoryProvider::GitHub
    } else if host.contains("gitlab") {
        RepositoryProvider::GitLab
    } else {
        return Err(format!("Unsupported git host: {}. Supported: github.com, gitlab.com", host));
    };

    // Parse path segments
    let path = parsed.path().trim_start_matches('/');
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.len() < 2 {
        return Err("URL must contain owner and repository name".into());
    }

    // For GitHub: owner/repo
    // For GitLab: can be group/subgroup/repo, we take first as owner, last as repo
    let owner = segments[0].to_string();
    let name = segments.last().unwrap().trim_end_matches(".git").to_string();

    Ok(ParsedRepository { provider, owner, name })
}

/// Parse SSH-style git URL: git@github.com:owner/repo.git
fn parse_ssh_url(url: &str) -> std::result::Result<ParsedRepository, String> {
    // Format: git@host:path
    let without_prefix = url.strip_prefix("git@")
        .ok_or("Invalid SSH URL format")?;

    let (host, path) = without_prefix.split_once(':')
        .ok_or("Invalid SSH URL format: missing ':'")?;

    let provider = if host.contains("github") {
        RepositoryProvider::GitHub
    } else if host.contains("gitlab") {
        RepositoryProvider::GitLab
    } else {
        return Err(format!("Unsupported git host: {}. Supported: github.com, gitlab.com", host));
    };

    let path = path.trim_end_matches(".git");
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.len() < 2 {
        return Err("SSH URL must contain owner and repository name".into());
    }

    let owner = segments[0].to_string();
    let name = segments.last().unwrap().to_string();

    Ok(ParsedRepository { provider, owner, name })
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
    /// Whether polling is enabled for this repository.
    pub polling_enabled: bool,
    /// Polling interval in seconds (if polling is enabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_interval_secs: Option<u32>,
    /// Local filesystem path where the repository is cloned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// HEAD commit SHA of the local clone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    pub last_indexed_at: Option<DateTime<Utc>>,
    /// Last time the repository was polled for changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_polled_at: Option<DateTime<Utc>>,
    pub webhook_id: Option<String>,
    /// Error message if the repository is in an error state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
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

/// File source provider information.
#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    /// Provider type identifier.
    pub provider_type: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Whether real-time webhooks are supported.
    pub supports_webhooks: bool,
    /// Whether polling is required for change detection.
    pub requires_polling: bool,
    /// Whether this provider is currently available.
    pub available: bool,
}

/// List providers response.
#[derive(Debug, Serialize)]
pub struct ListProvidersResponse {
    pub providers: Vec<ProviderInfo>,
}

/// Reindex response.
#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    pub job_id: Uuid,
    pub status: String,
    pub message: String,
}

/// Sync response - result of checking for new changes.
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub repository_id: Uuid,
    pub new_commits: usize,
    pub job_id: Option<Uuid>,
    pub message: String,
}

/// Response from syncing the fold/ directory.
#[derive(Debug, Serialize)]
pub struct SyncFoldResponse {
    pub project_id: Uuid,
    pub imported: usize,
    pub existing: usize,
    pub errors: usize,
    pub message: String,
}

/// Request to update a repository.
#[derive(Debug, Deserialize)]
pub struct UpdateRepositoryRequest {
    /// Update the access token
    pub access_token: Option<String>,
    /// Update the default branch
    pub default_branch: Option<String>,
    /// Enable/disable polling mode (check for changes periodically)
    pub polling_enabled: Option<bool>,
    /// Polling interval in seconds (default: 300 = 5 minutes)
    pub polling_interval_secs: Option<u32>,
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
        .map(|r| {
            let polling_enabled = r.notification_type.as_deref() == Some("polling");
            RepositoryResponse {
                id: Uuid::parse_str(&r.id).unwrap_or_else(|_| Uuid::new_v4()),
                project_id: Uuid::parse_str(&r.project_id).unwrap_or_else(|_| Uuid::new_v4()),
                provider: match r.provider.as_str() {
                    "gitlab" => RepositoryProvider::GitLab,
                    "local" => RepositoryProvider::Local,
                    _ => RepositoryProvider::GitHub,
                },
                owner: r.owner.clone(),
                name: r.repo.clone(),
                full_name: r.full_name(),
                default_branch: r.branch.clone(),
                status: if r.last_indexed_at.is_some() {
                    RepositoryStatus::Connected
                } else {
                    RepositoryStatus::Syncing
                },
                auto_index: r.webhook_id.is_some() || polling_enabled,
                polling_enabled,
                polling_interval_secs: if polling_enabled {
                    r.source_config.as_ref().and_then(|cfg| {
                        serde_json::from_str::<serde_json::Value>(cfg)
                            .ok()
                            .and_then(|v| v.get("polling_interval_secs")?.as_u64())
                            .map(|v| v as u32)
                    })
                } else {
                    None
                },
                local_path: r.local_path.clone(),
                head_sha: r.last_commit_sha.clone(),
                last_indexed_at: r.last_indexed_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                last_polled_at: r.last_sync.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                webhook_id: r.webhook_id,
                error_message: None, // TODO: Add error tracking to Repository model
                created_at: DateTime::parse_from_rfc3339(&r.created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&r.created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }
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
///
/// Request body can use either:
/// - `url`: A full repository URL (e.g., "https://github.com/owner/repo")
/// - `provider` + `owner` + `name`: Explicit fields
#[axum::debug_handler]
async fn connect_repository(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ConnectRepositoryRequest>,
) -> Result<Json<RepositoryResponse>> {
    // Parse repository info from URL or explicit fields
    let parsed = request.parse()
        .map_err(|e| Error::Validation(e))?;

    // Get project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Convert provider
    let db_provider = match parsed.provider {
        RepositoryProvider::GitHub => db::GitProvider::GitHub,
        RepositoryProvider::GitLab => db::GitProvider::GitLab,
        RepositoryProvider::Local => db::GitProvider::Local,
        RepositoryProvider::GoogleDrive | RepositoryProvider::OneDrive => {
            return Err(Error::Validation(format!(
                "Provider '{}' is not yet supported. Currently supported: github, gitlab, local",
                parsed.provider.as_source_type()
            )));
        }
    };

    let branch = request.default_branch.unwrap_or_else(|| "main".to_string());

    // Check if repository already connected
    if let Some(_existing) = db::get_repository_by_path(
        &state.db,
        &project.id,
        &db_provider,
        &parsed.owner,
        &parsed.name,
        &branch,
    )
    .await?
    {
        return Err(Error::AlreadyExists(format!(
            "Repository {}/{} branch {} already connected",
            parsed.owner, parsed.name, branch
        )));
    }

    // Create repository record in database
    let repo_id = crate::models::new_id();
    let access_token = request.access_token.unwrap_or_default();

    // For Local provider, use the provided local_path directly
    let initial_local_path = if matches!(parsed.provider, RepositoryProvider::Local) {
        request.local_path.clone()
    } else {
        None
    };

    let mut repo = db::create_repository(
        &state.db,
        db::CreateRepository {
            id: repo_id.clone(),
            project_id: project.id.clone(),
            provider: db_provider.clone(),
            owner: parsed.owner.clone(),
            repo: parsed.name.clone(),
            branch: branch.clone(),
            access_token: access_token.clone(),
            local_path: initial_local_path.clone(),
        },
    )
    .await?;

    // For non-local providers, clone repository locally for efficient indexing
    let final_local_path = if matches!(parsed.provider, RepositoryProvider::Local) {
        // Local provider already has local_path set
        info!(
            repo = %repo.full_name(),
            path = ?initial_local_path,
            "Connected local repository"
        );
        initial_local_path
    } else {
        // Clone from remote
        match state.git_local.clone_repo(
            &project.slug,
            &parsed.owner,
            &parsed.name,
            &branch,
            &access_token,
            db_provider.as_str(),
        ).await {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_string();
                info!(
                    repo = %repo.full_name(),
                    path = %path_str,
                    "Cloned repository locally"
                );
                // Update repository with local path
                repo = db::update_repository(
                    &state.db,
                    &repo_id,
                    db::UpdateRepository {
                        local_path: Some(path_str.clone()),
                        ..Default::default()
                    },
                ).await?;
                Some(path_str)
            }
            Err(e) => {
                // Log warning but don't fail the connection - can still use API
                warn!(
                    repo = %repo.full_name(),
                    error = %e,
                    "Failed to clone repository locally - will use API for indexing"
                );
                None
            }
        }
    };

    // Set up webhook if auto_index is enabled (GitHub only for now)
    let mut webhook_id: Option<String> = None;
    if request.auto_index && matches!(parsed.provider, RepositoryProvider::GitHub) {
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
            &parsed.owner,
            &parsed.name,
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
        provider: parsed.provider,
        owner: repo.owner,
        name: repo.repo,
        full_name,
        default_branch: repo.branch,
        status: RepositoryStatus::Connected,
        auto_index: webhook_id.is_some(),
        polling_enabled: false,
        polling_interval_secs: None,
        local_path: final_local_path,
        head_sha: None,
        last_indexed_at: None,
        last_polled_at: None,
        webhook_id,
        error_message: None,
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

    let polling_enabled = repo.notification_type.as_deref() == Some("polling");
    Ok(Json(RepositoryResponse {
        id: path.repo_id,
        project_id: Uuid::parse_str(&repo.project_id).unwrap_or_else(|_| Uuid::new_v4()),
        provider: match repo.provider.as_str() {
            "gitlab" => RepositoryProvider::GitLab,
            "local" => RepositoryProvider::Local,
            _ => RepositoryProvider::GitHub,
        },
        owner: repo.owner.clone(),
        name: repo.repo.clone(),
        full_name: repo.full_name(),
        default_branch: repo.branch.clone(),
        status: if repo.last_indexed_at.is_some() {
            RepositoryStatus::Connected
        } else {
            RepositoryStatus::Syncing
        },
        auto_index: repo.webhook_id.is_some() || polling_enabled,
        polling_enabled,
        polling_interval_secs: if polling_enabled {
            repo.source_config.as_ref().and_then(|cfg| {
                serde_json::from_str::<serde_json::Value>(cfg)
                    .ok()
                    .and_then(|v| v.get("polling_interval_secs")?.as_u64())
                    .map(|v| v as u32)
            })
        } else {
            None
        },
        local_path: repo.local_path.clone(),
        head_sha: repo.last_commit_sha.clone(),
        last_indexed_at: repo.last_indexed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }),
        last_polled_at: repo.last_sync.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }),
        webhook_id: repo.webhook_id,
        error_message: None,
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
        db::CreateJob::new(job_id.clone(), db::JobType::ReindexRepo)
            .with_project(repo.project_id)
            .with_repository(repo_id.clone()),
    )
    .await?;

    Ok(Json(ReindexResponse {
        job_id: Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4()),
        status: job.status,
        message: format!("Reindex job queued for repository {}", full_name),
    }))
}

/// Sync repository - check for new commits and index them.
///
/// POST /projects/:project_id/repositories/:repo_id/sync
///
/// Fetches new commits from GitHub since the last sync and queues
/// indexing jobs for any new content.
#[axum::debug_handler]
async fn sync_repository(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<SyncResponse>> {
    let repo_id = path.repo_id.to_string();

    // Get repository
    let repo = db::get_repository(&state.db, &repo_id).await?;
    let full_name = repo.full_name();

    // Fetch commits from GitHub since last sync
    let since_sha = repo.last_commit_sha.clone();
    let commits = state
        .github
        .get_commits(&repo.owner, &repo.repo, Some(&repo.branch), since_sha.as_deref(), 100, &repo.access_token)
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch commits: {}", e)))?;

    let new_commit_count = commits.len();

    if new_commit_count == 0 {
        return Ok(Json(SyncResponse {
            repository_id: path.repo_id,
            new_commits: 0,
            job_id: None,
            message: format!("No new commits in {}", full_name),
        }));
    }

    // Update last_commit_sha to the newest commit
    if let Some(newest) = commits.first() {
        db::update_repository(
            &state.db,
            &repo_id,
            db::UpdateRepository {
                last_commit_sha: Some(newest.sha.clone()),
                last_sync: Some(Utc::now().to_rfc3339()),
                ..Default::default()
            },
        )
        .await?;
    }

    // Create a sync job to process the commits
    let job_id = crate::models::new_id();
    let payload = serde_json::json!({
        "commits": commits.iter().map(|c| &c.sha).collect::<Vec<_>>(),
    });

    let job = db::create_job(
        &state.db,
        db::CreateJob::new(job_id.clone(), db::JobType::SyncMetadata)
            .with_project(repo.project_id)
            .with_repository(repo_id)
            .with_payload(payload),
    )
    .await?;

    info!(
        repo = %full_name,
        new_commits = new_commit_count,
        job_id = %job.id,
        "Sync found new commits"
    );

    Ok(Json(SyncResponse {
        repository_id: path.repo_id,
        new_commits: new_commit_count,
        job_id: Some(Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4())),
        message: format!("Found {} new commits in {}, queued for indexing", new_commit_count, full_name),
    }))
}

/// Sync memories from the fold/ directory.
///
/// POST /projects/:project_id/repositories/:repo_id/sync-fold
///
/// Pulls the latest changes from the remote repository and imports
/// any new memory files from the fold/ directory that don't exist
/// in the local database. This is useful when fold/ files are added
/// by other team members or from another machine.
#[axum::debug_handler]
async fn sync_fold(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
) -> Result<Json<SyncFoldResponse>> {
    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Use git service to sync from remote
    let stats = state
        .git_service
        .sync_from_remote(&project)
        .await?;

    let project_id = Uuid::parse_str(&project.id).unwrap_or_else(|_| Uuid::new_v4());

    info!(
        project = %project.slug,
        imported = stats.imported,
        existing = stats.existing,
        errors = stats.errors,
        "Synced fold/ from remote"
    );

    Ok(Json(SyncFoldResponse {
        project_id,
        imported: stats.imported,
        existing: stats.existing,
        errors: stats.errors,
        message: format!(
            "Synced fold/: {} imported, {} already existed, {} errors",
            stats.imported, stats.existing, stats.errors
        ),
    }))
}

/// Update a repository's settings.
///
/// PATCH /projects/:project_id/repositories/:repo_id
#[axum::debug_handler]
async fn update_repository(
    State(state): State<AppState>,
    Path(path): Path<RepositoryPath>,
    Json(request): Json<UpdateRepositoryRequest>,
) -> Result<Json<RepositoryResponse>> {
    let repo_id = path.repo_id.to_string();

    // Get existing repository
    let _repo = db::get_repository(&state.db, &repo_id).await?;

    // Build update
    let update = db::UpdateRepository {
        access_token: request.access_token,
        branch: request.default_branch,
        notification_type: request.polling_enabled.map(|enabled| {
            if enabled { "polling".to_string() } else { "webhook".to_string() }
        }),
        sync_cursor: request.polling_interval_secs.map(|s| s.to_string()),
        ..Default::default()
    };

    // Update repository
    let updated = db::update_repository(&state.db, &repo_id, update).await?;

    let polling_enabled = updated.notification_type.as_deref() == Some("polling");
    Ok(Json(RepositoryResponse {
        id: path.repo_id,
        project_id: Uuid::parse_str(&updated.project_id).unwrap_or_else(|_| Uuid::new_v4()),
        provider: match updated.provider.as_str() {
            "gitlab" => RepositoryProvider::GitLab,
            "local" => RepositoryProvider::Local,
            _ => RepositoryProvider::GitHub,
        },
        owner: updated.owner.clone(),
        name: updated.repo.clone(),
        full_name: updated.full_name(),
        default_branch: updated.branch.clone(),
        status: if updated.last_indexed_at.is_some() {
            RepositoryStatus::Connected
        } else {
            RepositoryStatus::Syncing
        },
        auto_index: updated.webhook_id.is_some() || polling_enabled,
        polling_enabled,
        polling_interval_secs: if polling_enabled {
            updated.source_config.as_ref().and_then(|cfg| {
                serde_json::from_str::<serde_json::Value>(cfg)
                    .ok()
                    .and_then(|v| v.get("polling_interval_secs")?.as_u64())
                    .map(|v| v as u32)
            })
        } else {
            None
        },
        local_path: updated.local_path.clone(),
        head_sha: updated.last_commit_sha.clone(),
        last_indexed_at: updated.last_indexed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }),
        last_polled_at: updated.last_sync.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        }),
        webhook_id: updated.webhook_id,
        error_message: None,
        created_at: DateTime::parse_from_rfc3339(&updated.created_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: Utc::now(),
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

// ============================================================================
// File Source Provider Handlers
// ============================================================================

/// List available file source providers.
///
/// GET /file-sources/providers
///
/// Returns all registered file source providers with their capabilities.
#[axum::debug_handler]
async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<ListProvidersResponse>> {
    let providers = state
        .providers
        .providers()
        .into_iter()
        .map(|p| ProviderInfo {
            provider_type: p.provider_type.to_string(),
            display_name: p.display_name.to_string(),
            supports_webhooks: p.supports_webhooks,
            requires_polling: p.requires_polling,
            available: true,
        })
        .collect();

    Ok(Json(ListProvidersResponse { providers }))
}
