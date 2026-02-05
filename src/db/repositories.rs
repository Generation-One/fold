//! Repository and git data database queries.
//!
//! Handles connected git repositories, commits, and pull requests.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;
use super::projects::GitProvider;

// ============================================================================
// Repository Types
// ============================================================================

/// Repository record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub project_id: String,
    pub provider: String,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub source_type: Option<String>,
    pub source_config: Option<String>,
    pub notification_type: Option<String>,
    pub webhook_id: Option<String>,
    pub webhook_secret: Option<String>,
    pub access_token: String,
    pub last_indexed_at: Option<String>,
    pub last_commit_sha: Option<String>,
    pub last_sync: Option<String>,
    pub sync_cursor: Option<String>,
    /// Local filesystem path where the repository is cloned
    pub local_path: Option<String>,
    pub created_at: String,
}

impl Repository {
    /// Get provider as enum.
    pub fn provider_enum(&self) -> Option<GitProvider> {
        GitProvider::from_str(&self.provider)
    }

    /// Get full repository path (owner/repo).
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// Get repository URL.
    pub fn url(&self) -> String {
        match self.provider.as_str() {
            "github" => format!("https://github.com/{}/{}", self.owner, self.repo),
            "gitlab" => format!("https://gitlab.com/{}/{}", self.owner, self.repo),
            "local" => self.local_path.clone().unwrap_or_else(|| format!("file://{}/{}", self.owner, self.repo)),
            _ => format!("{}/{}", self.owner, self.repo),
        }
    }
}

/// Input for creating a new repository.
#[derive(Debug, Clone)]
pub struct CreateRepository {
    pub id: String,
    pub project_id: String,
    pub provider: GitProvider,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub access_token: String,
    pub local_path: Option<String>,
}

/// Input for updating a repository.
#[derive(Debug, Clone, Default)]
pub struct UpdateRepository {
    pub branch: Option<String>,
    pub access_token: Option<String>,
    pub webhook_id: Option<String>,
    pub webhook_secret: Option<String>,
    pub notification_type: Option<String>,
    pub last_indexed_at: Option<String>,
    pub last_commit_sha: Option<String>,
    pub last_sync: Option<String>,
    pub sync_cursor: Option<String>,
    pub local_path: Option<String>,
}

// ============================================================================
// Git Commit Types
// ============================================================================

/// Git commit record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GitCommit {
    pub id: String,
    pub repository_id: String,
    pub sha: String,
    pub message: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub files_changed: Option<String>,  // JSON array
    pub insertions: Option<i32>,
    pub deletions: Option<i32>,
    pub committed_at: String,
    pub indexed_at: String,
    pub summary_memory_id: Option<String>,
}

impl GitCommit {
    /// Parse files_changed JSON into a vector.
    pub fn files_changed_vec(&self) -> Vec<FileChange> {
        self.files_changed
            .as_ref()
            .and_then(|f| serde_json::from_str(f).ok())
            .unwrap_or_default()
    }

    /// Get short SHA (first 7 characters).
    pub fn short_sha(&self) -> &str {
        if self.sha.len() >= 7 {
            &self.sha[..7]
        } else {
            &self.sha
        }
    }

    /// Get first line of commit message.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or(&self.message)
    }
}

/// File change info from a commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: String,  // 'added', 'modified', 'deleted', 'renamed'
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
}

/// Input for creating a git commit record.
#[derive(Debug, Clone)]
pub struct CreateGitCommit {
    pub id: String,
    pub repository_id: String,
    pub sha: String,
    pub message: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub files_changed: Option<Vec<FileChange>>,
    pub insertions: Option<i32>,
    pub deletions: Option<i32>,
    pub committed_at: String,
}

// ============================================================================
// Pull Request Types
// ============================================================================

/// PR state enumeration.
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
        match s {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }
}

/// Git pull request record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GitPullRequest {
    pub id: String,
    pub repository_id: String,
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

