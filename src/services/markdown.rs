//! Markdown parsing and serialization service.
//!
//! Handles parsing of markdown files with YAML frontmatter for memory storage.

use crate::Result;
use serde::{Deserialize, Serialize};

/// Frontmatter for a memory stored as markdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    /// Memory title
    pub title: Option<String>,
    /// Memory type (codebase, session, spec, decision, task, general)
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    /// Author of the memory
    pub author: Option<String>,
    /// Tags for categorisation
    #[serde(default)]
    pub tags: Vec<String>,
    /// Links to other memories
    #[serde(default)]
    pub links: Vec<FrontmatterLink>,
    /// Attached files
    #[serde(default)]
    pub attachments: Vec<FrontmatterAttachment>,
    /// File path (for codebase memories)
    pub file_path: Option<String>,
    /// Programming language
    pub language: Option<String>,
}

/// A link to another memory in frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmatterLink {
    /// Target memory ID or path
    pub target: String,
    /// Relationship type
    #[serde(rename = "type")]
    pub link_type: Option<String>,
    /// Optional label
    pub label: Option<String>,
}

/// An attachment reference in frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmatterAttachment {
    /// Filename
    pub filename: String,
    /// Content type
    pub content_type: Option<String>,
    /// Storage path or URL
    pub path: Option<String>,
}

/// Service for parsing and generating markdown with frontmatter.
pub struct MarkdownService;

impl MarkdownService {
    /// Create a new markdown service.
    pub fn new() -> Self {
        Self
    }

    /// Parse markdown content with YAML frontmatter.
    pub fn parse(&self, content: &str) -> Result<(MemoryFrontmatter, String)> {
        // Check for frontmatter delimiter
        if !content.starts_with("---") {
            return Ok((MemoryFrontmatter::default(), content.to_string()));
        }

        // Find end of frontmatter
        let rest = &content[3..];
        if let Some(end_idx) = rest.find("\n---") {
            let frontmatter_str = &rest[..end_idx];
            let body = rest[end_idx + 4..].trim_start().to_string();

            let frontmatter: MemoryFrontmatter =
                serde_yaml::from_str(frontmatter_str).unwrap_or_default();

            Ok((frontmatter, body))
        } else {
            Ok((MemoryFrontmatter::default(), content.to_string()))
        }
    }

    /// Generate markdown with frontmatter from components.
    pub fn generate(&self, frontmatter: &MemoryFrontmatter, body: &str) -> Result<String> {
        let yaml = serde_yaml::to_string(frontmatter)
            .map_err(|e| crate::Error::Internal(format!("Failed to serialize frontmatter: {}", e)))?;

        Ok(format!("---\n{}---\n\n{}", yaml, body))
    }

    /// Extract just the body without frontmatter.
    pub fn extract_body(&self, content: &str) -> String {
        self.parse(content)
            .map(|(_, body)| body)
            .unwrap_or_else(|_| content.to_string())
    }
}

impl Default for MarkdownService {
    fn default() -> Self {
        Self::new()
    }
}
