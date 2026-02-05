//! Git commits and pull requests database queries.
//!
//! These are indexed from git history during repository indexing.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// Types
// ============================================================================

/// PR state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

impl PrState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }
}

/// Git commit record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GitCommit {
    pub id: String,
    pub project_id: String,
    pub sha: String,
    pub message: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub files_changed: Option<String>, // JSON array
    pub insertions: Option<i32>,
    pub deletions: Option<i32>,
    pub committed_at: String,
    pub indexed_at: String,
    pub summary_memory_id: Option<String>,
}

/// Input for creating a git commit record.
#[derive(Debug, Clone)]
pub struct CreateGitCommit {
    pub id: String,
    pub project_id: String,
    pub sha: String,
    pub message: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub files_changed: Option<String>,
    pub insertions: Option<i32>,
    pub deletions: Option<i32>,
    pub committed_at: String,
}

/// Git pull request record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GitPullRequest {
    pub id: String,
    pub project_id: String,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub author: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub created_at: String,
    pub merged_at: Option<String>,
    pub indexed_at: String,
    pub memory_id: Option<String>,
}

/// Input for creating/updating a git pull request.
#[derive(Debug, Clone)]
pub struct CreateGitPullRequest {
    pub id: String,
    pub project_id: String,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
    pub state: PrState,
    pub author: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub created_at: String,
    pub merged_at: Option<String>,
}

// ============================================================================
// Git Commit Queries
// ============================================================================

/// Create a git commit record.
pub async fn create_git_commit(pool: &DbPool, input: CreateGitCommit) -> Result<GitCommit> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        INSERT INTO git_commits (
            id, project_id, sha, message, author_name, author_email,
            files_changed, insertions, deletions, committed_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(project_id, sha) DO UPDATE SET
            message = excluded.message,
            author_name = excluded.author_name,
            author_email = excluded.author_email,
            files_changed = excluded.files_changed,
            insertions = excluded.insertions,
            deletions = excluded.deletions
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(&input.sha)
    .bind(&input.message)
    .bind(&input.author_name)
    .bind(&input.author_email)
    .bind(&input.files_changed)
    .bind(input.insertions)
    .bind(input.deletions)
    .bind(&input.committed_at)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get a git commit by SHA.
pub async fn get_git_commit_by_sha(
    pool: &DbPool,
    project_id: &str,
    sha: &str,
) -> Result<Option<GitCommit>> {
    sqlx::query_as::<_, GitCommit>(
        "SELECT * FROM git_commits WHERE project_id = ? AND sha = ?",
    )
    .bind(project_id)
    .bind(sha)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// List commits for a project.
pub async fn list_project_commits(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
) -> Result<Vec<GitCommit>> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        SELECT * FROM git_commits
        WHERE project_id = ?
        ORDER BY committed_at DESC
        LIMIT ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// Git Pull Request Queries
// ============================================================================

/// Upsert a git pull request (create or update).
pub async fn upsert_git_pull_request(
    pool: &DbPool,
    input: CreateGitPullRequest,
) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        INSERT INTO git_pull_requests (
            id, project_id, number, title, description, state,
            author, source_branch, target_branch, created_at, merged_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(project_id, number) DO UPDATE SET
            title = excluded.title,
            description = excluded.description,
            state = excluded.state,
            merged_at = excluded.merged_at
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(input.number)
    .bind(&input.title)
    .bind(&input.description)
    .bind(input.state.as_str())
    .bind(&input.author)
    .bind(&input.source_branch)
    .bind(&input.target_branch)
    .bind(&input.created_at)
    .bind(&input.merged_at)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get a pull request by number.
pub async fn get_git_pull_request(
    pool: &DbPool,
    project_id: &str,
    number: i32,
) -> Result<Option<GitPullRequest>> {
    sqlx::query_as::<_, GitPullRequest>(
        "SELECT * FROM git_pull_requests WHERE project_id = ? AND number = ?",
    )
    .bind(project_id)
    .bind(number)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// List pull requests for a project.
pub async fn list_project_pull_requests(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
) -> Result<Vec<GitPullRequest>> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        SELECT * FROM git_pull_requests
        WHERE project_id = ?
        ORDER BY created_at DESC
        LIMIT ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Update PR memory link.
pub async fn update_pr_memory_link(
    pool: &DbPool,
    pr_id: &str,
    memory_id: &str,
) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>(
        "UPDATE git_pull_requests SET memory_id = ? WHERE id = ? RETURNING *",
    )
    .bind(memory_id)
    .bind(pr_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Pull request not found: {}", pr_id)))
}
