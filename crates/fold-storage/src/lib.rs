//! Hash-based storage service for the fold/ directory.
//!
//! Provides a storage layer that persists memories as markdown files
//! in a hash-based directory structure: fold/a/b/aBcD123.md
//!
//! Memory files use YAML frontmatter for metadata:
//! ```markdown
//! ---
//! id: aBcD123456789abc
//! title: "Memory title"
//! author: claude
//! tags:
//!   - tag1
//!   - tag2
//! file_path: src/auth/service.ts
//! language: typescript
//! memory_type: codebase
//! created_at: 2026-02-02T10:30:00Z
//! updated_at: 2026-02-02T10:30:00Z
//! related_to:
//!   - f0123456789abcde
//! ---
//!
//! Content goes here...
//! ```

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::info;

mod error;

pub use error::{Error, Result};

/// Trait for memory data that can be converted to/from frontmatter.
///
/// This allows fold-storage to work with any memory type that implements
/// this trait, without depending on fold-core's Memory model.
pub trait MemoryData {
    fn id(&self) -> &str;
    fn title(&self) -> Option<&str>;
    fn author(&self) -> Option<&str>;
    fn tags(&self) -> Vec<String>;
    fn file_path(&self) -> Option<&str>;
    fn language(&self) -> Option<&str>;
    fn memory_type(&self) -> &str;
    fn metadata_json(&self) -> Option<&str>;
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
}

/// Standalone memory structure for fold-storage.
///
/// This is a minimal memory representation that can be used
/// when fold-core types are not available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMemory {
    pub id: String,
    pub project_id: String,
    pub memory_type: String,
    pub source: Option<String>,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    /// JSON array of tags
    pub tags: Option<String>,
    pub file_path: Option<String>,
    pub language: Option<String>,
    pub metadata: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StorageMemory {
    /// Parse tags from JSON string
    pub fn tags_vec(&self) -> Vec<String> {
        self.tags
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}

impl MemoryData for StorageMemory {
    fn id(&self) -> &str {
        &self.id
    }

    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    fn tags(&self) -> Vec<String> {
        self.tags_vec()
    }

    fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    fn memory_type(&self) -> &str {
        &self.memory_type
    }

    fn metadata_json(&self) -> Option<&str> {
        self.metadata.as_deref()
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Frontmatter structure for memory files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub memory_type: String,
    /// Original creation date extracted from file content (if found)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_date: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_to: Vec<String>,
}

impl MemoryFrontmatter {
    /// Create frontmatter from any type implementing MemoryData.
    pub fn from_memory<M: MemoryData>(memory: &M) -> Self {
        // Extract original_date from metadata if present
        let original_date = memory
            .metadata_json()
            .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
            .and_then(|json| {
                json.get("original_date")
                    .and_then(|v| v.as_str().map(String::from))
            });

        Self {
            id: memory.id().to_string(),
            title: memory.title().map(String::from),
            author: memory.author().map(String::from),
            tags: memory.tags(),
            file_path: memory.file_path().map(String::from),
            language: memory.language().map(String::from),
            memory_type: memory.memory_type().to_string(),
            original_date,
            created_at: memory.created_at(),
            updated_at: memory.updated_at(),
            related_to: Vec::new(), // Populated separately if needed
        }
    }

    /// Convert frontmatter to a StorageMemory.
    /// Note: project_id must be set by the caller.
    pub fn to_storage_memory(&self) -> StorageMemory {
        StorageMemory {
            id: self.id.clone(),
            project_id: String::new(), // Set by caller
            memory_type: self.memory_type.clone(),
            source: Some("file".to_string()), // Default to file source for fold/ imports
            content: None,                    // Content is stored in the body, not frontmatter
            content_hash: Some(self.id.clone()),
            title: self.title.clone(),
            author: self.author.clone(),
            tags: if self.tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&self.tags).unwrap_or_default())
            },
            file_path: self.file_path.clone(),
            language: self.language.clone(),
            metadata: None,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Project configuration stored in fold/project.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectInfo,
    #[serde(default)]
    pub indexing: IndexingConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

/// Project information section of configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Indexing configuration for the project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexingConfig {
    /// Glob patterns to include in indexing.
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns to exclude from indexing.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Embedding configuration for the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_dimension")]
    pub dimension: usize,
}

fn default_provider() -> String {
    "gemini".to_string()
}

fn default_model() -> String {
    "text-embedding-004".to_string()
}

fn default_dimension() -> usize {
    768
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_model(),
            dimension: default_dimension(),
        }
    }
}

