//! GitHub service for repository operations.
//!
//! Provides API access to GitHub for:
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

const GITHUB_API_URL: &str = "https://api.github.com";

/// Service for GitHub API operations.
#[derive(Clone)]
pub struct GitHubService {
    client: Client,
}

/// GitHub repository info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub private: bool,
    pub html_url: String,
    pub clone_url: String,
}

/// GitHub file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub size: i64,
    pub content: Option<String>,
    pub encoding: Option<String>,
}

/// GitHub commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommit {
    pub sha: String,
    pub commit: GitHubCommitDetails,
    pub author: Option<GitHubUser>,
    pub stats: Option<GitHubStats>,
    pub files: Option<Vec<GitHubFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommitDetails {
    pub message: String,
    pub author: GitHubCommitAuthor,
    pub committer: GitHubCommitAuthor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubCommitAuthor {
    pub name: String,
    pub email: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub id: i64,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubStats {
    pub additions: i32,
    pub deletions: i32,
    pub total: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubFile {
    pub filename: String,
    pub status: String,
    pub additions: i32,
    pub deletions: i32,
    pub patch: Option<String>,
}

/// GitHub webhook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubWebhook {
    pub id: i64,
    pub name: String,
    pub active: bool,
    pub events: Vec<String>,
    pub config: GitHubWebhookConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubWebhookConfig {
    pub url: String,
    pub content_type: String,
    pub insecure_ssl: String,
}

/// Webhook creation request
#[derive(Debug, Clone, Serialize)]
struct CreateWebhookRequest {
    name: String,
    active: bool,
    events: Vec<String>,
    config: WebhookConfigRequest,
}

#[derive(Debug, Clone, Serialize)]
struct WebhookConfigRequest {
    url: String,
    content_type: String,
    secret: String,
    insecure_ssl: String,
}

impl GitHubService {
    /// Create a new GitHub service.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Fold/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Build headers with authentication.
    fn build_headers(&self, token: &str) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", token).parse().unwrap(),
        );
        headers.insert(
            header::ACCEPT,
            "application/vnd.github+json".parse().unwrap(),
        );
        headers.insert("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
        headers
    }

    /// Get repository information.
    pub async fn get_repo(&self, owner: &str, repo: &str, token: &str) -> Result<RepoInfo> {
        let url = format!("{}/repos/{}/{}", GITHUB_API_URL, owner, repo);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))
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
        let mut url = format!(
            "{}/repos/{}/{}/contents/{}",
            GITHUB_API_URL, owner, repo, path
        );

        if let Some(ref_name) = ref_name {
            url.push_str(&format!("?ref={}", ref_name));
        }

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Err(Error::NotFound(format!("File not found: {}", path)));
            }
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        let mut file_content: FileContent = response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))?;

        // Decode base64 content
        if let (Some(content), Some(encoding)) = (&file_content.content, &file_content.encoding) {
            if encoding == "base64" {
                let decoded = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    content.replace('\n', ""),
                )
                .map_err(|e| Error::GitHub(format!("Failed to decode content: {}", e)))?;

                file_content.content = Some(
                    String::from_utf8(decoded)
                        .map_err(|e| Error::GitHub(format!("Invalid UTF-8 content: {}", e)))?,
                );
            }
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
    ) -> Result<Vec<GitHubCommit>> {
        let mut url = format!(
            "{}/repos/{}/{}/commits?per_page={}",
            GITHUB_API_URL, owner, repo, per_page
        );

        if let Some(branch) = branch {
            url.push_str(&format!("&sha={}", branch));
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
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))
    }

    /// Get a single commit with full details.
    pub async fn get_commit(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        token: &str,
    ) -> Result<GitHubCommit> {
        let url = format!(
            "{}/repos/{}/{}/commits/{}",
            GITHUB_API_URL, owner, repo, sha
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))
    }

    /// Convert GitHub commit to internal CommitInfo.
    pub fn to_commit_info(&self, commit: GitHubCommit) -> CommitInfo {
        CommitInfo {
            sha: commit.sha,
            message: commit.commit.message,
            author: commit.author.map(|a| a.login),
            insertions: commit.stats.as_ref().map(|s| s.additions).unwrap_or(0),
            deletions: commit.stats.as_ref().map(|s| s.deletions).unwrap_or(0),
            files: commit
                .files
                .unwrap_or_default()
                .into_iter()
                .map(|f| CommitFile {
                    path: f.filename,
                    status: f.status,
                    patch: f.patch,
                })
                .collect(),
        }
    }

    /// Register a webhook on a repository.
    pub async fn register_webhook(
        &self,
        owner: &str,
        repo: &str,
        webhook_url: &str,
        secret: &str,
        events: Vec<String>,
        token: &str,
    ) -> Result<GitHubWebhook> {
        let url = format!("{}/repos/{}/{}/hooks", GITHUB_API_URL, owner, repo);

        let request = CreateWebhookRequest {
            name: "web".to_string(),
            active: true,
            events,
            config: WebhookConfigRequest {
                url: webhook_url.to_string(),
                content_type: "json".to_string(),
                secret: secret.to_string(),
                insecure_ssl: "0".to_string(),
            },
        };

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers(token))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        let webhook: GitHubWebhook = response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))?;

        info!(
            owner = owner,
            repo = repo,
            webhook_id = webhook.id,
            "Registered GitHub webhook"
        );

        Ok(webhook)
    }

    /// Delete a webhook from a repository.
    pub async fn delete_webhook(
        &self,
        owner: &str,
        repo: &str,
        webhook_id: i64,
        token: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/repos/{}/{}/hooks/{}",
            GITHUB_API_URL, owner, repo, webhook_id
        );

        let response = self
            .client
            .delete(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() && response.status().as_u16() != 404 {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        info!(
            owner = owner,
            repo = repo,
            webhook_id = webhook_id,
            "Deleted GitHub webhook"
        );

        Ok(())
    }

    /// List webhooks on a repository.
    pub async fn list_webhooks(
        &self,
        owner: &str,
        repo: &str,
        token: &str,
    ) -> Result<Vec<GitHubWebhook>> {
        let url = format!("{}/repos/{}/{}/hooks", GITHUB_API_URL, owner, repo);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))
    }

    /// Get files changed in a pull request.
    pub async fn get_pull_request_files(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u32,
        token: &str,
    ) -> Result<Vec<GitHubFile>> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}/files?per_page=100",
            GITHUB_API_URL, owner, repo, pr_number
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers(token))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::GitHub(format!(
                "GitHub API error {}: {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| Error::GitHub(format!("Failed to parse response: {}", e)))
    }

    /// Verify webhook signature.
    pub fn verify_signature(&self, payload: &[u8], signature: &str, secret: &str) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let signature = match signature.strip_prefix("sha256=") {
            Some(s) => s,
            None => return false,
        };

        let signature_bytes = match hex::decode(signature) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(payload);

        mac.verify_slice(&signature_bytes).is_ok()
    }
}

impl Default for GitHubService {
    fn default() -> Self {
        Self::new()
    }
}