impl GitPullRequest {
    /// Get state as enum.
    pub fn state_enum(&self) -> Option<PrState> {
        PrState::from_str(&self.state)
    }

    /// Check if PR is open.
    pub fn is_open(&self) -> bool {
        self.state == "open"
    }

    /// Check if PR is merged.
    pub fn is_merged(&self) -> bool {
        self.state == "merged"
    }
}

/// Input for creating a pull request record.
#[derive(Debug, Clone)]
pub struct CreateGitPullRequest {
    pub id: String,
    pub repository_id: String,
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
// Repository Queries
// ============================================================================

/// Create a new repository.
pub async fn create_repository(pool: &DbPool, input: CreateRepository) -> Result<Repository> {
    sqlx::query_as::<_, Repository>(
        r#"
        INSERT INTO repositories (id, project_id, provider, owner, repo, branch, access_token, local_path)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(input.provider.as_str())
    .bind(&input.owner)
    .bind(&input.repo)
    .bind(&input.branch)
    .bind(&input.access_token)
    .bind(&input.local_path)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!(
                "Repository {}/{} branch {} already connected",
                input.owner, input.repo, input.branch
            ))
        }
        _ => Error::Database(e),
    })
}

/// Get a repository by ID.
pub async fn get_repository(pool: &DbPool, id: &str) -> Result<Repository> {
    sqlx::query_as::<_, Repository>("SELECT * FROM repositories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Repository not found: {}", id)))
}

