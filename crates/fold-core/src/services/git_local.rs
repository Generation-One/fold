//! Local git operations service.
//!
//! Re-exports from the `fold-git` crate for local git operations.
//! This module provides a wrapper for integration with fold-core's error types.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

// Re-export the GitLocalService from fold-git
pub use fold_git::GitLocalService as FoldGitLocalService;

/// Service for local git operations.
///
/// This is a wrapper around `fold_git::GitLocalService` that converts errors
/// to fold-core's error types.
#[derive(Clone)]
pub struct GitLocalService {
    inner: FoldGitLocalService,
}

impl GitLocalService {
    /// Create a new git local service with the default repos directory.
    pub fn new() -> Self {
        Self {
            inner: FoldGitLocalService::new(),
        }
    }

    /// Create a new git local service with a custom base directory.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            inner: FoldGitLocalService::with_base_dir(base_dir),
        }
    }

    /// Get the local path for a repository clone.
    pub fn get_repo_path(&self, project_slug: &str, owner: &str, repo: &str) -> PathBuf {
        self.inner.get_repo_path(project_slug, owner, repo)
    }

    /// Clone a repository to a local path.
    pub async fn clone_repo(
        &self,
        project_slug: &str,
        owner: &str,
        repo: &str,
        branch: &str,
        token: &str,
        provider: &str,
    ) -> Result<PathBuf> {
        self.inner
            .clone_repo(project_slug, owner, repo, branch, token, provider)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Pull the latest changes for a repository.
    pub async fn pull_repo(
        &self,
        local_path: &Path,
        branch: &str,
        token: &str,
        provider: &str,
    ) -> Result<()> {
        self.inner
            .pull_repo(local_path, branch, token, provider)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Get the current HEAD commit SHA.
    pub async fn get_head_sha(&self, local_path: &Path) -> Result<String> {
        self.inner
            .get_head_sha(local_path)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Check if a path is a valid git repository.
    pub fn is_valid_repo(path: &Path) -> bool {
        FoldGitLocalService::is_valid_repo(path)
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
