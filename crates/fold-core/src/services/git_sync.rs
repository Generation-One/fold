//! Git sync service for webhook processing.
//!
//! Handles incoming webhooks from GitHub and GitLab,
//! processes commits, and creates memory summaries.

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{
    CommitInfo, GitCommit, Memory, MemoryCreate, MemoryLink, MemoryType, Project, Repository,
};

use super::{GitHubService, GitLabService, IndexerService, LlmService, MemoryService};

/// Service for processing git webhooks and syncing repositories.
#[derive(Clone)]
pub struct GitSyncService {
    db: DbPool,
    github: Arc<GitHubService>,
    gitlab: Arc<GitLabService>,
    memory: MemoryService,
    llm: Arc<LlmService>,
    indexer: IndexerService,
}

/// Webhook payload from GitHub
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubPushPayload {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub before: String,
    pub after: String,
    pub repository: GitHubRepoPayload,
    pub pusher: GitHubPusherPayload,
    pub commits: Vec<GitHubCommitPayload>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRepoPayload {
    pub id: i64,
    pub name: String,
    pub full_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubPusherPayload {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubCommitPayload {
    pub id: String,
    pub message: String,
    pub author: GitHubAuthorPayload,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAuthorPayload {
    pub name: String,
    pub email: String,
    pub username: Option<String>,
}

/// Webhook payload from GitLab
#[derive(Debug, Clone, Deserialize)]
pub struct GitLabPushPayload {
    pub object_kind: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub before: String,
    pub after: String,
    pub project: GitLabProjectPayload,
    pub user_name: String,
    pub user_email: String,
    pub commits: Vec<GitLabCommitPayload>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabProjectPayload {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabCommitPayload {
    pub id: String,
    pub message: String,
    pub author: GitLabAuthorPayload,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitLabAuthorPayload {
    pub name: String,
    pub email: String,
}

/// Result of processing a webhook
#[derive(Debug, Clone, Serialize)]
pub struct WebhookResult {
    pub repository_id: String,
    pub commits_processed: usize,
    pub memories_created: usize,
    pub links_created: usize,
}

impl GitSyncService {
    /// Create a new git sync service.
    pub fn new(
        db: DbPool,
        github: Arc<GitHubService>,
        gitlab: Arc<GitLabService>,
        memory: MemoryService,
        llm: Arc<LlmService>,
        indexer: IndexerService,
    ) -> Self {
        Self {
            db,
            github,
            gitlab,
            memory,
            llm,
            indexer,
        }
    }

    /// Process a GitHub push webhook.
    pub async fn process_github_push(
        &self,
        payload: GitHubPushPayload,
        repository: &Repository,
        project: &Project,
    ) -> Result<WebhookResult> {
        let branch = payload
            .ref_name
            .strip_prefix("refs/heads/")
            .unwrap_or(&payload.ref_name);

        // Only process pushes to the monitored branch
        if branch != repository.branch {
            debug!(
                branch = branch,
                expected = %repository.branch,
                "Ignoring push to non-monitored branch"
            );
            return Ok(WebhookResult {
                repository_id: repository.id.clone(),
                commits_processed: 0,
                memories_created: 0,
                links_created: 0,
            });
        }

        let mut memories_created = 0;
        let mut links_created = 0;

        for commit_payload in &payload.commits {
            // Get full commit details from GitHub
            let full_commit = self
                .github
                .get_commit(
                    &repository.owner,
                    &repository.repo,
                    &commit_payload.id,
                    &repository.access_token,
                )
                .await?;

            let commit_info = self.github.to_commit_info(full_commit);

            // Process the commit
            let (memories, links) = self
                .process_commit(&commit_info, repository, project)
                .await?;

            memories_created += memories;
            links_created += links;
        }

        // Update last indexed
        self.update_repository_last_indexed(&repository.id, &payload.after)
            .await?;

        info!(
            repository = %repository.full_name(),
            commits = payload.commits.len(),
            memories = memories_created,
            links = links_created,
            "Processed GitHub push"
        );

        Ok(WebhookResult {
            repository_id: repository.id.clone(),
            commits_processed: payload.commits.len(),
            memories_created,
            links_created,
        })
    }

    /// Process a GitLab push webhook.
    pub async fn process_gitlab_push(
        &self,
        payload: GitLabPushPayload,
        repository: &Repository,
        project: &Project,
    ) -> Result<WebhookResult> {
        let branch = payload
            .ref_name
            .strip_prefix("refs/heads/")
            .unwrap_or(&payload.ref_name);

        // Only process pushes to the monitored branch
        if branch != repository.branch {
            debug!(
                branch = branch,
                expected = %repository.branch,
                "Ignoring push to non-monitored branch"
            );
            return Ok(WebhookResult {
                repository_id: repository.id.clone(),
                commits_processed: 0,
                memories_created: 0,
                links_created: 0,
            });
        }

        let mut memories_created = 0;
        let mut links_created = 0;

        for commit_payload in &payload.commits {
            // Get full commit details from GitLab
            let full_commit = self
                .gitlab
                .get_commit(
                    &repository.owner,
                    &repository.repo,
                    &commit_payload.id,
                    &repository.access_token,
                )
                .await?;

            let commit_info = self
                .gitlab
                .to_commit_info(
                    &repository.owner,
                    &repository.repo,
                    full_commit,
                    &repository.access_token,
                )
                .await?;

            // Process the commit
            let (memories, links) = self
                .process_commit(&commit_info, repository, project)
                .await?;

            memories_created += memories;
            links_created += links;
        }

        // Update last indexed
        self.update_repository_last_indexed(&repository.id, &payload.after)
            .await?;

        info!(
            repository = %repository.full_name(),
            commits = payload.commits.len(),
            memories = memories_created,
            links = links_created,
            "Processed GitLab push"
        );

        Ok(WebhookResult {
            repository_id: repository.id.clone(),
            commits_processed: payload.commits.len(),
            memories_created,
            links_created,
        })
    }

    /// Check if a commit author should be ignored (e.g., Fold bot commits).
    /// This prevents webhook loops when Fold writes to metadata repos.
    ///
    /// Checks both global patterns and project-specific `ignored_commit_authors`.
    fn should_ignore_author(&self, author: &Option<String>, project: &Project) -> bool {
        let author = match author {
            Some(a) => a.to_lowercase(),
            None => return false,
        };

        // Global bot patterns (always ignored)
        let global_patterns = [
            "fold-meta-bot",  // Fold metadata sync bot
            "fold",           // Fold bot (general)
            "[bot]",          // GitHub bot convention
            "github-actions", // CI/CD
            "dependabot",     // Dependency updates
            "noreply@",       // No-reply emails
        ];

        // Check global patterns
        if global_patterns
            .iter()
            .any(|pattern| author.contains(pattern))
        {
            return true;
        }

        // Check project-specific patterns
        let project_patterns = project.ignored_commit_authors_vec();
        project_patterns
            .iter()
            .any(|pattern| author.contains(&pattern.to_lowercase()))
    }

    /// Process a single commit.
    async fn process_commit(
        &self,
        commit: &CommitInfo,
        repository: &Repository,
        project: &Project,
    ) -> Result<(usize, usize)> {
        let mut memories_created = 0;
        let mut links_created = 0;

        // Skip commits from bot authors to prevent webhook loops
        if self.should_ignore_author(&commit.author, project) {
            debug!(
                sha = %commit.sha,
                author = ?commit.author,
                "Skipping commit from bot author (prevents webhook loop)"
            );
            return Ok((0, 0));
        }

        // Check if commit already processed
        let existing: Option<GitCommit> = sqlx::query_as(
            r#"
            SELECT * FROM git_commits WHERE repository_id = ? AND sha = ?
            "#,
        )
        .bind(&repository.id)
        .bind(&commit.sha)
        .fetch_optional(&self.db)
        .await?;

        if existing.is_some() {
            debug!(sha = %commit.sha, "Commit already processed");
            return Ok((0, 0));
        }

        // Generate commit summary using LLM
        let summary = if self.llm.is_available().await {
            match self.llm.summarize_commit(commit).await {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "Failed to generate commit summary");
                    commit
                        .message
                        .lines()
                        .next()
                        .unwrap_or(&commit.message)
                        .to_string()
                }
            }
        } else {
            commit
                .message
                .lines()
                .next()
                .unwrap_or(&commit.message)
                .to_string()
        };

        // Create commit memory
        let commit_memory = self
            .memory
            .add(
                &project.id,
                &project.slug,
                MemoryCreate {
                    memory_type: MemoryType::Commit,
                    content: summary.clone(),
                    author: commit.author.clone(),
                    title: Some(format!(
                        "[{}] {}",
                        &commit.sha[..7.min(commit.sha.len())],
                        commit.message.lines().next().unwrap_or("")
                    )),
                    tags: vec!["commit".to_string()],
                    source: Some(crate::models::MemorySource::Git),
                    metadata: [
                        ("sha".to_string(), serde_json::json!(commit.sha)),
                        (
                            "repository_id".to_string(),
                            serde_json::json!(repository.id),
                        ),
                        (
                            "insertions".to_string(),
                            serde_json::json!(commit.insertions),
                        ),
                        ("deletions".to_string(), serde_json::json!(commit.deletions)),
                    ]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                },
                false,
            )
            .await?;

        memories_created += 1;

        // Store commit record
        let files_json = serde_json::to_string(&commit.files).ok();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO git_commits (
                id, repository_id, sha, message, author_name, author_email,
                files_changed, insertions, deletions, committed_at, indexed_at,
                summary_memory_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(crate::models::new_id())
        .bind(&repository.id)
        .bind(&commit.sha)
        .bind(&commit.message)
        .bind(&commit.author)
        .bind::<Option<&str>>(None) // author_email
        .bind(&files_json)
        .bind(commit.insertions)
        .bind(commit.deletions)
        .bind(now)
        .bind(now)
        .bind(&commit_memory.id)
        .execute(&self.db)
        .await?;

        // Create links between commit and affected files
        for file in &commit.files {
            // Find existing codebase memory for this file
            let existing_memory: Option<Memory> = sqlx::query_as(
                r#"
                SELECT * FROM memories
                WHERE project_id = ? AND type = 'codebase' AND file_path = ?
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .bind(&project.id)
            .bind(&file.path)
            .fetch_optional(&self.db)
            .await?;

            if let Some(file_memory) = existing_memory {
                // Create link: commit -> file (modifies)
                let link = MemoryLink::new(
                    project.id.clone(),
                    commit_memory.id.clone(),
                    file_memory.id.clone(),
                    crate::models::LinkType::Modifies,
                    "system".to_string(),
                );

                sqlx::query(
                    r#"
                    INSERT INTO memory_links (
                        id, project_id, source_id, target_id, link_type,
                        created_by, change_type, created_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&link.id)
                .bind(&link.project_id)
                .bind(&link.source_id)
                .bind(&link.target_id)
                .bind(&link.link_type)
                .bind(&link.created_by)
                .bind(&file.status)
                .bind(link.created_at)
                .execute(&self.db)
                .await?;

                links_created += 1;
            }
        }

        Ok((memories_created, links_created))
    }

    /// Sync repository history (initial indexing).
    pub async fn sync_history(
        &self,
        repository: &Repository,
        project: &Project,
        limit: u32,
    ) -> Result<WebhookResult> {
        let commits = match repository.provider.as_str() {
            "github" => {
                let gh_commits = self
                    .github
                    .get_commits(
                        &repository.owner,
                        &repository.repo,
                        Some(&repository.branch),
                        None,
                        limit,
                        &repository.access_token,
                    )
                    .await?;

                gh_commits
                    .into_iter()
                    .map(|c| self.github.to_commit_info(c))
                    .collect::<Vec<_>>()
            }
            "gitlab" => {
                let gl_commits = self
                    .gitlab
                    .get_commits(
                        &repository.owner,
                        &repository.repo,
                        Some(&repository.branch),
                        None,
                        limit,
                        &repository.access_token,
                    )
                    .await?;

                let mut result = Vec::new();
                for c in gl_commits {
                    let info = self
                        .gitlab
                        .to_commit_info(
                            &repository.owner,
                            &repository.repo,
                            c,
                            &repository.access_token,
                        )
                        .await?;
                    result.push(info);
                }
                result
            }
            _ => {
                return Err(Error::Validation(format!(
                    "Unknown provider: {}",
                    repository.provider
                )));
            }
        };

        let mut total_memories = 0;
        let mut total_links = 0;

        for commit in &commits {
            let (memories, links) = self.process_commit(commit, repository, project).await?;
            total_memories += memories;
            total_links += links;
        }

        // Update last indexed
        if let Some(latest) = commits.first() {
            self.update_repository_last_indexed(&repository.id, &latest.sha)
                .await?;
        }

        info!(
            repository = %repository.full_name(),
            commits = commits.len(),
            memories = total_memories,
            links = total_links,
            "Synced repository history"
        );

        Ok(WebhookResult {
            repository_id: repository.id.clone(),
            commits_processed: commits.len(),
            memories_created: total_memories,
            links_created: total_links,
        })
    }

    /// Update repository last indexed timestamp and commit SHA.
    async fn update_repository_last_indexed(&self, repository_id: &str, sha: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE repositories
            SET last_indexed_at = datetime('now'),
                last_commit_sha = ?
            WHERE id = ?
            "#,
        )
        .bind(sha)
        .bind(repository_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Get repository by webhook secret.
    pub async fn get_repository_by_secret(&self, secret: &str) -> Result<Option<Repository>> {
        let repo = sqlx::query_as::<_, Repository>(
            r#"
            SELECT * FROM repositories WHERE webhook_secret = ?
            "#,
        )
        .bind(secret)
        .fetch_optional(&self.db)
        .await?;

        Ok(repo)
    }

    /// Get repository by ID.
    pub async fn get_repository(&self, id: &str) -> Result<Option<Repository>> {
        let repo = sqlx::query_as::<_, Repository>(
            r#"
            SELECT * FROM repositories WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?;

        Ok(repo)
    }

    /// Process a push webhook from job queue payload.
    /// Extracts repository info from payload and dispatches to provider-specific handler.
    pub async fn process_push_webhook(&self, payload: &serde_json::Value) -> Result<WebhookResult> {
        // Extract repository ID from payload
        let repo_id = payload
            .get("repository_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::Validation("Missing repository_id in webhook payload".to_string())
            })?;

        // Get repository
        let repo = self
            .get_repository(repo_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Repository not found: {}", repo_id)))?;

        // Get project
        let project: Project = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
            .bind(&repo.project_id)
            .fetch_one(&self.db)
            .await?;

        // Dispatch based on provider
        match repo.provider.as_str() {
            "github" => {
                // Parse GitHub push payload
                let push_payload: GitHubPushPayload =
                    serde_json::from_value(payload.get("data").cloned().unwrap_or_default())
                        .map_err(|e| {
                            Error::Validation(format!("Invalid GitHub push payload: {}", e))
                        })?;

                self.process_github_push(push_payload, &repo, &project)
                    .await
            }
            "gitlab" => {
                // Parse GitLab push payload
                let push_payload: GitLabPushPayload =
                    serde_json::from_value(payload.get("data").cloned().unwrap_or_default())
                        .map_err(|e| {
                            Error::Validation(format!("Invalid GitLab push payload: {}", e))
                        })?;

                self.process_gitlab_push(push_payload, &repo, &project)
                    .await
            }
            provider => Err(Error::Validation(format!("Unknown provider: {}", provider))),
        }
    }

    /// Process a PR/MR webhook from job queue payload.
    /// For now, logs the event and returns success. Full implementation can be added later.
    pub async fn process_pr_webhook(&self, payload: &serde_json::Value) -> Result<WebhookResult> {
        // Extract repository ID from payload
        let repo_id = payload
            .get("repository_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::Validation("Missing repository_id in webhook payload".to_string())
            })?;

        // Get repository
        let repo = self
            .get_repository(repo_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Repository not found: {}", repo_id)))?;

        // Extract PR action and number for logging
        let action = payload
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let pr_number = payload.get("number").and_then(|v| v.as_i64()).unwrap_or(0);

        info!(
            repository = %repo.full_name(),
            action = action,
            pr_number = pr_number,
            "Processed PR webhook"
        );

        // Return success with no processing for now
        // Full PR processing (creating memories for PR descriptions, comments, etc.)
        // can be implemented in a future phase
        Ok(WebhookResult {
            repository_id: repo.id,
            commits_processed: 0,
            memories_created: 0,
            links_created: 0,
        })
    }
}
