//! GitHub implementation of FileSourceProvider.
//!
//! Wraps the existing GitHubService to provide a unified interface
//! for file source operations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::{debug, info};

use super::{
    ChangeDetectionResult, ChangeEvent, CommitFile, CommitStats, FileChangeStatus, FileContent,
    FileInfo, FileSourceProvider, NotificationConfig, NotificationType, PullRequestAction,
    SourceConfig, SourceInfo,
};
use crate::error::{Error, Result};
use crate::services::GitHubService;

/// GitHub file source provider.
///
/// Wraps the GitHubService to implement the FileSourceProvider trait.
pub struct GitHubFileSource {
    github: GitHubService,
}

impl GitHubFileSource {
    /// Create a new GitHub file source provider.
    pub fn new() -> Self {
        Self {
            github: GitHubService::new(),
        }
    }

    /// Create with an existing GitHubService.
    pub fn with_service(github: GitHubService) -> Self {
        Self { github }
    }

    /// Get owner and repo from source info.
    fn owner_repo<'a>(&self, source: &'a SourceInfo) -> (&'a str, &'a str) {
        let owner = source.owner.as_deref().unwrap_or("");
        let repo = &source.name;
        (owner, repo)
    }
}

impl Default for GitHubFileSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSourceProvider for GitHubFileSource {
    fn provider_type(&self) -> &'static str {
        "github"
    }

    fn display_name(&self) -> &'static str {
        "GitHub"
    }

    fn supports_webhooks(&self) -> bool {
        true
    }

    async fn connect(&self, config: SourceConfig, token: &str) -> Result<SourceInfo> {
        let owner = config.owner.as_deref().ok_or_else(|| {
            Error::Validation("GitHub requires owner in config".to_string())
        })?;

        let repo_info = self.github.get_repo(owner, &config.name, token).await?;

        Ok(SourceInfo {
            id: repo_info.id.to_string(),
            name: repo_info.name,
            full_name: repo_info.full_name,
            url: Some(repo_info.html_url),
            default_version: Some(config.branch.unwrap_or(repo_info.default_branch)),
            is_private: repo_info.private,
            owner: Some(owner.to_string()),
            metadata: serde_json::json!({
                "clone_url": repo_info.clone_url,
                "description": repo_info.description,
            }),
        })
    }

    async fn disconnect(&self, source: &SourceInfo, _token: &str) -> Result<()> {
        // Webhook cleanup is handled separately via unregister_notifications
        info!(
            source_id = %source.id,
            source_name = %source.full_name,
            "Disconnected GitHub source"
        );
        Ok(())
    }

    async fn get_file(
        &self,
        source: &SourceInfo,
        path: &str,
        version: Option<&str>,
        token: &str,
    ) -> Result<FileContent> {
        let (owner, repo) = self.owner_repo(source);
        let ref_name = version.or(source.default_version.as_deref());

        let file = self
            .github
            .get_file(owner, repo, path, ref_name, token)
            .await?;

        Ok(FileContent {
            path: file.path.clone(),
            name: file.name,
            content: file.content,
            bytes: None,
            hash: Some(file.sha),
            size: file.size,
            mime_type: None,
            modified_at: None,
        })
    }

    async fn list_files(
        &self,
        source: &SourceInfo,
        prefix: Option<&str>,
        version: Option<&str>,
        token: &str,
    ) -> Result<Vec<FileInfo>> {
        let (owner, repo) = self.owner_repo(source);
        let ref_name = version.or(source.default_version.as_deref());
        let path = prefix.unwrap_or("");

        // GitHub's contents API returns directory listings
        // For now, we'll use a simpler approach - this could be enhanced
        // with the Git Trees API for full recursive listing
        let file = self
            .github
            .get_file(owner, repo, path, ref_name, token)
            .await;

        match file {
            Ok(f) => {
                // Single file
                Ok(vec![FileInfo {
                    path: f.path,
                    name: f.name,
                    is_directory: false,
                    size: f.size,
                    hash: Some(f.sha),
                    modified_at: None,
                }])
            }
            Err(_) => {
                // Directory listing not directly supported via simple contents API
                // Would need to use Git Trees API for full recursive listing
                debug!(
                    owner = %owner,
                    repo = %repo,
                    path = %path,
                    "Directory listing requires Git Trees API"
                );
                Ok(vec![])
            }
        }
    }

    async fn register_notifications(
        &self,
        source: &SourceInfo,
        callback_url: &str,
        secret: &str,
        token: &str,
    ) -> Result<NotificationConfig> {
        let (owner, repo) = self.owner_repo(source);

        let webhook = self
            .github
            .register_webhook(
                owner,
                repo,
                callback_url,
                secret,
                vec!["push".to_string(), "pull_request".to_string()],
                token,
            )
            .await?;

        Ok(NotificationConfig {
            notification_type: NotificationType::Webhook,
            notification_id: webhook.id.to_string(),
            events: webhook.events,
            poll_interval_secs: None,
            expires_at: None,
        })
    }

    async fn unregister_notifications(
        &self,
        source: &SourceInfo,
        notification_id: &str,
        token: &str,
    ) -> Result<()> {
        let (owner, repo) = self.owner_repo(source);

        let webhook_id: i64 = notification_id.parse().map_err(|_| {
            Error::Validation(format!("Invalid webhook ID: {}", notification_id))
        })?;

        self.github
            .delete_webhook(owner, repo, webhook_id, token)
            .await
    }

    fn verify_notification(&self, payload: &[u8], signature: &str, secret: &str) -> bool {
        self.github.verify_signature(payload, signature, secret)
    }

    fn parse_notification(&self, event_type: &str, payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        let payload_str = std::str::from_utf8(payload)
            .map_err(|e| Error::Validation(format!("Invalid UTF-8 payload: {}", e)))?;

        match event_type {
            "push" => self.parse_push_event(payload_str),
            "pull_request" => self.parse_pull_request_event(payload_str),
            _ => {
                debug!(event_type = %event_type, "Unhandled GitHub event type");
                Ok(vec![])
            }
        }
    }

    fn supported_events(&self) -> Vec<&'static str> {
        vec!["push", "pull_request", "create", "delete"]
    }

    async fn detect_changes(
        &self,
        source: &SourceInfo,
        cursor: Option<&str>,
        token: &str,
    ) -> Result<ChangeDetectionResult> {
        let (owner, repo) = self.owner_repo(source);
        let branch = source.default_version.as_deref();

        // Use commits API with since parameter
        let since = cursor;

        let commits = self
            .github
            .get_commits(owner, repo, branch, since, 100, token)
            .await?;

        let events: Vec<ChangeEvent> = commits
            .into_iter()
            .map(|c| {
                let timestamp = DateTime::parse_from_rfc3339(&c.commit.author.date)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                ChangeEvent::Commit {
                    sha: c.sha,
                    message: c.commit.message,
                    author: c.commit.author.name,
                    author_email: Some(c.commit.author.email),
                    timestamp,
                    files: c
                        .files
                        .unwrap_or_default()
                        .into_iter()
                        .map(|f| CommitFile {
                            path: f.filename,
                            status: FileChangeStatus::from_str(&f.status),
                            previous_path: None,
                            patch: f.patch,
                            additions: f.additions,
                            deletions: f.deletions,
                        })
                        .collect(),
                    stats: c.stats.map(|s| CommitStats {
                        additions: s.additions,
                        deletions: s.deletions,
                        total: s.total,
                    }),
                }
            })
            .collect();

        // Use the latest commit timestamp as next cursor
        let next_cursor = events.first().and_then(|e| {
            if let ChangeEvent::Commit { timestamp, .. } = e {
                Some(timestamp.to_rfc3339())
            } else {
                None
            }
        });

        Ok(ChangeDetectionResult {
            events,
            next_cursor,
            has_more: false,
        })
    }
}

