//! Webhooks Routes
//!
//! Webhook handlers for file source providers (GitHub, GitLab, etc.).
//!
//! Uses the FileSourceProvider abstraction for signature verification
//! and event parsing, allowing new providers to be added easily.
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
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::services::FileSourceProvider;
use crate::{db, AppState, Error, Result};

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
/// Verifies the webhook signature using the FileSourceProvider abstraction
/// and processes the event.
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

    // Get webhook secret from database
    let webhook_secret = get_github_webhook_secret(&state, repo_id).await?;

    // Verify signature using the provider abstraction
    if let Some(signature) = headers.get("X-Hub-Signature-256") {
        let signature = signature.to_str().map_err(|_| {
            Error::Webhook("Invalid signature header".into())
        })?;

        // Use provider-agnostic verification
        let provider = state.providers.get("github").ok_or_else(|| {
            Error::Webhook("GitHub provider not available".into())
        })?;

        if !provider.verify_notification(&body, signature, &webhook_secret) {
            return Err(Error::Webhook("Signature verification failed".into()));
        }
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

    // Fetch repository webhook token from database
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
async fn get_github_webhook_secret(state: &AppState, repo_id: Uuid) -> Result<String> {
    let secret = db::get_webhook_secret(&state.db, &repo_id.to_string()).await?;
    Ok(secret.unwrap_or_default())
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
async fn get_gitlab_webhook_token(state: &AppState, repo_id: Uuid) -> Result<String> {
    // GitLab uses the same secret field as GitHub
    let secret = db::get_webhook_secret(&state.db, &repo_id.to_string()).await?;
    Ok(secret.unwrap_or_default())
}

// ============================================================================
// Event Processing Functions
// ============================================================================

/// Process GitHub push event.
async fn process_github_push(
    state: &AppState,
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

    // Get repository from database
    let repo_id_str = repo_id.to_string();
    let repo = db::get_repository(&state.db, &repo_id_str).await?;

    // Only process pushes to the tracked branch
    let branch = git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref);
    if branch != repo.branch {
        tracing::debug!(
            branch = %branch,
            tracked = %repo.branch,
            "Ignoring push to non-tracked branch"
        );
        return Ok(None);
    }

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

    // Create indexing job in database
    let job_id = crate::models::new_id();
    let job = db::create_job(
        &state.db,
        db::CreateJob::new(job_id.clone(), db::JobType::IndexRepo)
            .with_project(repo.project_id.clone())
            .with_repository(repo_id_str.clone())
            .with_total_items(changed_files.len() as i32),
    )
    .await?;

    tracing::info!(
        job_id = %job.id,
        files = changed_files.len(),
        "Created indexing job"
    );

    Ok(Some(Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4())))
}

/// Process GitHub pull request event.
async fn process_github_pull_request(
    state: &AppState,
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

        let repo_id_str = repo_id.to_string();

        // Determine PR state
        let pr_state = if pr.merged.unwrap_or(false) {
            db::PrState::Merged
        } else {
            match pr.state.as_str() {
                "closed" => db::PrState::Closed,
                _ => db::PrState::Open,
            }
        };

        // Upsert PR record in database
        let pr_record = db::upsert_git_pull_request(
            &state.db,
            db::CreateGitPullRequest {
                id: crate::models::new_id(),
                repository_id: repo_id_str.clone(),
                number: pr.number as i32,
                title: pr.title.clone(),
                description: None, // GitHub webhook doesn't include body in simplified payload
                state: pr_state,
                author: Some(pr.user.login.clone()),
                source_branch: Some(pr.head.branch.clone()),
                target_branch: Some(pr.base.branch.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
                merged_at: if pr.merged.unwrap_or(false) {
                    Some(chrono::Utc::now().to_rfc3339())
                } else {
                    None
                },
            },
        ).await?;

        tracing::debug!(
            pr_id = %pr_record.id,
            pr_number = pr.number,
            state = %pr_record.state,
            "Stored pull request"
        );

        // Process based on action - hybrid diff indexing for opened/synchronized PRs
        match action {
            "opened" | "synchronize" => {
                // Hybrid diff indexing: fetch files, rank by impact, analyze top 4
                if let Err(e) = process_pr_diff_hybrid(
                    state,
                    &repo_id_str,
                    pr.number,
                    &pr.title,
                ).await {
                    tracing::warn!(
                        pr_number = pr.number,
                        error = %e,
                        "Failed to process PR diff (non-fatal)"
                    );
                }
            }
            "closed" => {
                if pr.merged.unwrap_or(false) {
                    tracing::info!(pr_number = pr.number, "Pull request merged");
                }
            }
            _ => {}
        }
    }

    Ok(None)
}

