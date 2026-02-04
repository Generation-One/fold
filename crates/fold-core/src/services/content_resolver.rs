//! Content resolver service for memory content retrieval.
//!
//! Resolves memory content based on source:
//! - File/Git memories: content (LLM summary) stored in SQLite
//! - Agent memories: content stored in fold/ directory
//!
//! The `content_storage` field is deprecated in favour of using `source`.

use crate::db::{self, DbPool};
use crate::models::Memory;
use crate::services::MetaStorageService;
use crate::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, warn};

/// Service for resolving memory content from external storage.
pub struct ContentResolverService {
    db: DbPool,
    meta_storage: Arc<MetaStorageService>,
}

impl ContentResolverService {
    /// Create a new content resolver service.
    pub fn new(db: DbPool, meta_storage: Arc<MetaStorageService>) -> Self {
        Self { db, meta_storage }
    }

    /// Resolve content for a memory based on its source.
    ///
    /// Content resolution strategy:
    /// - If content is already populated in memory, return it
    /// - For file/git sources: content should be in SQLite (already loaded)
    /// - For agent sources: read from fold/ directory
    ///
    /// Returns the content string, or None if content is unavailable.
    pub async fn resolve_content(
        &self,
        memory: &Memory,
        project_slug: &str,
        _project_root_path: Option<&str>,
    ) -> Result<Option<String>> {
        // If content is already populated, return it
        if let Some(content) = &memory.content {
            if !content.is_empty() {
                return Ok(Some(content.clone()));
            }
        }

        // Determine resolution strategy based on source
        let source = memory.source.as_deref().unwrap_or("agent");

        match source {
            "file" | "git" => {
                // File/Git memories: content (LLM summary) should be in SQLite
                // If we get here with no content, it's a legacy record - try fold/
                debug!(
                    memory_id = %memory.id,
                    source = %source,
                    "File/git memory has no content in SQLite, trying fold/ (legacy)"
                );
                self.resolve_filesystem_content(memory, project_slug).await
            }
            _ => {
                // Agent memories: content stored in fold/ directory
                self.resolve_filesystem_content(memory, project_slug).await
            }
        }
    }

    /// Resolve content from the original source file (for reading raw file content).
    ///
    /// This is different from the LLM summary - it reads the actual file content.
    /// Useful when you need the original source, not the indexed summary.
    pub async fn resolve_source_file_content(
        &self,
        memory: &Memory,
        project_root_path: Option<&str>,
    ) -> Result<Option<String>> {
        let file_path = match &memory.file_path {
            Some(path) => path,
            None => {
                warn!(
                    memory_id = %memory.id,
                    "Memory has no file_path for source file resolution"
                );
                return Ok(None);
            }
        };

        // Try repository local_path first
        if let Some(repo_id) = &memory.repository_id {
            if let Ok(Some(repo)) = db::get_repository_optional(&self.db, repo_id).await {
                if let Some(local_path) = &repo.local_path {
                    let full_path = Path::new(local_path).join(file_path);
                    if full_path.exists() {
                        match fs::read_to_string(&full_path).await {
                            Ok(content) => return Ok(Some(content)),
                            Err(e) => {
                                warn!(
                                    memory_id = %memory.id,
                                    path = %full_path.display(),
                                    error = %e,
                                    "Failed to read source file from repository local_path"
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fall back to project root_path
        if let Some(root_path) = project_root_path {
            let full_path = Path::new(root_path).join(file_path);
            if full_path.exists() {
                match fs::read_to_string(&full_path).await {
                    Ok(content) => return Ok(Some(content)),
                    Err(e) => {
                        warn!(
                            memory_id = %memory.id,
                            path = %full_path.display(),
                            error = %e,
                            "Failed to read source file from project root_path"
                        );
                    }
                }
            }
        }

        // Content not available (file deleted or paths not configured)
        debug!(
            memory_id = %memory.id,
            file_path = %file_path,
            "Source file not found"
        );
        Ok(None)
    }

    /// Resolve content from filesystem (fold/ directory).
    async fn resolve_filesystem_content(
        &self,
        memory: &Memory,
        project_slug: &str,
    ) -> Result<Option<String>> {
        self.meta_storage
            .read_memory_content(project_slug, &memory.id)
            .await
    }

    /// Resolve content for multiple memories.
    ///
    /// Returns a list of (memory_id, content) pairs. Memories with unavailable
    /// content will have None.
    pub async fn resolve_contents(
        &self,
        memories: &[Memory],
        project_slug: &str,
        project_root_path: Option<&str>,
    ) -> Result<Vec<(String, Option<String>)>> {
        let mut results = Vec::with_capacity(memories.len());

        for memory in memories {
            let content = self
                .resolve_content(memory, project_slug, project_root_path)
                .await?;
            results.push((memory.id.clone(), content));
        }

        Ok(results)
    }

    /// Populate content field on a memory in-place.
    ///
    /// If content cannot be resolved, the content field remains unchanged.
    pub async fn populate_content(
        &self,
        memory: &mut Memory,
        project_slug: &str,
        project_root_path: Option<&str>,
    ) -> Result<()> {
        if let Some(content) = self
            .resolve_content(memory, project_slug, project_root_path)
            .await?
        {
            memory.content = Some(content);
        }
        Ok(())
    }

    /// Populate content fields on multiple memories in-place.
    pub async fn populate_contents(
        &self,
        memories: &mut [Memory],
        project_slug: &str,
        project_root_path: Option<&str>,
    ) -> Result<()> {
        for memory in memories.iter_mut() {
            self.populate_content(memory, project_slug, project_root_path)
                .await?;
        }
        Ok(())
    }

    /// Check if content is available for a memory.
    pub async fn content_available(
        &self,
        memory: &Memory,
        project_slug: &str,
        _project_root_path: Option<&str>,
    ) -> bool {
        // If content is already populated, it's available
        if memory.content.as_ref().is_some_and(|c| !c.is_empty()) {
            return true;
        }

        // Determine based on source
        let source = memory.source.as_deref().unwrap_or("agent");

        match source {
            "file" | "git" => {
                // File/Git memories: content should be in SQLite
                // If not populated, check fold/ for legacy records
                self.meta_storage.memory_exists(project_slug, &memory.id)
            }
            _ => {
                // Agent memories: check fold/ directory
                self.meta_storage.memory_exists(project_slug, &memory.id)
            }
        }
    }
}
