//! Metadata Sync Service
//!
//! Syncs Fold metadata back to repositories as Markdown files in a `.fold/` directory.
//! Commits are made as `fold-meta-bot` to distinguish them from user commits.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use git2::{Cred, PushOptions, RemoteCallbacks, Repository as Git2Repo, Signature};
use tracing::{debug, info};

use crate::db::{self, DbPool, Memory, MemoryLink, Repository};
use crate::error::{Error, Result};

/// Bot identity for metadata commits.
const BOT_NAME: &str = "fold-meta-bot";
const BOT_EMAIL: &str = "fold-meta-bot@noreply.fold.dev";

/// Directory in the repo where Fold metadata lives.
const FOLD_DIR: &str = ".fold";

/// Service for syncing metadata back to repositories.
#[derive(Clone)]
pub struct MetadataSyncService {
    db: DbPool,
    work_dir: PathBuf,
}

impl MetadataSyncService {
    /// Create a new metadata sync service.
    pub fn new(db: DbPool, work_dir: PathBuf) -> Self {
        Self { db, work_dir }
    }

    /// Sync metadata for a repository.
    ///
    /// This will:
    /// 1. Clone/open the repository locally
    /// 2. Generate MD files for indexed files
    /// 3. Commit changes as fold-meta-bot
    /// 4. Push to remote
    pub async fn sync_repository(&self, repo: &Repository) -> Result<SyncResult> {
        info!(
            repo = %repo.full_name(),
            "Starting metadata sync"
        );

        // Get all codebase memories for this repo
        let memories = self.get_repo_memories(repo).await?;
        if memories.is_empty() {
            debug!(repo = %repo.full_name(), "No memories to sync");
            return Ok(SyncResult {
                files_created: 0,
                files_updated: 0,
                commit_sha: None,
            });
        }

        // Get links for context
        let links = self.get_repo_links(&memories).await?;

        // Clone/open repo locally
        let repo_path = self.ensure_repo_cloned(repo).await?;

        // Generate MD files
        let changes = self
            .generate_metadata_files(&repo_path, &memories, &links)
            .await?;

        if changes.is_empty() {
            debug!(repo = %repo.full_name(), "No changes to commit");
            return Ok(SyncResult {
                files_created: 0,
                files_updated: 0,
                commit_sha: None,
            });
        }

        // Commit and push
        let commit_sha = self.commit_and_push(&repo_path, repo, &changes).await?;

        let (created, updated) = changes.iter().fold((0, 0), |(c, u), change| match change {
            FileChange::Created(_) => (c + 1, u),
            FileChange::Updated(_) => (c, u + 1),
        });

        info!(
            repo = %repo.full_name(),
            files_created = created,
            files_updated = updated,
            commit = %commit_sha,
            "Metadata sync complete"
        );

        Ok(SyncResult {
            files_created: created,
            files_updated: updated,
            commit_sha: Some(commit_sha),
        })
    }

    /// Get all codebase memories for a repository.
    async fn get_repo_memories(&self, repo: &Repository) -> Result<Vec<Memory>> {
        let memories = db::list_memories_by_repository(&self.db, &repo.id).await?;
        Ok(memories)
    }

    /// Get links between memories.
    async fn get_repo_links(
        &self,
        memories: &[Memory],
    ) -> Result<HashMap<String, Vec<MemoryLink>>> {
        let mut links_map: HashMap<String, Vec<MemoryLink>> = HashMap::new();

        for memory in memories {
            let links = db::list_memory_links(&self.db, &memory.id).await?;
            links_map.insert(memory.id.clone(), links);
        }

        Ok(links_map)
    }

    /// Ensure the repository is cloned locally.
    async fn ensure_repo_cloned(&self, repo: &Repository) -> Result<PathBuf> {
        let repo_dir = self.work_dir.join(&repo.id);

        if repo_dir.exists() {
            // Pull latest changes
            self.pull_repo(&repo_dir, repo).await?;
        } else {
            // Clone the repo
            self.clone_repo(&repo_dir, repo).await?;
        }

        Ok(repo_dir)
    }

