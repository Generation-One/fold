//! Content resolver service for memory content retrieval.
//!
//! Resolves memory content based on storage type:
//! - Codebase memories: read from source file via file_path
//! - Other memories: read from filesystem (fold/{project}/memories/{id}.md)

use crate::db::{self, DbPool};
use crate::models::{ContentStorage, Memory};
use crate::services::MetaStorageService;
use crate::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tracing::warn;

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

    /// Resolve content for a memory based on its type and storage.
    ///
    /// Returns the content string, or None if content is unavailable
    /// (e.g. source file deleted).
    pub async fn resolve_content(
        &self,
        memory: &Memory,
        project_slug: &str,
        project_root_path: Option<&str>,
    ) -> Result<Option<String>> {
        let storage = memory.get_content_storage();

        match storage {
            ContentStorage::SourceFile => {
                self.resolve_source_file_content(memory, project_root_path)
                    .await
            }
            ContentStorage::Filesystem => {
                self.resolve_filesystem_content(memory, project_slug).await
            }
        }
    }

    /// Resolve content from a source file (for codebase memories).
    async fn resolve_source_file_content(
        &self,
        memory: &Memory,
        project_root_path: Option<&str>,
    ) -> Result<Option<String>> {
        let file_path = match &memory.file_path {
            Some(path) => path,
            None => {
                warn!(
                    memory_id = %memory.id,
                    "Codebase memory has no file_path"
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
        warn!(
            memory_id = %memory.id,
            file_path = %file_path,
            "Source file not found for codebase memory"
        );
        Ok(None)
    }

    /// Resolve content from filesystem (for non-codebase memories).
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
        project_root_path: Option<&str>,
    ) -> bool {
        let storage = memory.get_content_storage();

        match storage {
            ContentStorage::SourceFile => {
                // Check if source file exists
                if let Some(file_path) = &memory.file_path {
                    // Try repository local_path
                    if let Some(repo_id) = &memory.repository_id {
                        if let Ok(Some(repo)) = db::get_repository_optional(&self.db, repo_id).await
                        {
                            if let Some(local_path) = &repo.local_path {
                                let full_path = Path::new(local_path).join(file_path);
                                if full_path.exists() {
                                    return true;
                                }
                            }
                        }
                    }

                    // Try project root_path
                    if let Some(root_path) = project_root_path {
                        let full_path = Path::new(root_path).join(file_path);
                        if full_path.exists() {
                            return true;
                        }
                    }
                }
                false
            }
            ContentStorage::Filesystem => self.meta_storage.memory_exists(project_slug, &memory.id),
        }
    }
}
