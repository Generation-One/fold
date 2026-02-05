//! Hash-based storage service for the fold/ directory.
//!
//! Re-exports from the `fold-storage` crate and provides integration
//! with fold-core's Memory model.

use std::path::{Path, PathBuf};

// Re-export from the fold-storage crate
pub use fold_storage::{
    generate_memory_id, slug_to_id, slug_to_memory_id, slugify, slugify_unique, EmbeddingConfig,
    Error as StorageError, FoldStorageService, IndexingConfig, MemoryData, MemoryFrontmatter,
    ProjectConfig, ProjectInfo, Result as StorageResult, StorageMemory,
};

use crate::error::{Error, Result};
use crate::models::Memory;

/// Extension trait for FoldStorageService to work with fold-core's Memory type.
pub trait FoldStorageExt {
    /// Read a memory from the fold/ directory, converting to fold-core Memory.
    fn read_memory_ext(
        &self,
        project_root: &Path,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<(Memory, String)>> + Send;

    /// Parse memory file content, converting to fold-core Memory.
    fn parse_memory_file_ext(&self, content: &str) -> Result<(Memory, String)>;
}

impl FoldStorageExt for FoldStorageService {
    async fn read_memory_ext(&self, project_root: &Path, hash: &str) -> Result<(Memory, String)> {
        let (storage_memory, content) = self.read_memory(project_root, hash).await.map_err(|e| {
            match e {
                StorageError::FileNotFound(msg) => Error::FileNotFound(msg),
                StorageError::InvalidInput(msg) => Error::InvalidInput(msg),
                StorageError::Internal(msg) => Error::Internal(msg),
                StorageError::Io(e) => Error::Internal(e.to_string()),
            }
        })?;

        Ok((storage_memory_to_memory(storage_memory), content))
    }

