//! Chunk model for semantic code/text chunks.
//!
//! Chunks are fine-grained pieces of source code or text
//! extracted using tree-sitter AST parsing or text-based splitting.
//! They enable precise search and similarity matching at the
//! function/class/section level rather than whole-file level.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A semantic chunk of code or text.
///
/// Chunks are linked to a parent memory and stored in both
/// SQLite (for metadata) and Qdrant (for vector search).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Unique identifier
    pub id: String,
    /// Parent memory ID
    pub memory_id: String,
    /// Project ID
    pub project_id: String,

    /// The actual content of the chunk
    pub content: String,
    /// SHA256 hash of content for change detection
    pub content_hash: String,

    /// Starting line number (1-indexed)
    pub start_line: i32,
    /// Ending line number (1-indexed)
    pub end_line: i32,
    /// Starting byte offset
    pub start_byte: i32,
    /// Ending byte offset
    pub end_byte: i32,

    /// Type of node: "function", "class", "struct", "heading", "paragraph", etc.
    pub node_type: String,
    /// Name of the node if available (function name, heading text, etc.)
    pub node_name: Option<String>,
    /// Programming language or format
    pub language: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Chunk {
    /// Calculate SHA256 hash of content
    pub fn hash_content(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Generate a deterministic chunk ID from memory ID and content hash
    pub fn generate_id(memory_id: &str, content_hash: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", memory_id, content_hash).as_bytes());
        let result = hasher.finalize();
        let full_hash = hex::encode(&result);
        // Format as UUID-like string
        format!(
            "{}-{}-{}-{}-{}",
            &full_hash[0..8],
            &full_hash[8..12],
            &full_hash[12..16],
            &full_hash[16..20],
            &full_hash[20..32]
        )
    }

    /// Get a short snippet of the content (first N chars)
    pub fn snippet(&self, max_len: usize) -> String {
        if self.content.len() <= max_len {
            self.content.clone()
        } else {
            format!("{}...", &self.content[..max_len])
        }
    }

    /// Get the line count of this chunk
    pub fn line_count(&self) -> i32 {
        self.end_line - self.start_line + 1
    }
}

/// Data for creating a new chunk
#[derive(Debug, Clone)]
pub struct ChunkCreate {
    pub memory_id: String,
    pub project_id: String,
    pub content: String,
    pub start_line: i32,
    pub end_line: i32,
    pub start_byte: i32,
    pub end_byte: i32,
    pub node_type: String,
    pub node_name: Option<String>,
    pub language: String,
}

impl ChunkCreate {
    /// Convert to a Chunk with generated ID and timestamps
    pub fn into_chunk(self) -> Chunk {
        let content_hash = Chunk::hash_content(&self.content);
        let id = Chunk::generate_id(&self.memory_id, &content_hash);
        let now = Utc::now();

        Chunk {
            id,
            memory_id: self.memory_id,
            project_id: self.project_id,
            content: self.content,
            content_hash,
            start_line: self.start_line,
            end_line: self.end_line,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            node_type: self.node_type,
            node_name: self.node_name,
            language: self.language,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Chunk search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSearchResult {
    pub chunk: Chunk,
    /// Vector similarity score (0.0-1.0)
    pub score: f32,
    /// File path from parent memory
    pub file_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_hash() {
        let hash = Chunk::hash_content("fn hello() {}");
        assert_eq!(hash.len(), 64); // SHA256 hex is 64 chars
    }

    #[test]
    fn test_chunk_id_generation() {
        let id = Chunk::generate_id("mem-123", "abc123");
        // Should be UUID-like format
        assert!(id.contains('-'));
        assert_eq!(id.len(), 36);
    }

    #[test]
    fn test_chunk_snippet() {
        let chunk = Chunk {
            id: "test".to_string(),
            memory_id: "mem".to_string(),
            project_id: "proj".to_string(),
            content: "This is a long content string that should be truncated".to_string(),
            content_hash: "hash".to_string(),
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            end_byte: 54,
            node_type: "test".to_string(),
            node_name: None,
            language: "text".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let snippet = chunk.snippet(20);
        assert_eq!(snippet, "This is a long conte...");
    }
}