impl GitHubFileSource {
    /// Parse a GitHub push webhook payload.
    fn parse_push_event(&self, payload: &str) -> Result<Vec<ChangeEvent>> {
        #[derive(serde::Deserialize)]
        struct PushPayload {
            #[serde(rename = "ref")]
            git_ref: Option<String>,
            after: Option<String>,
            commits: Option<Vec<PushCommit>>,
            deleted: Option<bool>,
        }

        #[derive(serde::Deserialize)]
        struct PushCommit {
            id: String,
            message: String,
            timestamp: String,
            author: PushAuthor,
            added: Option<Vec<String>>,
            removed: Option<Vec<String>>,
            modified: Option<Vec<String>>,
        }

        #[derive(serde::Deserialize)]
        struct PushAuthor {
            name: String,
            email: Option<String>,
        }

        let payload: PushPayload = serde_json::from_str(payload)
            .map_err(|e| Error::Validation(format!("Invalid push payload: {}", e)))?;

        // Check for branch deletion
        if payload.deleted.unwrap_or(false) {
            let branch = payload
                .git_ref
                .as_deref()
                .and_then(|r| r.strip_prefix("refs/heads/"))
                .unwrap_or("unknown");

            return Ok(vec![ChangeEvent::BranchDeleted {
                branch: branch.to_string(),
            }]);
        }

        let mut events = Vec::new();

        if let Some(commits) = payload.commits {
            for commit in commits {
                let timestamp = DateTime::parse_from_rfc3339(&commit.timestamp)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let mut files = Vec::new();

                // Added files
                for path in commit.added.unwrap_or_default() {
                    files.push(CommitFile {
                        path,
                        status: FileChangeStatus::Added,
                        previous_path: None,
                        patch: None,
                        additions: 0,
                        deletions: 0,
                    });
                }

                // Modified files
                for path in commit.modified.unwrap_or_default() {
                    files.push(CommitFile {
                        path,
                        status: FileChangeStatus::Modified,
                        previous_path: None,
                        patch: None,
                        additions: 0,
                        deletions: 0,
                    });
                }

                // Deleted files
                for path in commit.removed.unwrap_or_default() {
                    files.push(CommitFile {
                        path,
                        status: FileChangeStatus::Deleted,
                        previous_path: None,
                        patch: None,
                        additions: 0,
                        deletions: 0,
                    });
                }

                events.push(ChangeEvent::Commit {
                    sha: commit.id,
                    message: commit.message,
                    author: commit.author.name,
                    author_email: commit.author.email,
                    timestamp,
                    files,
                    stats: None,
                });
            }
        }

        Ok(events)
    }