/// Process PR diff using hybrid approach - only analyze top impactful files.
///
/// Fetches PR files, ranks by impact (additions + deletions), and calls LLM
/// for the top 4 most impactful files to minimize API costs while capturing
/// the most important changes.
async fn process_pr_diff_hybrid(
    state: &AppState,
    repo_id: &str,
    pr_number: u32,
    pr_title: &str,
) -> Result<()> {
    // Get repository details
    let repo = db::get_repository(&state.db, repo_id).await?;

    // Fetch PR files from GitHub
    let files = state.github.get_pull_request_files(
        &repo.owner,
        &repo.repo,
        pr_number,
        &repo.access_token,
    ).await?;

    if files.is_empty() {
        tracing::debug!(pr_number = pr_number, "No files in PR");
        return Ok(());
    }

    // Rank files by impact (additions + deletions), excluding trivial files
    let mut ranked_files: Vec<_> = files
        .iter()
        .filter(|f| {
            // Exclude lockfiles, generated files, and very small changes
            let path = f.filename.to_lowercase();
            !path.contains("lock") &&
            !path.contains(".min.") &&
            !path.ends_with(".map") &&
            !path.starts_with("vendor/") &&
            !path.starts_with("node_modules/") &&
            (f.additions + f.deletions) > 2
        })
        .collect();

    // Sort by total changes (most impactful first)
    ranked_files.sort_by(|a, b| {
        (b.additions + b.deletions).cmp(&(a.additions + a.deletions))
    });

    // Take top 4 most impactful files
    let top_files: Vec<_> = ranked_files.into_iter().take(4).collect();

    if top_files.is_empty() {
        tracing::debug!(pr_number = pr_number, "No significant files to analyze");
        return Ok(());
    }

    tracing::info!(
        pr_number = pr_number,
        files_total = files.len(),
        files_analyzed = top_files.len(),
        "Analyzing top impactful files in PR"
    );

    // Analyze each top file with LLM
    let mut analyses = Vec::new();
    for file in &top_files {
        match state.llm.summarize_pr_diff(
            pr_title,
            &file.filename,
            &file.status,
            file.additions,
            file.deletions,
            file.patch.as_deref(),
        ).await {
            Ok(analysis) => {
                analyses.push(format!(
                    "**{}** (+{}, -{})\n{}",
                    file.filename, file.additions, file.deletions, analysis
                ));
            }
            Err(e) => {
                tracing::warn!(
                    file = %file.filename,
                    error = %e,
                    "Failed to analyze file diff"
                );
            }
        }
    }

    if !analyses.is_empty() {
        // Store analysis in PR description field via update
        let analysis_text = analyses.join("\n\n---\n\n");
        tracing::debug!(
            pr_number = pr_number,
            analysis_len = analysis_text.len(),
            "Generated PR diff analysis"
        );

        // Update PR record with analysis (stored in description field)
        sqlx::query(
            "UPDATE git_pull_requests SET description = ? WHERE repository_id = ? AND number = ?"
        )
        .bind(&analysis_text)
        .bind(repo_id)
        .bind(pr_number as i32)
        .execute(&state.db)
        .await?;
    }

    Ok(())
}

/// Process GitLab push event.
async fn process_gitlab_push(
    state: &AppState,
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

    // Get repository from database
    let repo_id_str = repo_id.to_string();
    let repo = db::get_repository(&state.db, &repo_id_str).await?;

    // Only process pushes to the tracked branch
    let branch = git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref);
    if branch != repo.branch {
        tracing::debug!(
            branch = %branch,
            tracked = %repo.branch,
            "Ignoring push to non-tracked branch"
        );
        return Ok(None);
    }

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

    // Create indexing job in database
    let job_id = crate::models::new_id();
    let job = db::create_job(
        &state.db,
        db::CreateJob::new(job_id.clone(), db::JobType::IndexRepo)
            .with_project(repo.project_id.clone())
            .with_repository(repo_id_str.clone())
            .with_total_items(changed_files.len() as i32),
    )
    .await?;

    Ok(Some(Uuid::parse_str(&job.id).unwrap_or_else(|_| Uuid::new_v4())))
}

/// Process GitLab merge request event.
async fn process_gitlab_merge_request(
    state: &AppState,
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

        let repo_id_str = repo_id.to_string();

        // Determine MR state
        let mr_state = match mr.state.as_str() {
            "merged" => db::PrState::Merged,
            "closed" => db::PrState::Closed,
            _ => db::PrState::Open,
        };

        // Get author from user payload if available
        let author = payload.user.as_ref().map(|u| u.username.clone());

        // Upsert MR record in database (GitLab MRs use the same table as GitHub PRs)
        let mr_record = db::upsert_git_pull_request(
            &state.db,
            db::CreateGitPullRequest {
                id: crate::models::new_id(),
                repository_id: repo_id_str.clone(),
                number: mr.iid as i32,
                title: mr.title.clone(),
                description: None,
                state: mr_state,
                author,
                source_branch: Some(mr.source_branch.clone()),
                target_branch: Some(mr.target_branch.clone()),
                created_at: Utc::now().to_rfc3339(),
                merged_at: if mr.state == "merged" {
                    Some(Utc::now().to_rfc3339())
                } else {
                    None
                },
            },
        ).await?;

        tracing::debug!(
            mr_id = %mr_record.id,
            mr_iid = mr.iid,
            state = %mr_record.state,
            "Stored merge request"
        );

        // Process based on action
        match action {
            "open" | "update" => {
                tracing::debug!(action = %action, "MR action recorded");
            }
            "merge" => {
                tracing::info!(mr_iid = mr.iid, "Merge request merged");
            }
            _ => {}
        }
    }

    Ok(None)
}
