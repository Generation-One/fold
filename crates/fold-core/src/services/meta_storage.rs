//! Filesystem-centric memory storage service.
//!
//! Provides a storage layer that persists memories as markdown files
//! on the filesystem, enabling version control and direct editing.
//!
//! Memory files use YAML frontmatter for metadata:
//! ```markdown
//! ---
//! id: mem_abc123
//! type: session
//! title: "Session title"
//! author: claude
//! tags: ["tag1", "tag2"]
//! created_at: 2024-01-15T10:30:00Z
//! ---
//!
//! Content goes here...
//! ```

use crate::models::{Memory, MemoryType};
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Parsed memory file with frontmatter metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFileContent {
    pub id: String,
    pub memory_type: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub keywords: Vec<String>,
    pub context: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub content: String,
}

/// YAML frontmatter structure for memory files.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryFrontmatter {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<DateTime<Utc>>,
}

/// Service for filesystem-based memory storage.
pub struct MetaStorageService {
    base_path: PathBuf,
}

impl MetaStorageService {
    /// Create a new meta storage service.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Get the base path for storage.
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Get the storage path for a project.
    pub fn project_path(&self, project_slug: &str) -> PathBuf {
        self.base_path.join(project_slug)
    }

    /// Get the storage path for a memory.
    pub fn memory_path(&self, project_slug: &str, memory_id: &str) -> PathBuf {
        self.project_path(project_slug)
            .join("memories")
            .join(format!("{}.md", memory_id))
    }

    /// Get the absolute path for a memory file.
    pub fn memory_absolute_path(&self, project_slug: &str, memory_id: &str) -> PathBuf {
        self.memory_path(project_slug, memory_id)
    }

    /// Check if a memory file exists.
    pub fn memory_exists(&self, project_slug: &str, memory_id: &str) -> bool {
        self.memory_path(project_slug, memory_id).exists()
    }

    /// Ensure project directory structure exists.
    pub async fn ensure_project_dirs(&self, project_slug: &str) -> Result<()> {
        let project_path = self.project_path(project_slug);
        fs::create_dir_all(project_path.join("memories"))
            .await
            .map_err(|e| Error::Internal(format!("Failed to create project directories: {}", e)))?;
        fs::create_dir_all(project_path.join("attachments"))
            .await
            .map_err(|e| {
                Error::Internal(format!("Failed to create attachments directory: {}", e))
            })?;
        Ok(())
    }

    /// Write a memory to the filesystem (raw content, no frontmatter).
    pub async fn write_memory(
        &self,
        project_slug: &str,
        memory_id: &str,
        content: &str,
    ) -> Result<()> {
        self.ensure_project_dirs(project_slug).await?;
        let path = self.memory_path(project_slug, memory_id);

        // Use atomic write: write to temp file then rename
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, content)
            .await
            .map_err(|e| Error::Internal(format!("Failed to write memory file: {}", e)))?;
        fs::rename(&temp_path, &path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to rename memory file: {}", e)))?;

        Ok(())
    }

    /// Write a memory with YAML frontmatter metadata.
    pub async fn write_memory_with_frontmatter(
        &self,
        project_slug: &str,
        memory: &Memory,
        content: &str,
    ) -> Result<()> {
        let frontmatter = MemoryFrontmatter {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            author: memory.author.clone(),
            tags: memory.tags_vec(),
            keywords: memory.keywords_vec(),
            context: memory.context.clone(),
            created_at: Some(memory.created_at),
            updated_at: Some(memory.updated_at),
        };

        let yaml = serde_yaml::to_string(&frontmatter)
            .map_err(|e| Error::Internal(format!("Failed to serialize frontmatter: {}", e)))?;

        let file_content = format!("---\n{}---\n\n{}", yaml, content);
        self.write_memory(project_slug, &memory.id, &file_content)
            .await
    }