/// Service for hash-based storage in the fold/ directory.
pub struct FoldStorageService {
    // No state needed - operates on paths provided by caller
}

impl Default for FoldStorageService {
    fn default() -> Self {
        Self::new()
    }
}

impl FoldStorageService {
    /// Create a new fold storage service.
    pub fn new() -> Self {
        Self {}
    }

    /// Get the path to a memory file based on its hash.
    ///
    /// The path structure is: fold/a/b/aBcD123.md
    /// where 'a' and 'b' are the first two hex characters of the hash.
    pub fn get_memory_path(&self, project_root: &Path, hash: &str) -> PathBuf {
        let char1 = if !hash.is_empty() { &hash[0..1] } else { "0" };
        let char2 = if hash.len() >= 2 { &hash[1..2] } else { "0" };

        project_root
            .join("fold")
            .join(char1)
            .join(char2)
            .join(format!("{}.md", hash))
    }

    /// Get the fold/ directory path for a project.
    pub fn get_fold_path(&self, project_root: &Path) -> PathBuf {
        project_root.join("fold")
    }

    /// Write a memory to the fold/ directory.
    ///
    /// Creates the hash-based directory structure and writes the memory
    /// as a markdown file with YAML frontmatter.
    pub async fn write_memory<M: MemoryData>(
        &self,
        project_root: &Path,
        memory: &M,
        content: &str,
    ) -> Result<PathBuf> {
        self.write_memory_with_links(project_root, memory, content, &[])
            .await
    }

    /// Write a memory with wiki-style links to related memories.
    ///
    /// Related memory IDs are included both in the frontmatter and as
    /// navigable [[wiki-style]] links in the body.
    pub async fn write_memory_with_links<M: MemoryData>(
        &self,
        project_root: &Path,
        memory: &M,
        content: &str,
        related_ids: &[String],
    ) -> Result<PathBuf> {
        let file_path = self.get_memory_path(project_root, memory.id());

        // Log this write operation
        info!(
            memory_id = %memory.id(),
            file_path = %file_path.display(),
            related_count = related_ids.len(),
            related_ids = ?related_ids,
            "FOLD_WRITE: Writing memory file"
        );

        // Ensure parent directories exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                Error::Internal(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Build frontmatter with related_to populated
        let mut frontmatter = MemoryFrontmatter::from_memory(memory);
        frontmatter.related_to = related_ids.to_vec();
        let yaml = serde_yaml::to_string(&frontmatter)
            .map_err(|e| Error::Internal(format!("Failed to serialize frontmatter: {}", e)))?;

        // Build content with wiki-style links appended
        let mut full_content = content.to_string();
        if !related_ids.is_empty() {
            full_content.push_str("\n\n---\n\n## Related\n\n");
            for id in related_ids {
                // Wiki-style link: [[id]] with relative path
                let char1 = &id[0..1];
                let char2 = &id[1..2];
                full_content.push_str(&format!("- [[{}/{}/{}.md|{}]]\n", char1, char2, id, id));
            }
        }

        // Write file with frontmatter + content
        let file_content = format!("---\n{}---\n\n{}", yaml, full_content);

        // Use atomic write: write to temp file then rename
        let temp_path = file_path.with_extension("tmp");
        fs::write(&temp_path, &file_content).await.map_err(|e| {
            Error::Internal(format!(
                "Failed to write memory file {}: {}",
                temp_path.display(),
                e
            ))
        })?;
        fs::rename(&temp_path, &file_path).await.map_err(|e| {
            Error::Internal(format!(
                "Failed to rename memory file to {}: {}",
                file_path.display(),
                e
            ))
        })?;

        Ok(file_path)
    }

    /// Update a memory file with new related links.
    ///
    /// Re-writes the memory file with updated wiki-style links.
    pub async fn update_memory_links(
        &self,
        project_root: &Path,
        memory_id: &str,
        related_ids: &[String],
    ) -> Result<PathBuf> {
        info!(
            memory_id = %memory_id,
            related_count = related_ids.len(),
            related_ids = ?related_ids,
            "FOLD_UPDATE_LINKS: Updating memory with links"
        );

        // Read existing memory
        let (memory, content) = self.read_memory(project_root, memory_id).await?;

        // Strip any existing Related section from content
        let clean_content = if let Some(idx) = content.find("\n---\n\n## Related") {
            content[..idx].to_string()
        } else {
            content
        };

        // Re-write with new links
        self.write_memory_with_links(project_root, &memory, &clean_content, related_ids)
            .await
    }

