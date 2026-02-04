//! Git integration service for Fold.
//!
//! Provides auto-commit functionality for the fold/ directory
//! and sync operations to import memories from remote repositories.

use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::db::{DbPool, Project};
use crate::error::{Error, Result};
use crate::models::{MemoryCreate, MemoryType};
use crate::services::{EmbeddingService, FoldStorageService, MemoryService, QdrantService};

/// Statistics from a sync operation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncStats {
    /// Number of memories imported from remote.
    pub imported: usize,
    /// Number of memories that already existed locally.
    pub existing: usize,
    /// Number of errors during sync.
    pub errors: usize,
}

/// Result of an auto-commit operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommitResult {
    /// The commit SHA if a commit was made.
    pub sha: Option<String>,
    /// Whether any changes were committed.
    pub committed: bool,
    /// Message describing the result.
    pub message: String,
}

/// Service for git operations on the fold/ directory.
#[derive(Clone)]
pub struct GitService {
    db: DbPool,
    memory_service: MemoryService,
    fold_storage: Arc<FoldStorageService>,
    qdrant: Arc<QdrantService>,
    embeddings: Arc<EmbeddingService>,
}

impl GitService {
    /// Create a new git service.
    pub fn new(
        db: DbPool,
        memory_service: MemoryService,
        fold_storage: Arc<FoldStorageService>,
        qdrant: Arc<QdrantService>,
        embeddings: Arc<EmbeddingService>,
    ) -> Self {
        Self {
            db,
            memory_service,
            fold_storage,
            qdrant,
            embeddings,
        }
    }

    /// Stage the fold/ directory for commit.
    async fn stage_fold(&self, repo_path: &Path) -> Result<()> {
        let output = tokio::process::Command::new("git")
            .arg("add")
            .arg("fold/")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run git add: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If fold/ doesn't exist yet, that's fine
            if stderr.contains("did not match any files") {
                debug!("fold/ directory does not exist yet, nothing to stage");
                return Ok(());
            }
            return Err(Error::Internal(format!("Git add failed: {}", stderr)));
        }

