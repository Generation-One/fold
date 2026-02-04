//! Git operations for Fold.
//!
//! This crate provides local git operations for cloning and pulling repositories.
//! Uses the `git2` library for native git operations.
//!
//! # Example
//!
//! ```rust,no_run
//! use fold_git::GitLocalService;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), fold_git::Error> {
//!     let service = GitLocalService::new();
//!
//!     // Clone a repository
//!     let path = service.clone_repo(
//!         "my-project",
//!         "owner",
//!         "repo",
//!         "main",
//!         "token",
//!         "github",
//!     ).await?;
//!
//!     // Get the current HEAD SHA
//!     let sha = service.get_head_sha(&path).await?;
//!     println!("HEAD: {}", sha);
//!
//!     Ok(())
//! }
//! ```

use std::path::{Path, PathBuf};

use git2::{Cred, FetchOptions, RemoteCallbacks, Repository as GitRepo};
use tracing::{debug, info, warn};

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during git operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Git operation failed.
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    /// IO operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Task join failed.
    #[error("Task join error: {0}")]
    Join(#[from] tokio::task::JoinError),

    /// Invalid provider.
    #[error("Unknown provider: {0}")]
    UnknownProvider(String),

    /// Internal error.
    #[error("{0}")]
    Internal(String),
}

/// Result type for git operations.
pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Service
// ============================================================================

/// Default base directory for cloned repositories.
const DEFAULT_REPOS_DIR: &str = "data/repos";

/// Service for local git operations.
///
/// Handles cloning and pulling repositories locally for efficient indexing.
/// Uses the `git2` library for native git operations.
#[derive(Clone)]
pub struct GitLocalService {
    /// Base directory for cloned repositories.
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

    /// Get the base directory for repositories.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Get the local path for a repository clone.
    ///
    /// Path format: `{base_dir}/{project_slug}/{owner}-{repo}`
    pub fn get_repo_path(&self, project_slug: &str, owner: &str, repo: &str) -> PathBuf {
        self.base_dir
            .join(project_slug)
            .join(format!("{}-{}", owner, repo))
    }

    /// Clone a repository to a local path.
    ///
    /// Returns the path to the cloned repository. If the repository already exists
    /// and is valid, it will pull the latest changes instead.
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
                tokio::fs::remove_dir_all(&local_path).await?;
            }
        }

        // Create parent directories
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Build the repository URL
        let url = match provider {
            "github" => format!("https://github.com/{}/{}.git", owner, repo),
            "gitlab" => format!("https://gitlab.com/{}/{}.git", owner, repo),
            _ => return Err(Error::UnknownProvider(provider.to_string())),
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

            builder.clone(&url, &local_path_clone)?;

            Ok::<PathBuf, Error>(local_path_clone)
        })
        .await?
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
            let repo = GitRepo::open(&repo_path)?;

            // Set up callbacks for authentication
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token)
            });

            let mut fetch_opts = FetchOptions::new();
            fetch_opts.remote_callbacks(callbacks);

            // Find origin remote
            let mut remote = repo.find_remote("origin")?;

            // Fetch the branch
            remote.fetch(&[&branch], Some(&mut fetch_opts), None)?;

            // Get the fetch head
            let fetch_head = repo.find_reference("FETCH_HEAD")?;

            let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;

            // Do a fast-forward merge
            let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

            if analysis.is_up_to_date() {
                debug!("Repository is up to date");
                return Ok(());
            }

            if analysis.is_fast_forward() {
                // Get the reference for the branch
                let refname = format!("refs/heads/{}", branch);
                let mut reference = repo.find_reference(&refname)?;

                // Fast-forward
                reference.set_target(fetch_commit.id(), "Fast-forward")?;

                // Checkout the updated tree
                repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;

                info!("Fast-forwarded to latest commit");
            } else {
                // For non-fast-forward cases, just reset to the remote branch
                // This discards local changes, which is fine for an indexing clone
                let obj = fetch_commit.id();
                let commit = repo.find_commit(obj)?;

                repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;

                info!("Reset to latest commit (non-fast-forward)");
            }

            Ok(())
        })
        .await?
    }

    /// Get the current HEAD commit SHA.
    pub async fn get_head_sha(&self, local_path: &Path) -> Result<String> {
        let repo_path = local_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&repo_path)?;
            let head = repo.head()?;
            let commit = head.peel_to_commit()?;
            Ok(commit.id().to_string())
        })
        .await?
    }

    /// Check if a path is a valid git repository.
    pub fn is_valid_repo(path: &Path) -> bool {
        GitRepo::open(path).is_ok()
    }

    /// Check if a directory is a git repository (has .git folder).
    pub fn is_git_repo(path: &Path) -> bool {
        path.join(".git").exists()
    }
}

impl Default for GitLocalService {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

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

    #[test]
    fn test_base_dir() {
        let service = GitLocalService::new();
        assert_eq!(service.base_dir(), Path::new(DEFAULT_REPOS_DIR));

        let custom = GitLocalService::with_base_dir("/tmp/repos");
        assert_eq!(custom.base_dir(), Path::new("/tmp/repos"));
    }

    #[test]
    fn test_default() {
        let service = GitLocalService::default();
        assert_eq!(service.base_dir(), Path::new(DEFAULT_REPOS_DIR));
    }

    #[test]
    fn test_is_git_repo() {
        // Current directory should not be a git repo in tests
        let non_git = std::env::temp_dir();
        // This might or might not be a git repo depending on the environment
        // Just test that the function doesn't panic
        let _ = GitLocalService::is_git_repo(&non_git);
    }
}