/// Get a repository by ID (optional).
pub async fn get_repository_optional(pool: &DbPool, id: &str) -> Result<Option<Repository>> {
    sqlx::query_as::<_, Repository>("SELECT * FROM repositories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get repository by owner/repo/branch.
pub async fn get_repository_by_path(
    pool: &DbPool,
    project_id: &str,
    provider: &GitProvider,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Option<Repository>> {
    sqlx::query_as::<_, Repository>(
        r#"
        SELECT * FROM repositories
        WHERE project_id = ? AND provider = ? AND owner = ? AND repo = ? AND branch = ?
        "#,
    )
    .bind(project_id)
    .bind(provider.as_str())
    .bind(owner)
    .bind(repo)
    .bind(branch)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Update a repository.
pub async fn update_repository(pool: &DbPool, id: &str, input: UpdateRepository) -> Result<Repository> {
    let mut updates = Vec::new();
    let mut bindings: Vec<Option<String>> = Vec::new();

    if let Some(branch) = input.branch {
        updates.push("branch = ?");
        bindings.push(Some(branch));
    }
    if let Some(token) = input.access_token {
        updates.push("access_token = ?");
        bindings.push(Some(token));
    }
    if let Some(webhook_id) = input.webhook_id {
        updates.push("webhook_id = ?");
        bindings.push(Some(webhook_id));
    }
    if let Some(webhook_secret) = input.webhook_secret {
        updates.push("webhook_secret = ?");
        bindings.push(Some(webhook_secret));
    }
    if let Some(notification_type) = input.notification_type {
        updates.push("notification_type = ?");
        bindings.push(Some(notification_type));
    }
    if let Some(last_indexed_at) = input.last_indexed_at {
        updates.push("last_indexed_at = ?");
        bindings.push(Some(last_indexed_at));
    }
    if let Some(last_commit_sha) = input.last_commit_sha {
        updates.push("last_commit_sha = ?");
        bindings.push(Some(last_commit_sha));
    }
    if let Some(last_sync) = input.last_sync {
        updates.push("last_sync = ?");
        bindings.push(Some(last_sync));
    }
    if let Some(sync_cursor) = input.sync_cursor {
        updates.push("sync_cursor = ?");
        bindings.push(Some(sync_cursor));
    }
    if let Some(local_path) = input.local_path {
        updates.push("local_path = ?");
        bindings.push(Some(local_path));
    }

    if updates.is_empty() {
        return get_repository(pool, id).await;
    }

    let query = format!(
        "UPDATE repositories SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, Repository>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Repository not found: {}", id)))
}

/// Update repository indexing status.
pub async fn update_repository_indexed(
    pool: &DbPool,
    id: &str,
    commit_sha: &str,
) -> Result<Repository> {
    sqlx::query_as::<_, Repository>(
        r#"
        UPDATE repositories SET
            last_indexed_at = datetime('now'),
            last_commit_sha = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(commit_sha)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Repository not found: {}", id)))
}

/// Delete a repository.
pub async fn delete_repository(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM repositories WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Repository not found: {}", id)));
    }

    Ok(())
}

/// List repositories for a project.
/// Uses idx_repositories_project index.
pub async fn list_project_repositories(pool: &DbPool, project_id: &str) -> Result<Vec<Repository>> {
    sqlx::query_as::<_, Repository>(
        r#"
        SELECT * FROM repositories
        WHERE project_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List all repositories.
pub async fn list_repositories(pool: &DbPool) -> Result<Vec<Repository>> {
    sqlx::query_as::<_, Repository>(
        "SELECT * FROM repositories ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List all repositories with polling enabled.
/// These are repositories without webhooks that need periodic checking.
pub async fn list_polling_repositories(pool: &DbPool) -> Result<Vec<Repository>> {
    sqlx::query_as::<_, Repository>(
        r#"
        SELECT * FROM repositories
        WHERE notification_type = 'polling'
           OR (webhook_id IS NULL AND access_token != '')
        ORDER BY last_sync ASC NULLS FIRST
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Get a repository by its webhook ID.
/// Used for routing incoming webhook events to the correct repository.
pub async fn get_repository_by_webhook_id(pool: &DbPool, webhook_id: &str) -> Result<Option<Repository>> {
    sqlx::query_as::<_, Repository>(
        "SELECT * FROM repositories WHERE webhook_id = ?",
    )
    .bind(webhook_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Get the webhook secret for a repository.
/// Used for verifying webhook signature on incoming events.
pub async fn get_webhook_secret(pool: &DbPool, repo_id: &str) -> Result<Option<String>> {
    let result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT webhook_secret FROM repositories WHERE id = ?",
    )
    .bind(repo_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)?;

    Ok(result.flatten())
}

// ============================================================================
// Git Commit Queries
// ============================================================================

/// Create a git commit record.
pub async fn create_git_commit(pool: &DbPool, input: CreateGitCommit) -> Result<GitCommit> {
    let files_json = input.files_changed.map(|f| serde_json::to_string(&f).unwrap_or_default());

    sqlx::query_as::<_, GitCommit>(
        r#"
        INSERT INTO git_commits (
            id, repository_id, sha, message, author_name, author_email,
            files_changed, insertions, deletions, committed_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.repository_id)
    .bind(&input.sha)
    .bind(&input.message)
    .bind(&input.author_name)
    .bind(&input.author_email)
    .bind(&files_json)
    .bind(input.insertions)
    .bind(input.deletions)
    .bind(&input.committed_at)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("Commit {} already indexed", input.sha))
        }
        _ => Error::Database(e),
    })
}

/// Get a git commit by ID.
pub async fn get_git_commit(pool: &DbPool, id: &str) -> Result<GitCommit> {
    sqlx::query_as::<_, GitCommit>("SELECT * FROM git_commits WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Commit not found: {}", id)))
}

/// Get a git commit by SHA.
/// Uses idx_git_commits_sha index.
pub async fn get_git_commit_by_sha(
    pool: &DbPool,
    repository_id: &str,
    sha: &str,
) -> Result<Option<GitCommit>> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        SELECT * FROM git_commits
        WHERE repository_id = ? AND sha = ?
        "#,
    )
    .bind(repository_id)
    .bind(sha)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Update commit's summary memory ID.
pub async fn update_commit_summary(
    pool: &DbPool,
    commit_id: &str,
    memory_id: &str,
) -> Result<GitCommit> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        UPDATE git_commits SET summary_memory_id = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(memory_id)
    .bind(commit_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Commit not found: {}", commit_id)))
}

/// List commits for a repository.
/// Uses idx_git_commits_repo index.
pub async fn list_repository_commits(
    pool: &DbPool,
    repository_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<GitCommit>> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        SELECT * FROM git_commits
        WHERE repository_id = ?
        ORDER BY committed_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(repository_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List commits in a date range.
/// Uses idx_git_commits_date index.
pub async fn list_commits_in_range(
    pool: &DbPool,
    repository_id: &str,
    from: &str,
    to: &str,
) -> Result<Vec<GitCommit>> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        SELECT * FROM git_commits
        WHERE repository_id = ? AND committed_at >= ? AND committed_at <= ?
        ORDER BY committed_at DESC
        "#,
    )
    .bind(repository_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List commits without summaries (for batch summarization).
pub async fn list_commits_without_summary(
    pool: &DbPool,
    repository_id: &str,
    limit: i64,
) -> Result<Vec<GitCommit>> {
    sqlx::query_as::<_, GitCommit>(
        r#"
        SELECT * FROM git_commits
        WHERE repository_id = ? AND summary_memory_id IS NULL
        ORDER BY committed_at DESC
        LIMIT ?
        "#,
    )
    .bind(repository_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Count commits for a repository.
pub async fn count_repository_commits(pool: &DbPool, repository_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM git_commits WHERE repository_id = ?",
    )
    .bind(repository_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Delete commits for a repository.
pub async fn delete_repository_commits(pool: &DbPool, repository_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM git_commits WHERE repository_id = ?")
        .bind(repository_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Pull Request Queries
// ============================================================================

/// Create a pull request record.
pub async fn create_git_pull_request(pool: &DbPool, input: CreateGitPullRequest) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        INSERT INTO git_pull_requests (
            id, repository_id, number, title, description, state,
            author, source_branch, target_branch, created_at, merged_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.repository_id)
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
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("PR #{} already indexed", input.number))
        }
        _ => Error::Database(e),
    })
}

/// Get a pull request by ID.
pub async fn get_git_pull_request(pool: &DbPool, id: &str) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>("SELECT * FROM git_pull_requests WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Pull request not found: {}", id)))
}

/// Get a pull request by number.
pub async fn get_git_pull_request_by_number(
    pool: &DbPool,
    repository_id: &str,
    number: i32,
) -> Result<Option<GitPullRequest>> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        SELECT * FROM git_pull_requests
        WHERE repository_id = ? AND number = ?
        "#,
    )
    .bind(repository_id)
    .bind(number)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Update pull request state.
pub async fn update_pull_request_state(
    pool: &DbPool,
    id: &str,
    state: PrState,
    merged_at: Option<&str>,
) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        UPDATE git_pull_requests SET
            state = ?,
            merged_at = ?,
            indexed_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(state.as_str())
    .bind(merged_at)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Pull request not found: {}", id)))
}

