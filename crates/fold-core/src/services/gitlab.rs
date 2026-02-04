//! GitLab service for repository operations.
//!
//! Provides API access to GitLab for:
//! - Repository information
//! - File content retrieval
//! - Webhook registration
//! - Commit fetching

use std::time::Duration;

use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{Error, Result};
use crate::models::{CommitFile, CommitInfo};

const GITLAB_API_URL: &str = "https://gitlab.com/api/v4";

/// Service for GitLab API operations.
#[derive(Clone)]
pub struct GitLabService {
    client: Client,
    base_url: String,
}

/// GitLab project info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub visibility: String,
    pub web_url: String,
    pub http_url_to_repo: String,
}

/// GitLab file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub file_name: String,
    pub file_path: String,
    pub size: i64,
    pub encoding: String,
    pub content: String,
    pub content_sha256: String,
    pub ref_name: String,
    pub blob_id: String,
    pub commit_id: String,
}

/// GitLab commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabCommit {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_date: String,
    pub committer_name: String,
    pub committer_email: String,
    pub committed_date: String,
    pub stats: Option<GitLabStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabStats {
    pub additions: i32,
    pub deletions: i32,
    pub total: i32,
}

/// GitLab commit diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabDiff {
    pub old_path: String,
    pub new_path: String,
    pub a_mode: String,
    pub b_mode: String,
    pub diff: String,
    pub new_file: bool,
    pub renamed_file: bool,
    pub deleted_file: bool,
}

/// GitLab webhook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabWebhook {
    pub id: i64,
    pub url: String,
    pub push_events: bool,
    pub merge_requests_events: bool,
    pub tag_push_events: bool,
    pub enable_ssl_verification: bool,
}

/// Webhook creation request
#[derive(Debug, Clone, Serialize)]
struct CreateWebhookRequest {
    url: String,
    token: String,
    push_events: bool,
    merge_requests_events: bool,
    tag_push_events: bool,
    enable_ssl_verification: bool,
}

impl GitLabService {
    /// Create a new GitLab service with default base URL.
    pub fn new() -> Self {
        Self::with_base_url(GITLAB_API_URL)
    }

    /// Create a new GitLab service with custom base URL (for self-hosted).
    pub fn with_base_url(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Fold/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Build headers with authentication.
    fn build_headers(&self, token: &str) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert("PRIVATE-TOKEN", token.parse().unwrap());
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        headers
    }

    /// URL-encode a project path (owner/repo -> owner%2Frepo).
    fn encode_project_path(owner: &str, repo: &str) -> String {
        urlencoding::encode(&format!("{}/{}", owner, repo)).to_string()
    }