    /// Read a memory from the fold/ directory.
    ///
    /// Returns the StorageMemory metadata and body content.
    pub async fn read_memory(
        &self,
        project_root: &Path,
        hash: &str,
    ) -> Result<(StorageMemory, String)> {
        let file_path = self.get_memory_path(project_root, hash);
        let content = fs::read_to_string(&file_path).await.map_err(|e| {
            Error::FileNotFound(format!(
                "Memory file not found {}: {}",
                file_path.display(),
                e
            ))
        })?;

        self.parse_memory_file(&content)
    }

    /// Parse memory file content (frontmatter + body).
    ///
    /// Splits the file into YAML frontmatter and markdown body,
    /// then deserializes the frontmatter into a StorageMemory.
    pub fn parse_memory_file(&self, content: &str) -> Result<(StorageMemory, String)> {
        // Split frontmatter and body
        if !content.starts_with("---") {
            return Err(Error::InvalidInput(
                "Invalid memory file format: missing frontmatter".to_string(),
            ));
        }

        let rest = &content[3..];
        let end_marker = rest.find("\n---");

        let (frontmatter_str, body) = match end_marker {
            Some(pos) => {
                let yaml = &rest[..pos];
                let after_marker = pos + 4; // "\n---"
                let body = if after_marker < rest.len() {
                    rest[after_marker..].trim_start_matches('\n').trim()
                } else {
                    ""
                };
                (yaml.trim(), body)
            }
            None => {
                return Err(Error::InvalidInput(
                    "Invalid memory file format: unclosed frontmatter".to_string(),
                ));
            }
        };

        // Parse frontmatter
        let frontmatter: MemoryFrontmatter = serde_yaml::from_str(frontmatter_str)
            .map_err(|e| Error::InvalidInput(format!("Failed to parse frontmatter: {}", e)))?;

        let memory = frontmatter.to_storage_memory();

        Ok((memory, body.to_string()))
    }