/// Update pull request's memory ID.
pub async fn update_pull_request_memory(
    pool: &DbPool,
    pr_id: &str,
    memory_id: &str,
) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        UPDATE git_pull_requests SET memory_id = ?
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(memory_id)
    .bind(pr_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Pull request not found: {}", pr_id)))
}

/// List pull requests for a repository.
/// Uses idx_git_prs_repo index.
pub async fn list_repository_pull_requests(
    pool: &DbPool,
    repository_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<GitPullRequest>> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        SELECT * FROM git_pull_requests
        WHERE repository_id = ?
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(repository_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List open pull requests for a repository.
/// Uses idx_git_prs_state index.
pub async fn list_open_pull_requests(pool: &DbPool, repository_id: &str) -> Result<Vec<GitPullRequest>> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        SELECT * FROM git_pull_requests
        WHERE repository_id = ? AND state = 'open'
        ORDER BY created_at DESC
        "#,
    )
    .bind(repository_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List pull requests by state.
pub async fn list_pull_requests_by_state(
    pool: &DbPool,
    repository_id: &str,
    state: PrState,
) -> Result<Vec<GitPullRequest>> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        SELECT * FROM git_pull_requests
        WHERE repository_id = ? AND state = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(repository_id)
    .bind(state.as_str())
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Count pull requests for a repository.
pub async fn count_repository_pull_requests(pool: &DbPool, repository_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM git_pull_requests WHERE repository_id = ?",
    )
    .bind(repository_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Upsert a pull request (for webhook updates).
pub async fn upsert_git_pull_request(pool: &DbPool, input: CreateGitPullRequest) -> Result<GitPullRequest> {
    sqlx::query_as::<_, GitPullRequest>(
        r#"
        INSERT INTO git_pull_requests (
            id, repository_id, number, title, description, state,
            author, source_branch, target_branch, created_at, merged_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(repository_id, number) DO UPDATE SET
            title = excluded.title,
            description = excluded.description,
            state = excluded.state,
            merged_at = excluded.merged_at,
            indexed_at = datetime('now')
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.repository_id)
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
    async fn test_create_and_get_repository() {
        let pool = setup_test_db().await;

        let repo = create_repository(&pool, CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: GitProvider::GitHub,
            owner: "testorg".to_string(),
            repo: "testrepo".to_string(),
            branch: "main".to_string(),
            access_token: "token123".to_string(),
            local_path: None,
        }).await.unwrap();

        assert_eq!(repo.id, "repo-1");
        assert_eq!(repo.full_name(), "testorg/testrepo");
        assert_eq!(repo.url(), "https://github.com/testorg/testrepo");

        let fetched = get_repository(&pool, "repo-1").await.unwrap();
        assert_eq!(fetched.branch, "main");
    }

    #[tokio::test]
    async fn test_create_git_commit() {
        let pool = setup_test_db().await;

        create_repository(&pool, CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: GitProvider::GitHub,
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
            access_token: "token".to_string(),
            local_path: None,
        }).await.unwrap();

        let commit = create_git_commit(&pool, CreateGitCommit {
            id: "commit-1".to_string(),
            repository_id: "repo-1".to_string(),
            sha: "abc123def456".to_string(),
            message: "Fix bug in authentication\n\nDetailed description".to_string(),
            author_name: Some("Test User".to_string()),
            author_email: Some("test@example.com".to_string()),
            files_changed: Some(vec![FileChange {
                path: "src/auth.rs".to_string(),
                status: "modified".to_string(),
                additions: Some(10),
                deletions: Some(5),
            }]),
            insertions: Some(10),
            deletions: Some(5),
            committed_at: "2024-01-01T12:00:00Z".to_string(),
        }).await.unwrap();

        assert_eq!(commit.short_sha(), "abc123d");
        assert_eq!(commit.summary(), "Fix bug in authentication");
        assert_eq!(commit.files_changed_vec().len(), 1);
    }

    #[tokio::test]
    async fn test_create_pull_request() {
        let pool = setup_test_db().await;

        create_repository(&pool, CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: GitProvider::GitHub,
            owner: "test".to_string(),
            repo: "test".to_string(),
            branch: "main".to_string(),
            access_token: "token".to_string(),
            local_path: None,
        }).await.unwrap();

        let pr = create_git_pull_request(&pool, CreateGitPullRequest {
            id: "pr-1".to_string(),
            repository_id: "repo-1".to_string(),
            number: 42,
            title: "Add new feature".to_string(),
            description: Some("This PR adds...".to_string()),
            state: PrState::Open,
            author: Some("contributor".to_string()),
            source_branch: Some("feature/new".to_string()),
            target_branch: Some("main".to_string()),
            created_at: "2024-01-01T12:00:00Z".to_string(),
            merged_at: None,
        }).await.unwrap();

        assert_eq!(pr.number, 42);
        assert!(pr.is_open());
        assert!(!pr.is_merged());
    }
}