    /// Read raw memory content from the filesystem (includes frontmatter if present).
    pub async fn read_memory(&self, project_slug: &str, memory_id: &str) -> Result<Option<String>> {
        let path = self.memory_path(project_slug, memory_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read memory file: {}", e)))?;
        Ok(Some(content))
    }

    /// Read memory content only (strips frontmatter if present).
    pub async fn read_memory_content(
        &self,
        project_slug: &str,
        memory_id: &str,
    ) -> Result<Option<String>> {
        let raw = match self.read_memory(project_slug, memory_id).await? {
            Some(content) => content,
            None => return Ok(None),
        };

        // Parse and strip frontmatter
        let content = Self::strip_frontmatter(&raw);
        Ok(Some(content))
    }

    /// Read memory with parsed frontmatter metadata.
    pub async fn read_memory_with_frontmatter(
        &self,
        project_slug: &str,
        memory_id: &str,
    ) -> Result<Option<MemoryFileContent>> {
        let raw = match self.read_memory(project_slug, memory_id).await? {
            Some(content) => content,
            None => return Ok(None),
        };

        let parsed = Self::parse_frontmatter(&raw, memory_id)?;
        Ok(Some(parsed))
    }

    /// Parse frontmatter from file content.
    fn parse_frontmatter(raw: &str, memory_id: &str) -> Result<MemoryFileContent> {
        if !raw.starts_with("---") {
            // No frontmatter, treat entire file as content
            return Ok(MemoryFileContent {
                id: memory_id.to_string(),
                memory_type: MemoryType::General.as_str().to_string(),
                title: None,
                author: None,
                tags: Vec::new(),
                keywords: Vec::new(),
                context: None,
                created_at: None,
                updated_at: None,
                content: raw.to_string(),
            });
        }

        // Find the closing ---
        let rest = &raw[3..];
        let end_marker = rest.find("\n---");
        let (yaml_str, content) = match end_marker {
            Some(pos) => {
                let yaml = &rest[..pos];
                let after_marker = pos + 4; // "\n---"
                let content = if after_marker < rest.len() {
                    rest[after_marker..].trim_start_matches('\n')
                } else {
                    ""
                };
                (yaml, content)
            }
            None => {
                // Malformed frontmatter, treat as content
                return Ok(MemoryFileContent {
                    id: memory_id.to_string(),
                    memory_type: MemoryType::General.as_str().to_string(),
                    title: None,
                    author: None,
                    tags: Vec::new(),
                    keywords: Vec::new(),
                    context: None,
                    created_at: None,
                    updated_at: None,
                    content: raw.to_string(),
                });
            }
        };

        let frontmatter: MemoryFrontmatter = serde_yaml::from_str(yaml_str)
            .map_err(|e| Error::Internal(format!("Failed to parse frontmatter: {}", e)))?;

        Ok(MemoryFileContent {
            id: frontmatter.id,
            memory_type: frontmatter.memory_type,
            title: frontmatter.title,
            author: frontmatter.author,
            tags: frontmatter.tags,
            keywords: frontmatter.keywords,
            context: frontmatter.context,
            created_at: frontmatter.created_at,
            updated_at: frontmatter.updated_at,
            content: content.to_string(),
        })
    }

    /// Strip frontmatter from file content, returning just the body.
    fn strip_frontmatter(raw: &str) -> String {
        if !raw.starts_with("---") {
            return raw.to_string();
        }

        let rest = &raw[3..];
        match rest.find("\n---") {
            Some(pos) => {
                let after_marker = pos + 4;
                if after_marker < rest.len() {
                    rest[after_marker..].trim_start_matches('\n').to_string()
                } else {
                    String::new()
                }
            }
            None => raw.to_string(),
        }
    }

    /// Delete a memory from the filesystem.
    pub async fn delete_memory(&self, project_slug: &str, memory_id: &str) -> Result<()> {
        let path = self.memory_path(project_slug, memory_id);
        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| Error::Internal(format!("Failed to delete memory file: {}", e)))?;
        }
        Ok(())
    }

    /// List all memory IDs in a project.
    pub async fn list_memories(&self, project_slug: &str) -> Result<Vec<String>> {
        let memories_path = self.project_path(project_slug).join("memories");
        if !memories_path.exists() {
            return Ok(vec![]);
        }

        let mut entries = fs::read_dir(&memories_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read memories directory: {}", e)))?;

        let mut ids = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read directory entry: {}", e)))?
        {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".md") {
                    ids.push(name.trim_end_matches(".md").to_string());
                }
            }
        }

        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_frontmatter() {
        let with_fm = "---\nid: test\ntype: session\n---\n\nContent here";
        assert_eq!(
            MetaStorageService::strip_frontmatter(with_fm),
            "Content here"
        );

        let without_fm = "Just content";
        assert_eq!(
            MetaStorageService::strip_frontmatter(without_fm),
            "Just content"
        );

        let empty_content = "---\nid: test\n---\n";
        assert_eq!(MetaStorageService::strip_frontmatter(empty_content), "");
    }

    #[test]
    fn test_parse_frontmatter() {
        let raw = "---\nid: mem_123\ntype: session\ntitle: Test\n---\n\nContent";
        let parsed = MetaStorageService::parse_frontmatter(raw, "mem_123").unwrap();

        assert_eq!(parsed.id, "mem_123");
        assert_eq!(parsed.memory_type, "session");
        assert_eq!(parsed.title, Some("Test".to_string()));
        assert_eq!(parsed.content, "Content");
    }
}