        Ok(())
    }

    /// Check if there are staged changes in the fold/ directory.
    async fn has_changes(&self, repo_path: &Path) -> Result<bool> {
        let output = tokio::process::Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .arg("fold/")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run git status: {}", e)))?;

        Ok(!output.stdout.is_empty())
    }

    /// Create a commit with the specified message.
    async fn commit(&self, repo_path: &Path, message: &str) -> Result<String> {
        let output = tokio::process::Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run git commit: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Internal(format!("Git commit failed: {}", stderr)));
        }

        // Get the commit SHA
        self.get_head_sha(repo_path).await
    }

    /// Get the current HEAD commit SHA.
    pub async fn get_head_sha(&self, repo_path: &Path) -> Result<String> {
        let output = tokio::process::Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run git rev-parse: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Internal("Failed to get HEAD SHA".to_string()));
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(sha)
    }

    /// Pull latest changes from the remote with rebase.
    pub async fn pull(&self, repo_path: &Path) -> Result<()> {
        let output = tokio::process::Command::new("git")
            .arg("pull")
            .arg("--rebase")
            .arg("--strategy-option=theirs")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run git pull: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If there's no remote or no commits, that's ok
            if stderr.contains("no tracking information") || stderr.contains("no remote") {
                debug!("No remote tracking branch configured");
                return Ok(());
            }
            return Err(Error::Internal(format!("Git pull failed: {}", stderr)));
        }

        Ok(())
    }

    /// Push changes to the remote.
    pub async fn push(&self, repo_path: &Path) -> Result<()> {
        let output = tokio::process::Command::new("git")
            .arg("push")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run git push: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If there's no remote configured, that's ok
            if stderr.contains("no upstream branch") || stderr.contains("no remote") {
                debug!("No remote configured, skipping push");
                return Ok(());
            }
            return Err(Error::Internal(format!("Git push failed: {}", stderr)));
        }

        Ok(())
    }

    /// Check if a directory is a git repository.
    pub fn is_git_repo(path: &Path) -> bool {
        path.join(".git").exists()
    }

    /// Check if the fold/ directory exists and is initialised.
    pub fn is_fold_initialized(repo_path: &Path) -> bool {
        repo_path.join("fold").exists()
    }

    /// Initialise the fold/ directory with a .gitignore file.
    pub async fn init_fold(&self, repo_path: &Path) -> Result<()> {
        let fold_path = repo_path.join("fold");
        tokio::fs::create_dir_all(&fold_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to create fold/ directory: {}", e)))?;

        // Create .gitignore for temporary files
        let gitignore_content = "*.tmp\n*.lock\n";
        tokio::fs::write(fold_path.join(".gitignore"), gitignore_content)
            .await
            .map_err(|e| Error::Internal(format!("Failed to write .gitignore: {}", e)))?;

        Ok(())
    }

    /// Auto-commit changes to the fold/ directory and push to remote.
    ///
    /// This is called after indexing operations to automatically commit
    /// any new or modified memory files and sync them to the remote.
    /// If there are conflicts, uses rebase with remote-wins strategy.
    pub async fn auto_commit_fold(&self, repo_path: &Path, message: &str) -> Result<CommitResult> {
        // Check if this is a git repository
        if !Self::is_git_repo(repo_path) {
            return Ok(CommitResult {
                sha: None,
                committed: false,
                message: "Not a git repository".to_string(),
            });
        }

        // Initialise fold/ if needed
        if !Self::is_fold_initialized(repo_path) {
            self.init_fold(repo_path).await?;
        }

        // Pull latest changes from remote with rebase (silently handle no remote)
        if let Err(e) = self.pull(repo_path).await {
            debug!("Pull before commit failed (continuing anyway): {}", e);
        }

        // Stage fold/ directory
        self.stage_fold(repo_path).await?;

        // Check if there are changes to commit
        if !self.has_changes(repo_path).await? {
            return Ok(CommitResult {
                sha: None,
                committed: false,
                message: "No changes to commit".to_string(),
            });
        }

        // Create the commit
        let sha = self.commit(repo_path, message).await?;

        info!(
            repo = %repo_path.display(),
            sha = %sha,
            "Auto-committed fold/ changes"
        );

        // Push to remote (silently handle no remote)
        if let Err(e) = self.push(repo_path).await {
            debug!("Push after commit failed: {}", e);
            return Ok(CommitResult {
                sha: Some(sha.clone()),
                committed: true,
                message: format!("Committed {} (push failed: {})", sha, e),
            });
        }

        info!(
            repo = %repo_path.display(),
            sha = %sha,
            "Pushed fold/ changes to remote"
        );

        Ok(CommitResult {
            sha: Some(sha.clone()),
            committed: true,
            message: format!("Committed and pushed: {}", sha),
        })
    }

    /// Scan the fold/ directory for memory files.
    ///
    /// Uses FoldStorageService to walk the hash-based directory structure
    /// (fold/a/b/hash.md) and return all memory hashes.
    async fn scan_fold_directory(&self, repo_path: &Path) -> Result<Vec<String>> {
        self.fold_storage.scan_fold_directory(repo_path).await
    }

    /// Sync memories from the remote repository.
    ///
    /// This pulls the latest changes and imports any new memory files
    /// that don't exist in the local database.
    pub async fn sync_from_remote(&self, project: &Project) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        // Get the project root path
        let repo_path = match &project.root_path {
            Some(path) => std::path::PathBuf::from(path),
            None => {
                return Err(Error::Validation(
                    "Project has no root_path configured".to_string(),
                ));
            }
        };

        // Check if this is a git repository
        if !Self::is_git_repo(&repo_path) {
            return Err(Error::Validation(
                "Project root is not a git repository".to_string(),
            ));
        }

        // Pull latest changes
        self.pull(&repo_path).await?;

        // Scan fold/ directory for memory files
        let memory_ids = self.scan_fold_directory(&repo_path).await?;

        info!(
            project = %project.slug,
            found = memory_ids.len(),
            "Scanned fold/ directory for memories"
        );

        for memory_id in memory_ids {
            // Check if memory already exists in database
            match crate::db::get_memory(&self.db, &memory_id).await {
                Ok(_) => {
                    // Memory already exists
                    stats.existing += 1;
                    continue;
                }
                Err(Error::NotFound(_)) => {
                    // Memory doesn't exist, import it
                }
                Err(e) => {
                    warn!(
                        memory_id = %memory_id,
                        error = %e,
                        "Error checking memory existence"
                    );
                    stats.errors += 1;
                    continue;
                }
            }

            // Read memory file from filesystem using hash-based storage
            match self.fold_storage.read_memory(&repo_path, &memory_id).await {
                Ok((mut memory, content)) => {
                    // Set project_id (not in frontmatter)
                    memory.project_id = project.id.clone();

                    // Create memory through the service
                    let memory_type =
                        MemoryType::from_str(&memory.memory_type).unwrap_or(MemoryType::General);

                    let create = MemoryCreate {
                        memory_type,
                        content: content.clone(),
                        title: memory.title.clone(),
                        author: memory.author.clone(),
                        tags: memory.tags_vec(),
                        keywords: memory.keywords_vec(),
                        context: memory.context.clone(),
                        file_path: memory.file_path.clone(),
                        language: memory.language.clone(),
                        source: memory
                            .source
                            .as_ref()
                            .and_then(|s| crate::models::MemorySource::from_str(s)),
                        ..Default::default()
                    };

                    // Add memory through the service (handles embedding and Qdrant)
                    match self
                        .memory_service
                        .add(&project.id, &project.slug, create, false)
                        .await
                    {
                        Ok(_memory) => {
                            info!(
                                memory_id = %memory_id,
                                "Imported memory from fold/"
                            );
                            stats.imported += 1;
                        }
                        Err(e) => {
                            warn!(
                                memory_id = %memory_id,
                                error = %e,
                                "Failed to import memory"
                            );
                            stats.errors += 1;
                        }
                    }
                }
                Err(Error::FileNotFound(_)) => {
                    debug!(
                        memory_id = %memory_id,
                        "Memory file not found (may have been deleted)"
                    );
                }
                Err(e) => {
                    warn!(
                        memory_id = %memory_id,
                        error = %e,
                        "Failed to read memory file"
                    );
                    stats.errors += 1;
                }
            }
        }

        info!(
            project = %project.slug,
            imported = stats.imported,
            existing = stats.existing,
            errors = stats.errors,
            "Completed sync from remote"
        );

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_stats_default() {
        let stats = SyncStats::default();
        assert_eq!(stats.imported, 0);
        assert_eq!(stats.existing, 0);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_commit_result_no_commit() {
        let result = CommitResult {
            sha: None,
            committed: false,
            message: "No changes".to_string(),
        };
        assert!(!result.committed);
        assert!(result.sha.is_none());
    }
}
