//! Webhooks Routes
//!
//! Git webhook handlers for GitHub and GitLab.
//!
//! Routes:
//! - POST /webhooks/github/:repo_id - Handle GitHub webhook
//! - POST /webhooks/gitlab/:repo_id - Handle GitLab webhook

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::{AppState, Error, Result};

type HmacSha256 = Hmac<Sha256>;

/// Build webhook routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/github/:repo_id", post(handle_github_webhook))
        .route("/gitlab/:repo_id", post(handle_gitlab_webhook))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Webhook response.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<Uuid>,
}

// ============================================================================
// GitHub Webhook Types
// ============================================================================

/// GitHub webhook payload (simplified).
#[derive(Debug, Deserialize)]
pub struct GitHubWebhookPayload {
    pub action: Option<String>,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub repository: Option<GitHubRepository>,
    pub sender: Option<GitHubUser>,
    pub commits: Option<Vec<GitHubCommit>>,
    pub head_commit: Option<GitHubCommit>,
    pub pull_request: Option<GitHubPullRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommit {
    pub id: String,
    pub message: String,
    pub author: GitHubCommitAuthor,
    pub added: Option<Vec<String>>,
    pub removed: Option<Vec<String>>,
    pub modified: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommitAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequest {
    pub number: u32,
    pub title: String,
    pub state: String,
    pub user: GitHubUser,
    pub head: GitHubPullRequestRef,
    pub base: GitHubPullRequestRef,
    pub merged: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequestRef {
    #[serde(rename = "ref")]
    pub branch: String,
    pub sha: String,
}

// ============================================================================
// GitLab Webhook Types
// ============================================================================

/// GitLab webhook payload (simplified).
#[derive(Debug, Deserialize)]
pub struct GitLabWebhookPayload {
    pub object_kind: String,
    pub event_type: Option<String>,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub project: Option<GitLabProject>,
    pub user: Option<GitLabUser>,
    pub commits: Option<Vec<GitLabCommit>>,
    pub object_attributes: Option<GitLabMergeRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GitLabProject {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
pub struct GitLabUser {
    pub id: i64,
    pub username: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GitLabCommit {
    pub id: String,
    pub message: String,
    pub author: GitLabCommitAuthor,
    pub added: Option<Vec<String>>,
    pub removed: Option<Vec<String>>,
    pub modified: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct GitLabCommitAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct GitLabMergeRequest {
    pub iid: u32,
    pub title: String,
    pub state: String,
    pub action: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RepoIdPath {
    pub repo_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// Handle GitHub webhook.
///
/// POST /webhooks/github/:repo_id
///
/// Verifies the webhook signature and processes the event.
#[axum::debug_handler]
async fn handle_github_webhook(
    State(state): State<AppState>,
    Path(path): Path<RepoIdPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse> {
    let repo_id = path.repo_id;

    // Get event type
    let event_type = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    tracing::info!(
        repo_id = %repo_id,
        event_type = %event_type,
        "Received GitHub webhook"
    );

    // TODO: Fetch repository from database to get webhook secret
    let webhook_secret = get_github_webhook_secret(&state, repo_id).await?;

    // Verify signature
    if let Some(signature) = headers.get("X-Hub-Signature-256") {
        let signature = signature.to_str().map_err(|_| {
            Error::Webhook("Invalid signature header".into())
        })?;

        verify_github_signature(&body, &webhook_secret, signature)?;
    } else {
        // Signature required if secret is configured
        if !webhook_secret.is_empty() {
            return Err(Error::Webhook("Missing signature header".into()));
        }
    }

    // Parse payload
    let payload: GitHubWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| Error::Webhook(format!("Invalid payload: {}", e)))?;

    // Process event
    let job_id = match event_type {
        "push" => process_github_push(&state, repo_id, &payload).await?,
        "pull_request" => process_github_pull_request(&state, repo_id, &payload).await?,
        "ping" => {
            tracing::info!("GitHub webhook ping received");
            None
        }
        _ => {
            tracing::debug!(event_type = %event_type, "Ignoring GitHub event");
            None
        }
    };

    Ok((
        StatusCode::OK,
        Json(WebhookResponse {
            status: "ok".into(),
            message: format!("Processed {} event", event_type),
            job_id,
        }),
    ))
}

/// Handle GitLab webhook.
///
/// POST /webhooks/gitlab/:repo_id
///
/// Verifies the webhook token and processes the event.
#[axum::debug_handler]
async fn handle_gitlab_webhook(
    State(state): State<AppState>,
    Path(path): Path<RepoIdPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse> {
    let repo_id = path.repo_id;

    // Get event type
    let event_type = headers
        .get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    tracing::info!(
        repo_id = %repo_id,
        event_type = %event_type,
        "Received GitLab webhook"
    );

    // TODO: Fetch repository from database to get webhook token
    let webhook_token = get_gitlab_webhook_token(&state, repo_id).await?;

    // Verify token
    if let Some(token) = headers.get("X-Gitlab-Token") {
        let token = token.to_str().map_err(|_| {
            Error::Webhook("Invalid token header".into())
        })?;

        if token != webhook_token {
            return Err(Error::Webhook("Invalid webhook token".into()));
        }
    } else {
        // Token required if configured
        if !webhook_token.is_empty() {
            return Err(Error::Webhook("Missing token header".into()));
        }
    }

    // Parse payload
    let payload: GitLabWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| Error::Webhook(format!("Invalid payload: {}", e)))?;

    // Process event
    let job_id = match payload.object_kind.as_str() {
        "push" => process_gitlab_push(&state, repo_id, &payload).await?,
        "merge_request" => process_gitlab_merge_request(&state, repo_id, &payload).await?,
        _ => {
            tracing::debug!(event_type = %payload.object_kind, "Ignoring GitLab event");
            None
        }
    };

    Ok((
        StatusCode::OK,
        Json(WebhookResponse {
            status: "ok".into(),
            message: format!("Processed {} event", event_type),
            job_id,
        }),
    ))
}

// ============================================================================
// Verification Functions
// ============================================================================

/// Get GitHub webhook secret for a repository.
async fn get_github_webhook_secret(_state: &AppState, _repo_id: Uuid) -> Result<String> {
    // TODO: Fetch from database
    Ok(String::new())
}

/// Verify GitHub webhook signature.
fn verify_github_signature(body: &[u8], secret: &str, signature: &str) -> Result<()> {
    let signature = signature
        .strip_prefix("sha256=")
        .ok_or_else(|| Error::Webhook("Invalid signature format".into()))?;

    let expected = hex::decode(signature)
        .map_err(|_| Error::Webhook("Invalid signature hex".into()))?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| Error::Webhook("Invalid secret".into()))?;
    mac.update(body);

    mac.verify_slice(&expected)
        .map_err(|_| Error::Webhook("Signature verification failed".into()))?;

    Ok(())
}

/// Get GitLab webhook token for a repository.
async fn get_gitlab_webhook_token(_state: &AppState, _repo_id: Uuid) -> Result<String> {
    // TODO: Fetch from database
    Ok(String::new())
}

// ============================================================================
// Event Processing Functions
// ============================================================================

/// Process GitHub push event.
async fn process_github_push(
    _state: &AppState,
    repo_id: Uuid,
    payload: &GitHubWebhookPayload,
) -> Result<Option<Uuid>> {
    let git_ref = payload.git_ref.as_deref().unwrap_or("");
    let commits = payload.commits.as_ref().map(|c| c.len()).unwrap_or(0);

    tracing::info!(
        repo_id = %repo_id,
        git_ref = %git_ref,
        commits = commits,
        "Processing GitHub push"
    );

    // Only process pushes to default branch
    // TODO: Check if this is the default branch

    // Collect changed files
    let mut changed_files: Vec<String> = Vec::new();
    if let Some(commits) = &payload.commits {
        for commit in commits {
            if let Some(added) = &commit.added {
                changed_files.extend(added.clone());
            }
            if let Some(modified) = &commit.modified {
                changed_files.extend(modified.clone());
            }
        }
    }

    // TODO: Queue indexing job for changed files
    let job_id = Uuid::new_v4();

    tracing::info!(
        job_id = %job_id,
        files = changed_files.len(),
        "Queued indexing job"
    );

    Ok(Some(job_id))
}

/// Process GitHub pull request event.
async fn process_github_pull_request(
    _state: &AppState,
    repo_id: Uuid,
    payload: &GitHubWebhookPayload,
) -> Result<Option<Uuid>> {
    let action = payload.action.as_deref().unwrap_or("");
    let pr = payload.pull_request.as_ref();

    if let Some(pr) = pr {
        tracing::info!(
            repo_id = %repo_id,
            action = %action,
            pr_number = pr.number,
            title = %pr.title,
            "Processing GitHub pull request"
        );

        // Process based on action
        match action {
            "opened" | "synchronize" => {
                // TODO: Index PR changes
            }
            "closed" => {
                if pr.merged.unwrap_or(false) {
                    // TODO: Create merge memory
                }
            }
            _ => {}
        }
    }

    Ok(None)
}

/// Process GitLab push event.
async fn process_gitlab_push(
    _state: &AppState,
    repo_id: Uuid,
    payload: &GitLabWebhookPayload,
) -> Result<Option<Uuid>> {
    let git_ref = payload.git_ref.as_deref().unwrap_or("");
    let commits = payload.commits.as_ref().map(|c| c.len()).unwrap_or(0);

    tracing::info!(
        repo_id = %repo_id,
        git_ref = %git_ref,
        commits = commits,
        "Processing GitLab push"
    );

    // Collect changed files
    let mut changed_files: Vec<String> = Vec::new();
    if let Some(commits) = &payload.commits {
        for commit in commits {
            if let Some(added) = &commit.added {
                changed_files.extend(added.clone());
            }
            if let Some(modified) = &commit.modified {
                changed_files.extend(modified.clone());
            }
        }
    }

    // TODO: Queue indexing job
    let job_id = Uuid::new_v4();

    Ok(Some(job_id))
}

/// Process GitLab merge request event.
async fn process_gitlab_merge_request(
    _state: &AppState,
    repo_id: Uuid,
    payload: &GitLabWebhookPayload,
) -> Result<Option<Uuid>> {
    let mr = payload.object_attributes.as_ref();

    if let Some(mr) = mr {
        let action = mr.action.as_deref().unwrap_or("");

        tracing::info!(
            repo_id = %repo_id,
            action = %action,
            mr_iid = mr.iid,
            title = %mr.title,
            "Processing GitLab merge request"
        );

        // Process based on action
        match action {
            "open" | "update" => {
                // TODO: Index MR changes
            }
            "merge" => {
                // TODO: Create merge memory
            }
            _ => {}
        }
    }

    Ok(None)
}
