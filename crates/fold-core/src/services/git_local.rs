//! Local git operations service.
//!
//! Handles cloning and pulling repositories locally for efficient indexing.
//! Uses git2 library for native git operations.

use std::path::{Path, PathBuf};

use git2::{Cred, FetchOptions, RemoteCallbacks, Repository as GitRepo};
use tracing::{debug, info, warn};

use crate::error::{Error, Result};

/// Default base directory for cloned repositories
const DEFAULT_REPOS_DIR: &str = "data/repos";

/// Service for local git operations.
#[derive(Clone)]
pub struct GitLocalService {
    /// Base directory for cloned repositories
    base_dir: PathBuf,
}

impl GitLocalService {
    /// Create a new git local service with the default repos directory.
    pub fn new() -> Self {
        Self {
            base_dir: PathBuf::from(DEFAULT_REPOS_DIR),
        }
    }

    /// Create a new git local service with a custom base directory.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Get the local path for a repository clone.
    /// Path format: {base_dir}/{project_slug}/{owner}-{repo}
    pub fn get_repo_path(&self, project_slug: &str, owner: &str, repo: &str) -> PathBuf {
        self.base_dir
            .join(project_slug)
            .join(format!("{}-{}", owner, repo))
    }

    /// Clone a repository to a local path.
    /// Returns the path to the cloned repository.
    pub async fn clone_repo(
        &self,
        project_slug: &str,
        owner: &str,
        repo: &str,
        branch: &str,
        token: &str,
        provider: &str,
    ) -> Result<PathBuf> {
        let local_path = self.get_repo_path(project_slug, owner, repo);

        // If directory exists and is a valid git repo, just pull instead
        if local_path.exists() {
            if Self::is_valid_repo(&local_path) {
                info!(
                    path = %local_path.display(),
                    "Repository already exists, pulling instead"
                );
                self.pull_repo(&local_path, branch, token, provider).await?;
                return Ok(local_path);
            } else {
                // Directory exists but is not a valid git repo, remove it
                warn!(
                    path = %local_path.display(),
                    "Invalid repository directory, removing"
                );
                tokio::fs::remove_dir_all(&local_path).await.map_err(|e| {
                    Error::Internal(format!("Failed to remove invalid repo dir: {}", e))
                })?;
            }
        }

        // Create parent directories
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Error::Internal(format!("Failed to create parent directory: {}", e))
            })?;
        }

        // Build the repository URL
        let url = match provider {
            "github" => format!("https://github.com/{}/{}.git", owner, repo),
            "gitlab" => format!("https://gitlab.com/{}/{}.git", owner, repo),
            _ => return Err(Error::Validation(format!("Unknown provider: {}", provider))),
        };

        info!(
            url = %url,
            path = %local_path.display(),
            branch,
            "Cloning repository"
        );

        // Clone in a blocking task since git2 is synchronous
        let local_path_clone = local_path.clone();
        let token = token.to_string();
        let branch = branch.to_string();

        tokio::task::spawn_blocking(move || {
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                // Use token as password with x-access-token as username (works for GitHub)
                Cred::userpass_plaintext("x-access-token", &token)
            });

            let mut fetch_opts = FetchOptions::new();
            fetch_opts.remote_callbacks(callbacks);

            let mut builder = git2::build::RepoBuilder::new();
            builder.fetch_options(fetch_opts);
            builder.branch(&branch);

            builder
                .clone(&url, &local_path_clone)
                .map_err(|e| Error::Internal(format!("Failed to clone repository: {}", e)))?;

            Ok::<PathBuf, Error>(local_path_clone)
        })
        .await
        .map_err(|e| Error::Internal(format!("Clone task failed: {}", e)))?
    }

    /// Pull the latest changes for a repository.
    pub async fn pull_repo(
        &self,
        local_path: &Path,
        branch: &str,
        token: &str,
        _provider: &str,
    ) -> Result<()> {
        let token = token.to_string();
        let branch = branch.to_string();
        let repo_path = local_path.to_path_buf();

        info!(
            path = %repo_path.display(),
            branch,
            "Pulling repository"
        );

        tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&repo_path)
                .map_err(|e| Error::Internal(format!("Failed to open repository: {}", e)))?;

            // Set up callbacks for authentication
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token)
            });

            let mut fetch_opts = FetchOptions::new();
            fetch_opts.remote_callbacks(callbacks);

            // Find origin remote
            let mut remote = repo
                .find_remote("origin")
                .map_err(|e| Error::Internal(format!("Failed to find origin remote: {}", e)))?;

            // Fetch the branch
            remote
                .fetch(&[&branch], Some(&mut fetch_opts), None)
                .map_err(|e| Error::Internal(format!("Failed to fetch: {}", e)))?;

            // Get the fetch head
            let fetch_head = repo
                .find_reference("FETCH_HEAD")
                .map_err(|e| Error::Internal(format!("Failed to find FETCH_HEAD: {}", e)))?;

            let fetch_commit = repo
                .reference_to_annotated_commit(&fetch_head)
                .map_err(|e| Error::Internal(format!("Failed to get fetch commit: {}", e)))?;

            // Do a fast-forward merge
            let (analysis, _) = repo
                .merge_analysis(&[&fetch_commit])
                .map_err(|e| Error::Internal(format!("Failed to analyze merge: {}", e)))?;

            if analysis.is_up_to_date() {
                debug!("Repository is up to date");
                return Ok(());
            }

            if analysis.is_fast_forward() {
                // Get the reference for the branch
                let refname = format!("refs/heads/{}", branch);
                let mut reference = repo
                    .find_reference(&refname)
                    .map_err(|e| Error::Internal(format!("Failed to find reference: {}", e)))?;

                // Fast-forward
                reference
                    .set_target(fetch_commit.id(), "Fast-forward")
                    .map_err(|e| Error::Internal(format!("Failed to fast-forward: {}", e)))?;

                // Checkout the updated tree
                repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                    .map_err(|e| Error::Internal(format!("Failed to checkout: {}", e)))?;

                info!("Fast-forwarded to latest commit");
            } else {
                // For non-fast-forward cases, just reset to the remote branch
                // This discards local changes, which is fine for an indexing clone
                let obj = fetch_commit.id();
                let commit = repo
                    .find_commit(obj)
                    .map_err(|e| Error::Internal(format!("Failed to find commit: {}", e)))?;

                repo.reset(commit.as_object(), git2::ResetType::Hard, None)
                    .map_err(|e| Error::Internal(format!("Failed to reset: {}", e)))?;

                info!("Reset to latest commit (non-fast-forward)");
            }

            Ok(())
        })
        .await
        .map_err(|e| Error::Internal(format!("Pull task failed: {}", e)))?
    }

    /// Get the current HEAD commit SHA.
    pub async fn get_head_sha(&self, local_path: &Path) -> Result<String> {
        let repo_path = local_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&repo_path)
                .map_err(|e| Error::Internal(format!("Failed to open repository: {}", e)))?;

            let head = repo
                .head()
                .map_err(|e| Error::Internal(format!("Failed to get HEAD: {}", e)))?;

            let commit = head
                .peel_to_commit()
                .map_err(|e| Error::Internal(format!("Failed to get commit: {}", e)))?;

            Ok(commit.id().to_string())
        })
        .await
        .map_err(|e| Error::Internal(format!("Get HEAD task failed: {}", e)))?
    }

    /// Check if a path is a valid git repository.
    pub fn is_valid_repo(path: &Path) -> bool {
        GitRepo::open(path).is_ok()
    }
}

impl Default for GitLocalService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_repo_path() {
        let service = GitLocalService::new();
        let path = service.get_repo_path("my-project", "owner", "repo");
        assert!(path.ends_with("my-project/owner-repo"));
    }

    #[test]
    fn test_get_repo_path_custom_base() {
        let service = GitLocalService::with_base_dir("/custom/path");
        let path = service.get_repo_path("proj", "org", "name");
        assert_eq!(path, PathBuf::from("/custom/path/proj/org-name"));
    }
}