    /// Scan the fold/ directory for all memory hashes.
    ///
    /// Walks the hash-based directory tree and collects all memory IDs
    /// (extracted from filenames).
    pub async fn scan_fold_directory(&self, project_root: &Path) -> Result<Vec<String>> {
        let fold_path = project_root.join("fold");
        let mut hashes = Vec::new();

        if !fold_path.exists() {
            return Ok(hashes);
        }

        // Walk first level (first hex char)
        let mut entries = fs::read_dir(&fold_path).await.map_err(|e| {
            Error::Internal(format!(
                "Failed to read fold directory {}: {}",
                fold_path.display(),
                e
            ))
        })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read directory entry: {}", e)))?
        {
            let entry_path = entry.path();

            // Skip non-directories and special files (project.toml, .gitignore)
            if !entry.file_type().await.map_or(false, |t| t.is_dir()) {
                continue;
            }

            // Walk second level (second hex char)
            let mut sub_entries = match fs::read_dir(&entry_path).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            while let Some(sub_entry) = sub_entries.next_entry().await.unwrap_or(None) {
                let sub_path = sub_entry.path();

                if !sub_entry.file_type().await.map_or(false, |t| t.is_dir()) {
                    continue;
                }

                // Walk memory files
                let mut file_entries = match fs::read_dir(&sub_path).await {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                while let Some(file_entry) = file_entries.next_entry().await.unwrap_or(None) {
                    let file_path = file_entry.path();

                    if let Some(ext) = file_path.extension() {
                        if ext == "md" {
                            if let Some(stem) = file_path.file_stem() {
                                hashes.push(stem.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(hashes)
    }

    /// Check if a memory file exists.
    pub async fn exists(&self, project_root: &Path, hash: &str) -> bool {
        let path = self.get_memory_path(project_root, hash);
        fs::metadata(&path).await.is_ok()
    }

    /// Delete a memory file.
    pub async fn delete_memory(&self, project_root: &Path, hash: &str) -> Result<()> {
        let path = self.get_memory_path(project_root, hash);

        if fs::metadata(&path).await.is_ok() {
            fs::remove_file(&path).await.map_err(|e| {
                Error::Internal(format!(
                    "Failed to delete memory file {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }

        Ok(())
    }

    /// Read project.toml configuration.
    pub async fn read_project_config(&self, project_root: &Path) -> Result<ProjectConfig> {
        let config_path = project_root.join("fold").join("project.toml");
        let content = fs::read_to_string(&config_path).await.map_err(|e| {
            Error::FileNotFound(format!(
                "Project config not found {}: {}",
                config_path.display(),
                e
            ))
        })?;

        let config: ProjectConfig = toml::from_str(&content)
            .map_err(|e| Error::InvalidInput(format!("Failed to parse project.toml: {}", e)))?;

        Ok(config)
    }

    /// Write project.toml configuration.
    ///
    /// Also creates a .gitignore file in the fold/ directory.
    pub async fn write_project_config(
        &self,
        project_root: &Path,
        config: &ProjectConfig,
    ) -> Result<()> {
        let fold_path = project_root.join("fold");
        fs::create_dir_all(&fold_path).await.map_err(|e| {
            Error::Internal(format!(
                "Failed to create fold directory {}: {}",
                fold_path.display(),
                e
            ))
        })?;

        // Write project.toml
        let config_path = fold_path.join("project.toml");
        let content: String = toml::to_string_pretty(config)
            .map_err(|e| Error::Internal(format!("Failed to serialize project.toml: {}", e)))?;
        fs::write(&config_path, &content)
            .await
            .map_err(|e: std::io::Error| {
                Error::Internal(format!(
                    "Failed to write project.toml {}: {}",
                    config_path.display(),
                    e
                ))
            })?;

        // Create .gitignore
        let gitignore_path = fold_path.join(".gitignore");
        let gitignore_content = "*.tmp\n*.lock\n";
        fs::write(&gitignore_path, gitignore_content)
            .await
            .map_err(|e| {
                Error::Internal(format!(
                    "Failed to write .gitignore {}: {}",
                    gitignore_path.display(),
                    e
                ))
            })?;

        Ok(())
    }

    /// Initialize a new fold/ directory for a project.
    pub async fn init_fold_directory(
        &self,
        project_root: &Path,
        project_id: &str,
        project_slug: &str,
        project_name: &str,
    ) -> Result<()> {
        let config = ProjectConfig {
            project: ProjectInfo {
                id: project_id.to_string(),
                slug: project_slug.to_string(),
                name: project_name.to_string(),
                created_at: Utc::now(),
            },
            indexing: IndexingConfig::default(),
            embedding: EmbeddingConfig::default(),
        };

        self.write_project_config(project_root, &config).await
    }

    /// Check if a fold/ directory exists and is initialised.
    pub async fn is_initialised(&self, project_root: &Path) -> bool {
        let config_path = project_root.join("fold").join("project.toml");
        fs::metadata(&config_path).await.is_ok()
    }
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

        let (memory, body) = service.parse_memory_file(content).unwrap();

        assert_eq!(memory.id, "aBcD123456789abc");
        assert_eq!(memory.title, Some("Test Memory".to_string()));
        assert_eq!(memory.author, Some("claude".to_string()));
        assert_eq!(memory.memory_type, "session");
        assert!(body.contains("This is the memory content."));
        assert!(body.contains("## Section"));
    }

    #[test]
    fn test_frontmatter_conversion() {
        let now = Utc::now();
        let frontmatter = MemoryFrontmatter {
            id: "test123".to_string(),
            title: Some("Test".to_string()),
            author: Some("claude".to_string()),
            tags: vec!["tag1".to_string()],
            file_path: Some("src/main.rs".to_string()),
            language: Some("rust".to_string()),
            memory_type: "codebase".to_string(),
            original_date: None,
            created_at: now,
            updated_at: now,
            related_to: vec![],
        };

        let memory = frontmatter.to_storage_memory();
        assert_eq!(memory.id, "test123");
        assert_eq!(memory.title, Some("Test".to_string()));
        assert_eq!(memory.memory_type, "codebase");
        assert_eq!(memory.file_path, Some("src/main.rs".to_string()));

        let tags = memory.tags_vec();
        assert_eq!(tags, vec!["tag1".to_string()]);
    }
}