    fn parse_memory_file_ext(&self, content: &str) -> Result<(Memory, String)> {
        let (storage_memory, body) = self.parse_memory_file(content).map_err(|e| {
            match e {
                StorageError::FileNotFound(msg) => Error::FileNotFound(msg),
                StorageError::InvalidInput(msg) => Error::InvalidInput(msg),
                StorageError::Internal(msg) => Error::Internal(msg),
                StorageError::Io(e) => Error::Internal(e.to_string()),
            }
        })?;

        Ok((storage_memory_to_memory(storage_memory), body))
    }
}

/// Convert StorageMemory to fold-core Memory.
pub fn storage_memory_to_memory(sm: StorageMemory) -> Memory {
    Memory {
        id: sm.id,
        project_id: sm.project_id,
        memory_type: sm.memory_type,
        source: sm.source,
        content: sm.content,
        content_hash: sm.content_hash,
        content_storage: None,
        title: sm.title,
        author: sm.author,
        keywords: None,
        tags: sm.tags,
        context: None,
        file_path: sm.file_path,
        language: sm.language,
        line_start: None,
        line_end: None,
        status: None,
        assignee: None,
        metadata: sm.metadata,
        created_at: sm.created_at,
        updated_at: sm.updated_at,
        retrieval_count: 0,
        last_accessed: None,
    }
}

/// Convert fold-core Memory to StorageMemory.
pub fn memory_to_storage_memory(m: &Memory) -> StorageMemory {
    StorageMemory {
        id: m.id.clone(),
        project_id: m.project_id.clone(),
        memory_type: m.memory_type.clone(),
        source: m.source.clone(),
        content: m.content.clone(),
        content_hash: m.content_hash.clone(),
        title: m.title.clone(),
        author: m.author.clone(),
        tags: m.tags.clone(),
        file_path: m.file_path.clone(),
        language: m.language.clone(),
        metadata: m.metadata.clone(),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

/// Convert storage error to fold-core error.
pub fn storage_error_to_error(e: StorageError) -> Error {
    match e {
        StorageError::FileNotFound(msg) => Error::FileNotFound(msg),
        StorageError::InvalidInput(msg) => Error::InvalidInput(msg),
        StorageError::Internal(msg) => Error::Internal(msg),
        StorageError::Io(e) => Error::Internal(e.to_string()),
    }
}

/// Wrapper function to read project config with fold-core error type.
pub async fn read_project_config(
    service: &FoldStorageService,
    project_root: &Path,
) -> Result<ProjectConfig> {
    service
        .read_project_config(project_root)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to write project config with fold-core error type.
pub async fn write_project_config(
    service: &FoldStorageService,
    project_root: &Path,
    config: &ProjectConfig,
) -> Result<()> {
    service
        .write_project_config(project_root, config)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to init fold directory with fold-core error type.
pub async fn init_fold_directory(
    service: &FoldStorageService,
    project_root: &Path,
    project_id: &str,
    project_slug: &str,
    project_name: &str,
) -> Result<()> {
    service
        .init_fold_directory(project_root, project_id, project_slug, project_name)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to write memory with fold-core error type.
pub async fn write_memory(
    service: &FoldStorageService,
    project_root: &Path,
    memory: &Memory,
    content: &str,
) -> Result<PathBuf> {
    service
        .write_memory(project_root, memory, content)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to write memory with links with fold-core error type.
pub async fn write_memory_with_links(
    service: &FoldStorageService,
    project_root: &Path,
    memory: &Memory,
    content: &str,
    related_ids: &[String],
) -> Result<PathBuf> {
    service
        .write_memory_with_links(project_root, memory, content, related_ids)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to update memory links with fold-core error type.
pub async fn update_memory_links(
    service: &FoldStorageService,
    project_root: &Path,
    memory_id: &str,
    related_ids: &[String],
) -> Result<PathBuf> {
    service
        .update_memory_links(project_root, memory_id, related_ids)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to read memory with fold-core error type.
pub async fn read_memory(
    service: &FoldStorageService,
    project_root: &Path,
    hash: &str,
) -> Result<(Memory, String)> {
    let (sm, content) = service
        .read_memory(project_root, hash)
        .await
        .map_err(storage_error_to_error)?;
    Ok((storage_memory_to_memory(sm), content))
}

/// Wrapper function to scan fold directory with fold-core error type.
pub async fn scan_fold_directory(
    service: &FoldStorageService,
    project_root: &Path,
) -> Result<Vec<String>> {
    service
        .scan_fold_directory(project_root)
        .await
        .map_err(storage_error_to_error)
}

/// Wrapper function to delete memory with fold-core error type.
pub async fn delete_memory(
    service: &FoldStorageService,
    project_root: &Path,
    hash: &str,
) -> Result<()> {
    service
        .delete_memory(project_root, hash)
        .await
        .map_err(storage_error_to_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_memory_path() {
        let service = FoldStorageService::new();
        let project_root = Path::new("/projects/my-app");

        let path = service.get_memory_path(project_root, "aBcD123456789abc");
        assert_eq!(
            path,
            Path::new("/projects/my-app/fold/a/B/aBcD123456789abc.md")
        );

        let path = service.get_memory_path(project_root, "f0123456789abcde");
        assert_eq!(
            path,
            Path::new("/projects/my-app/fold/f/0/f0123456789abcde.md")
        );
    }

    #[test]
    fn test_parse_memory_file() {
        let service = FoldStorageService::new();

        let content = r#"---
id: aBcD123456789abc
title: Test Memory
author: claude
tags:
  - test
  - example
memory_type: session
created_at: 2026-02-02T10:30:00Z
updated_at: 2026-02-02T10:30:00Z
---

This is the memory content.

## Section

More content here."#;

        let (memory, body) = service.parse_memory_file_ext(content).unwrap();

        assert_eq!(memory.id, "aBcD123456789abc");
        assert_eq!(memory.title, Some("Test Memory".to_string()));
        assert_eq!(memory.author, Some("claude".to_string()));
        assert_eq!(memory.memory_type, "session");
        assert!(body.contains("This is the memory content."));
        assert!(body.contains("## Section"));
    }

    #[test]
    fn test_memory_data_impl() {
        use chrono::Utc;

        let now = Utc::now();
        let memory = Memory {
            id: "test123".to_string(),
            project_id: "proj1".to_string(),
            memory_type: "codebase".to_string(),
            source: None,
            content: None,
            content_hash: None,
            content_storage: None,
            title: Some("Test".to_string()),
            author: Some("claude".to_string()),
            keywords: None,
            tags: Some(r#"["tag1"]"#.to_string()),
            context: None,
            file_path: Some("src/main.rs".to_string()),
            language: Some("rust".to_string()),
            line_start: None,
            line_end: None,
            status: None,
            assignee: None,
            metadata: None,
            created_at: now,
            updated_at: now,
            retrieval_count: 0,
            last_accessed: None,
        };

        // Test MemoryData trait implementation
        assert_eq!(memory.id(), "test123");
        assert_eq!(memory.title(), Some("Test"));
        assert_eq!(memory.author(), Some("claude"));
        assert_eq!(memory.memory_type(), "codebase");
        assert_eq!(memory.file_path(), Some("src/main.rs"));
        assert_eq!(memory.language(), Some("rust"));
        assert_eq!(memory.tags(), vec!["tag1".to_string()]);
    }
}