    /// Parse a GitHub pull_request webhook payload.
    fn parse_pull_request_event(&self, payload: &str) -> Result<Vec<ChangeEvent>> {
        #[derive(serde::Deserialize)]
        struct PrPayload {
            action: String,
            pull_request: PrDetails,
        }

        #[derive(serde::Deserialize)]
        struct PrDetails {
            number: u32,
            title: String,
            merged: Option<bool>,
            user: PrUser,
            head: PrBranch,
            base: PrBranch,
        }

        #[derive(serde::Deserialize)]
        struct PrUser {
            login: String,
        }

        #[derive(serde::Deserialize)]
        struct PrBranch {
            #[serde(rename = "ref")]
            branch: String,
        }

        let payload: PrPayload = serde_json::from_str(payload)
            .map_err(|e| Error::Validation(format!("Invalid pull_request payload: {}", e)))?;

        let action = PullRequestAction::from_str(&payload.action);
        let is_merged = payload.pull_request.merged.unwrap_or(false)
            || matches!(action, PullRequestAction::Merged);

        Ok(vec![ChangeEvent::PullRequest {
            number: payload.pull_request.number,
            action,
            title: payload.pull_request.title,
            author: payload.pull_request.user.login,
            source_branch: Some(payload.pull_request.head.branch),
            target_branch: Some(payload.pull_request.base.branch),
            is_merged,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_push_event() {
        let provider = GitHubFileSource::new();

        let payload = r#"{
            "ref": "refs/heads/main",
            "after": "abc123",
            "commits": [{
                "id": "abc123",
                "message": "Test commit",
                "timestamp": "2024-01-15T10:00:00Z",
                "author": {"name": "Test User", "email": "test@example.com"},
                "added": ["new-file.rs"],
                "modified": ["existing.rs"],
                "removed": []
            }]
        }"#;

        let events = provider.parse_push_event(payload).unwrap();
        assert_eq!(events.len(), 1);

        if let ChangeEvent::Commit { sha, message, files, .. } = &events[0] {
            assert_eq!(sha, "abc123");
            assert_eq!(message, "Test commit");
            assert_eq!(files.len(), 2);
        } else {
            panic!("Expected Commit event");
        }
    }

    #[test]
    fn test_parse_pr_event() {
        let provider = GitHubFileSource::new();

        let payload = r#"{
            "action": "opened",
            "pull_request": {
                "number": 42,
                "title": "Add feature X",
                "merged": false,
                "user": {"login": "testuser"},
                "head": {"ref": "feature-x"},
                "base": {"ref": "main"}
            }
        }"#;

        let events = provider.parse_pull_request_event(payload).unwrap();
        assert_eq!(events.len(), 1);

        if let ChangeEvent::PullRequest { number, action, title, .. } = &events[0] {
            assert_eq!(*number, 42);
            assert_eq!(*action, PullRequestAction::Opened);
            assert_eq!(title, "Add feature X");
        } else {
            panic!("Expected PullRequest event");
        }
    }
}