    /// Get project information.
    pub async fn get_project(&self, owner: &str, repo: &str, token: &str) -> Result<ProjectInfo> {
        let encoded = Self::encode_project_path(owner, repo);
        let url = format!("{}/projects/{}", self.base_url, encoded);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))
    }

    /// Get file content from repository.
    pub async fn get_file(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        ref_name: Option<&str>,
        token: &str,
    ) -> Result<FileContent> {
        let encoded_project = Self::encode_project_path(owner, repo);
        let encoded_path = urlencoding::encode(path);
        let ref_name = ref_name.unwrap_or("HEAD");

        let url = format!(
            "{}/projects/{}/repository/files/{}?ref={}",
            self.base_url, encoded_project, encoded_path, ref_name
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Err(Error::NotFound(format!("File not found: {}", path)));
            }
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        let mut file_content: FileContent = response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))?;

        // Decode base64 content
        if file_content.encoding == "base64" {
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                file_content.content.replace('\n', ""),
            )
            .map_err(|e| Error::GitLab(format!("Failed to decode content: {}", e)))?;

            file_content.content = String::from_utf8(decoded)
                .map_err(|e| Error::GitLab(format!("Invalid UTF-8 content: {}", e)))?;
        }

        Ok(file_content)
    }

    /// Get commits from repository.
    pub async fn get_commits(
        &self,
        owner: &str,
        repo: &str,
        branch: Option<&str>,
        since: Option<&str>,
        per_page: u32,
        token: &str,
    ) -> Result<Vec<GitLabCommit>> {
        let encoded = Self::encode_project_path(owner, repo);
        let mut url = format!(
            "{}/projects/{}/repository/commits?per_page={}",
            self.base_url, encoded, per_page
        );

        if let Some(branch) = branch {
            url.push_str(&format!("&ref_name={}", branch));
        }

        if let Some(since) = since {
            url.push_str(&format!("&since={}", since));
        }

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))
    }

    /// Get a single commit with full details.
    pub async fn get_commit(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        token: &str,
    ) -> Result<GitLabCommit> {
        let encoded = Self::encode_project_path(owner, repo);
        let url = format!(
            "{}/projects/{}/repository/commits/{}",
            self.base_url, encoded, sha
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))
    }

    /// Get commit diff.
    pub async fn get_commit_diff(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        token: &str,
    ) -> Result<Vec<GitLabDiff>> {
        let encoded = Self::encode_project_path(owner, repo);
        let url = format!(
            "{}/projects/{}/repository/commits/{}/diff",
            self.base_url, encoded, sha
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))
    }

    /// Convert GitLab commit to internal CommitInfo.
    pub async fn to_commit_info(
        &self,
        owner: &str,
        repo: &str,
        commit: GitLabCommit,
        token: &str,
    ) -> Result<CommitInfo> {
        // Get diff to get file changes
        let diffs = self.get_commit_diff(owner, repo, &commit.id, token).await?;

        let files: Vec<CommitFile> = diffs
            .into_iter()
            .map(|d| CommitFile {
                path: d.new_path,
                status: if d.new_file {
                    "added".to_string()
                } else if d.deleted_file {
                    "removed".to_string()
                } else if d.renamed_file {
                    "renamed".to_string()
                } else {
                    "modified".to_string()
                },
                patch: Some(d.diff),
            })
            .collect();

        Ok(CommitInfo {
            sha: commit.id,
            message: commit.message,
            author: Some(commit.author_name),
            insertions: commit.stats.as_ref().map(|s| s.additions).unwrap_or(0),
            deletions: commit.stats.as_ref().map(|s| s.deletions).unwrap_or(0),
            files,
        })
    }

    /// Register a webhook on a project.
    pub async fn register_webhook(
        &self,
        owner: &str,
        repo: &str,
        webhook_url: &str,
        secret: &str,
        token: &str,
    ) -> Result<GitLabWebhook> {
        let encoded = Self::encode_project_path(owner, repo);
        let url = format!("{}/projects/{}/hooks", self.base_url, encoded);

        let request = CreateWebhookRequest {
            url: webhook_url.to_string(),
            token: secret.to_string(),
            push_events: true,
            merge_requests_events: true,
            tag_push_events: true,
            enable_ssl_verification: true,
        };

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers(token))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        let webhook: GitLabWebhook = response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))?;

        info!(
            owner = owner,
            repo = repo,
            webhook_id = webhook.id,
            "Registered GitLab webhook"
        );

        Ok(webhook)
    }

    /// Delete a webhook from a project.
    pub async fn delete_webhook(
        &self,
        owner: &str,
        repo: &str,
        webhook_id: i64,
        token: &str,
    ) -> Result<()> {
        let encoded = Self::encode_project_path(owner, repo);
        let url = format!(
            "{}/projects/{}/hooks/{}",
            self.base_url, encoded, webhook_id
        );

        let response = self
            .client
            .delete(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() && response.status().as_u16() != 404 {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        info!(
            owner = owner,
            repo = repo,
            webhook_id = webhook_id,
            "Deleted GitLab webhook"
        );

        Ok(())
    }

    /// List webhooks on a project.
    pub async fn list_webhooks(
        &self,
        owner: &str,
        repo: &str,
        token: &str,
    ) -> Result<Vec<GitLabWebhook>> {
        let encoded = Self::encode_project_path(owner, repo);
        let url = format!("{}/projects/{}/hooks", self.base_url, encoded);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitLab(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitLab(format!(
                "GitLab API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitLab(format!("Failed to parse response: {}", e)))
    }

    /// Verify webhook token.
    pub fn verify_token(&self, provided_token: &str, expected_token: &str) -> bool {
        // GitLab uses a simple token comparison
        provided_token == expected_token
    }
}

impl Default for GitLabService {
    fn default() -> Self {
        Self::new()
    }
}