    /// Clone a repository.
    async fn clone_repo(&self, repo_dir: &Path, repo: &Repository) -> Result<()> {
        let clone_url = self.get_clone_url(repo);

        info!(
            repo = %repo.full_name(),
            path = %repo_dir.display(),
            "Cloning repository"
        );

        let token = repo.access_token.clone();
        let repo_dir = repo_dir.to_path_buf();

        // Clone in a blocking task since git2 is sync
        tokio::task::spawn_blocking(move || {
            // Create callbacks inside the blocking task
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token)
            });

            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);

            let mut builder = git2::build::RepoBuilder::new();
            builder.fetch_options(fetch_options);

            builder
                .clone(&clone_url, &repo_dir)
                .map_err(|e| Error::Internal(format!("Git clone failed: {}", e)))?;

            Ok::<_, Error>(())
        })
        .await
        .map_err(|e| Error::Internal(format!("Clone task failed: {}", e)))??;

        Ok(())
    }

    /// Pull latest changes from remote.
    async fn pull_repo(&self, repo_dir: &Path, repo: &Repository) -> Result<()> {
        debug!(
            repo = %repo.full_name(),
            path = %repo_dir.display(),
            "Pulling latest changes"
        );

        let repo_dir = repo_dir.to_path_buf();
        let token = repo.access_token.clone();

        tokio::task::spawn_blocking(move || {
            let git_repo = Git2Repo::open(&repo_dir)
                .map_err(|e| Error::Internal(format!("Failed to open repo: {}", e)))?;

            let mut remote = git_repo
                .find_remote("origin")
                .map_err(|e| Error::Internal(format!("Failed to find remote: {}", e)))?;

            // Create callbacks inside the blocking task
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token)
            });

            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);

            remote
                .fetch(
                    &["refs/heads/*:refs/remotes/origin/*"],
                    Some(&mut fetch_options),
                    None,
                )
                .map_err(|e| Error::Internal(format!("Fetch failed: {}", e)))?;

            // Fast-forward if possible
            if let Ok(fetch_head) = git_repo.find_reference("FETCH_HEAD") {
                if let Ok(fetch_commit) = git_repo.reference_to_annotated_commit(&fetch_head) {
                    if let Ok((analysis, _)) = git_repo.merge_analysis(&[&fetch_commit]) {
                        if analysis.is_fast_forward() {
                            if let Ok(head) = git_repo.head() {
                                let refname =
                                    format!("refs/heads/{}", head.shorthand().unwrap_or("main"));
                                if let Ok(mut reference) = git_repo.find_reference(&refname) {
                                    let _ = reference.set_target(fetch_commit.id(), "Fast-forward");
                                    let _ = git_repo.checkout_head(Some(
                                        git2::build::CheckoutBuilder::default().force(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            Ok::<_, Error>(())
        })
        .await
        .map_err(|e| Error::Internal(format!("Pull task failed: {}", e)))??;

        Ok(())
    }

    /// Generate metadata MD files.
    async fn generate_metadata_files(
        &self,
        repo_dir: &Path,
        memories: &[Memory],
        links: &HashMap<String, Vec<MemoryLink>>,
    ) -> Result<Vec<FileChange>> {
        let fold_dir = repo_dir.join(FOLD_DIR);
        let files_dir = fold_dir.join("files");

        // Ensure directories exist
        tokio::fs::create_dir_all(&files_dir)
            .await
            .map_err(|e| Error::Internal(format!("Failed to create .fold dir: {}", e)))?;

        let mut changes = Vec::new();

        // Group memories by file path
        let mut by_file: HashMap<String, Vec<&Memory>> = HashMap::new();
        for memory in memories {
            if let Some(ref path) = memory.file_path {
                by_file.entry(path.clone()).or_default().push(memory);
            }
        }

        // Generate MD for each file
        for (file_path, file_memories) in &by_file {
            let md_path = files_dir.join(format!(
                "{}.md",
                file_path.replace('/', "_").replace('\\', "_")
            ));
            let content = self.generate_file_md(file_path, file_memories, links);

            let is_new = !md_path.exists();
            let should_write = if is_new {
                true
            } else {
                // Check if content changed
                let existing = tokio::fs::read_to_string(&md_path)
                    .await
                    .unwrap_or_default();
                existing != content
            };

            if should_write {
                tokio::fs::write(&md_path, &content)
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to write MD file: {}", e)))?;

                let relative_path = md_path
                    .strip_prefix(repo_dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| md_path.to_string_lossy().to_string());

                if is_new {
                    changes.push(FileChange::Created(relative_path));
                } else {
                    changes.push(FileChange::Updated(relative_path));
                }
            }
        }

        // Generate README.md for .fold
        let readme_path = fold_dir.join("README.md");
        let readme_content = self.generate_readme(memories);

        let readme_is_new = !readme_path.exists();
        let readme_should_write = if readme_is_new {
            true
        } else {
            let existing = tokio::fs::read_to_string(&readme_path)
                .await
                .unwrap_or_default();
            existing != readme_content
        };

        if readme_should_write {
            tokio::fs::write(&readme_path, &readme_content)
                .await
                .map_err(|e| Error::Internal(format!("Failed to write README: {}", e)))?;

            if readme_is_new {
                changes.push(FileChange::Created(".fold/README.md".to_string()));
            } else {
                changes.push(FileChange::Updated(".fold/README.md".to_string()));
            }
        }

        Ok(changes)
    }

    /// Generate Markdown content for a single file.
    fn generate_file_md(
        &self,
        file_path: &str,
        memories: &[&Memory],
        links: &HashMap<String, Vec<MemoryLink>>,
    ) -> String {
        let mut md = String::new();

        // Header
        md.push_str(&format!("# {}\n\n", file_path));

        // Summary from latest memory
        if let Some(memory) = memories.iter().max_by_key(|m| &m.updated_at) {
            if let Some(ref title) = memory.title {
                md.push_str(&format!("## Summary\n\n{}\n\n", title));
            }

            // Keywords
            if let Some(ref keywords_json) = memory.keywords {
                if let Ok(keywords) = serde_json::from_str::<Vec<String>>(keywords_json) {
                    if !keywords.is_empty() {
                        md.push_str("## Keywords\n\n");
                        for kw in &keywords {
                            md.push_str(&format!("- {}\n", kw));
                        }
                        md.push('\n');
                    }
                }
            }

            // Links
            if let Some(memory_links) = links.get(&memory.id) {
                if !memory_links.is_empty() {
                    md.push_str("## Links\n\n");
                    for link in memory_links {
                        md.push_str(&format!("- **{}**: {}\n", link.link_type, link.target_id));
                    }
                    md.push('\n');
                }
            }
        }

        // Metadata footer
        md.push_str("---\n\n");
        md.push_str(&format!(
            "*Last indexed: {} by [Fold](https://fold.dev)*\n",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
        ));

        md
    }

    /// Generate README.md for the .fold directory.
    fn generate_readme(&self, memories: &[Memory]) -> String {
        let mut md = String::new();

        md.push_str("# Fold Metadata\n\n");
        md.push_str(
            "This directory contains auto-generated metadata from [Fold](https://fold.dev).\n\n",
        );
        md.push_str("> **Note**: Do not edit these files manually. They are automatically updated by Fold.\n\n");

        // Stats
        let file_count = memories
            .iter()
            .filter_map(|m| m.file_path.as_ref())
            .collect::<std::collections::HashSet<_>>()
            .len();

        md.push_str("## Statistics\n\n");
        md.push_str(&format!("- **Files indexed**: {}\n", file_count));
        md.push_str(&format!("- **Total memories**: {}\n", memories.len()));
        md.push_str(&format!(
            "- **Last sync**: {}\n\n",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
        ));

        // Structure
        md.push_str("## Structure\n\n");
        md.push_str("```\n");
        md.push_str(".fold/\n");
        md.push_str("  README.md      # This file\n");
        md.push_str("  files/         # Per-file metadata\n");
        md.push_str("```\n\n");

        md.push_str("---\n\n");
        md.push_str("*Generated by [fold-meta-bot](https://fold.dev)*\n");

        md
    }

    /// Commit changes and push to remote.
    async fn commit_and_push(
        &self,
        repo_dir: &Path,
        repo: &Repository,
        changes: &[FileChange],
    ) -> Result<String> {
        let repo_dir = repo_dir.to_path_buf();
        let token = repo.access_token.clone();

        let file_count = changes.len();
        let message = if file_count == 1 {
            "chore(fold): update metadata for 1 file\n\nSynced by fold-meta-bot".to_string()
        } else {
            format!(
                "chore(fold): update metadata for {} files\n\nSynced by fold-meta-bot",
                file_count
            )
        };

        let changes: Vec<String> = changes.iter().map(|c| c.path().to_string()).collect();

        tokio::task::spawn_blocking(move || {
            let git_repo = Git2Repo::open(&repo_dir)
                .map_err(|e| Error::Internal(format!("Failed to open repo: {}", e)))?;

            // Stage all changes
            let mut index = git_repo
                .index()
                .map_err(|e| Error::Internal(format!("Failed to get index: {}", e)))?;

            for file_path in &changes {
                // Normalize path separators for git
                let normalized = file_path.replace('\\', "/");
                index.add_path(Path::new(&normalized)).map_err(|e| {
                    Error::Internal(format!("Failed to stage {}: {}", file_path, e))
                })?;
            }

            index
                .write()
                .map_err(|e| Error::Internal(format!("Failed to write index: {}", e)))?;

            let tree_id = index
                .write_tree()
                .map_err(|e| Error::Internal(format!("Failed to write tree: {}", e)))?;
            let tree = git_repo
                .find_tree(tree_id)
                .map_err(|e| Error::Internal(format!("Failed to find tree: {}", e)))?;

            // Create commit
            let signature = Signature::now(BOT_NAME, BOT_EMAIL)
                .map_err(|e| Error::Internal(format!("Failed to create signature: {}", e)))?;

            let parent_commit = git_repo
                .head()
                .and_then(|r| r.peel_to_commit())
                .map_err(|e| Error::Internal(format!("Failed to get HEAD commit: {}", e)))?;

            let commit_oid = git_repo
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &message,
                    &tree,
                    &[&parent_commit],
                )
                .map_err(|e| Error::Internal(format!("Failed to create commit: {}", e)))?;

            // Push to remote
            let mut remote = git_repo
                .find_remote("origin")
                .map_err(|e| Error::Internal(format!("Failed to find remote: {}", e)))?;

            // Create callbacks inside the blocking task
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token)
            });

            let mut push_options = PushOptions::new();
            push_options.remote_callbacks(callbacks);

            let head_ref = git_repo
                .head()
                .map_err(|e| Error::Internal(format!("Failed to get HEAD: {}", e)))?;
            let refspec = format!(
                "refs/heads/{}:refs/heads/{}",
                head_ref.shorthand().unwrap_or("main"),
                head_ref.shorthand().unwrap_or("main")
            );

            remote
                .push(&[&refspec], Some(&mut push_options))
                .map_err(|e| Error::Internal(format!("Failed to push: {}", e)))?;

            Ok::<_, Error>(commit_oid.to_string())
        })
        .await
        .map_err(|e| Error::Internal(format!("Commit task failed: {}", e)))?
    }

    /// Get clone URL with authentication.
    fn get_clone_url(&self, repo: &Repository) -> String {
        match repo.provider.as_str() {
            "github" => format!("https://github.com/{}/{}.git", repo.owner, repo.repo),
            "gitlab" => format!("https://gitlab.com/{}/{}.git", repo.owner, repo.repo),
            _ => format!("https://github.com/{}/{}.git", repo.owner, repo.repo),
        }
    }
}

/// Result of a metadata sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub files_created: usize,
    pub files_updated: usize,
    pub commit_sha: Option<String>,
}

/// A file change to be committed.
#[derive(Debug, Clone)]
enum FileChange {
    Created(String),
    Updated(String),
}

impl FileChange {
    fn path(&self) -> &str {
        match self {
            FileChange::Created(p) => p,
            FileChange::Updated(p) => p,
        }
    }
}
