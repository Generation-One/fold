//! Memory model for storing project knowledge.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

/// Types of memories that can be stored
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Indexed source code
    Codebase,
    /// Coding session summaries
    Session,
    /// Requirements & specifications
    Spec,
    /// Architecture decisions
    Decision,
    /// Current work items
    Task,
    /// General knowledge
    General,
    /// Git commit summaries
    Commit,
    /// Pull request summaries
    Pr,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Codebase => "codebase",
            MemoryType::Session => "session",
            MemoryType::Spec => "spec",
            MemoryType::Decision => "decision",
            MemoryType::Task => "task",
            MemoryType::General => "general",
            MemoryType::Commit => "commit",
            MemoryType::Pr => "pr",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "codebase" => Some(MemoryType::Codebase),
            "session" => Some(MemoryType::Session),
            "spec" => Some(MemoryType::Spec),
            "decision" => Some(MemoryType::Decision),
            "task" => Some(MemoryType::Task),
            "general" => Some(MemoryType::General),
            "commit" => Some(MemoryType::Commit),
            "pr" => Some(MemoryType::Pr),
            _ => None,
        }
    }

    pub fn all() -> &'static [MemoryType] {
        &[
            MemoryType::Codebase,
            MemoryType::Session,
            MemoryType::Spec,
            MemoryType::Decision,
            MemoryType::Task,
            MemoryType::General,
            MemoryType::Commit,
            MemoryType::Pr,
        ]
    }
}

impl Default for MemoryType {
    fn default() -> Self {
        MemoryType::General
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Link type for memory relationships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// Memory modifies another
    Modifies,
    /// Memory contains another
    Contains,
    /// Memory affects another
    Affects,
    /// Memory implements a spec/decision
    Implements,
    /// Memory records a decision
    Decides,
    /// Memory supersedes another
    Supersedes,
    /// Memory references another
    References,
    /// Memories are related
    Related,
    /// Parent-child relationship
    Parent,
    /// Memory blocks another
    Blocks,
    /// Memory caused by another
    CausedBy,
}

impl LinkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkType::Modifies => "modifies",
            LinkType::Contains => "contains",
            LinkType::Affects => "affects",
            LinkType::Implements => "implements",
            LinkType::Decides => "decides",
            LinkType::Supersedes => "supersedes",
            LinkType::References => "references",
            LinkType::Related => "related",
            LinkType::Parent => "parent",
            LinkType::Blocks => "blocks",
            LinkType::CausedBy => "caused_by",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "modifies" => Some(LinkType::Modifies),
            "contains" => Some(LinkType::Contains),
            "affects" => Some(LinkType::Affects),
            "implements" => Some(LinkType::Implements),
            "decides" => Some(LinkType::Decides),
            "supersedes" => Some(LinkType::Supersedes),
            "references" => Some(LinkType::References),
            "related" => Some(LinkType::Related),
            "parent" => Some(LinkType::Parent),
            "blocks" => Some(LinkType::Blocks),
            "caused_by" => Some(LinkType::CausedBy),
            _ => None,
        }
    }

    pub fn all() -> &'static [LinkType] {
        &[
            LinkType::Modifies,
            LinkType::Contains,
            LinkType::Affects,
            LinkType::Implements,
            LinkType::Decides,
            LinkType::Supersedes,
            LinkType::References,
            LinkType::Related,
            LinkType::Parent,
            LinkType::Blocks,
            LinkType::CausedBy,
        ]
    }
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Request model for creating a memory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryCreate {
    #[serde(default)]
    pub memory_type: MemoryType,
    pub content: String,
    pub author: Option<String>,

    // Optional metadata (auto-generated if not provided)
    pub title: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub context: Option<String>,

    // For codebase type
    pub file_path: Option<String>,
    pub language: Option<String>,

    // For task type
    pub status: Option<String>,
    pub assignee: Option<String>,

    // Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for MemoryCreate {
    fn default() -> Self {
        Self {
            memory_type: MemoryType::General,
            content: String::new(),
            author: None,
            title: None,
            keywords: Vec::new(),
            tags: Vec::new(),
            context: None,
            file_path: None,
            language: None,
            status: None,
            assignee: None,
            metadata: HashMap::new(),
        }
    }
}

/// Request model for updating a memory
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryUpdate {
    pub content: Option<String>,
    pub title: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub context: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// A memory representing a piece of project knowledge
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct Memory {
    pub id: String,
    pub project_id: String,
    /// 'codebase', 'session', 'spec', 'decision', 'task', 'general'
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub memory_type: String,
    pub content: String,

    // Metadata
    pub title: Option<String>,
    pub author: Option<String>,
    /// JSON array of keywords
    pub keywords: Option<String>,
    /// JSON array of tags
    pub tags: Option<String>,
    pub context: Option<String>,

    // For codebase type
    pub file_path: Option<String>,
    pub language: Option<String>,
    pub line_start: Option<i32>,
    pub line_end: Option<i32>,

    // For task type
    pub status: Option<String>,
    pub assignee: Option<String>,

    // Custom metadata as JSON
    pub metadata: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Usage tracking
    pub retrieval_count: i32,
    pub last_accessed: Option<DateTime<Utc>>,
}

impl Memory {
    /// Get the typed memory type
    pub fn get_type(&self) -> Option<MemoryType> {
        MemoryType::from_str(&self.memory_type)
    }

    /// Parse keywords from JSON string
    pub fn keywords_vec(&self) -> Vec<String> {
        self.keywords
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Parse tags from JSON string
    pub fn tags_vec(&self) -> Vec<String> {
        self.tags
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Parse metadata from JSON string
    pub fn metadata_map(&self) -> HashMap<String, serde_json::Value> {
        self.metadata
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Generate text for embedding/search
    pub fn to_search_text(&self) -> String {
        let mut parts = Vec::new();

        if let Some(title) = &self.title {
            parts.push(title.clone());
        }

        parts.push(self.content.clone());

        if let Some(context) = &self.context {
            parts.push(context.clone());
        }

        let keywords = self.keywords_vec();
        if !keywords.is_empty() {
            parts.push(keywords.join(" "));
        }

        if let Some(file_path) = &self.file_path {
            parts.push(format!("file: {}", file_path));
        }

        parts.join("\n")
    }

    /// Create a new memory with generated ID
    pub fn new(project_id: String, memory_type: MemoryType, content: String) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            project_id,
            memory_type: memory_type.as_str().to_string(),
            content,
            title: None,
            author: None,
            keywords: None,
            tags: None,
            context: None,
            file_path: None,
            language: None,
            line_start: None,
            line_end: None,
            status: None,
            assignee: None,
            metadata: None,
            created_at: now,
            updated_at: now,
            retrieval_count: 0,
            last_accessed: None,
        }
    }
}

/// Search result with score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub memory: Memory,
    pub score: f32,
}

/// Code summary generated by LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSummary {
    pub title: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    /// Key functions/classes/exports
    pub exports: Vec<String>,
    /// Dependencies/imports
    pub dependencies: Vec<String>,
}

impl Default for CodeSummary {
    fn default() -> Self {
        Self {
            title: String::new(),
            summary: String::new(),
            keywords: Vec::new(),
            tags: Vec::new(),
            language: None,
            exports: Vec::new(),
            dependencies: Vec::new(),
        }
    }
}

/// Suggested link between memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedLink {
    pub source_id: String,
    pub target_id: String,
    pub link_type: String,
    pub confidence: f32,
    pub reason: String,
}

/// Commit info for summarization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: Option<String>,
    pub files: Vec<CommitFile>,
    pub insertions: i32,
    pub deletions: i32,
}

/// File changed in a commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitFile {
    pub path: String,
    pub status: String,
    pub patch: Option<String>,
}

/// Link between two memories in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct MemoryLink {
    pub id: String,
    pub project_id: String,
    pub source_id: String,
    pub target_id: String,
    /// Link type: 'modifies', 'contains', 'affects', 'implements', 'decides',
    /// 'supersedes', 'references', 'related', 'parent', 'blocks', 'caused_by'
    pub link_type: String,
    /// Who created this link: 'system', 'user', or 'ai'
    pub created_by: String,
    /// For AI-suggested links (0.0-1.0)
    pub confidence: Option<f64>,
    /// Why this link exists
    pub context: Option<String>,
    // For code links
    /// 'added', 'modified', 'deleted'
    pub change_type: Option<String>,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
    pub created_at: DateTime<Utc>,
}

impl MemoryLink {
    /// Get the typed link type
    pub fn get_link_type(&self) -> Option<LinkType> {
        LinkType::from_str(&self.link_type)
    }

    /// Create a new link with generated ID
    pub fn new(
        project_id: String,
        source_id: String,
        target_id: String,
        link_type: LinkType,
        created_by: String,
    ) -> Self {
        Self {
            id: super::new_id(),
            project_id,
            source_id,
            target_id,
            link_type: link_type.as_str().to_string(),
            created_by,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
            created_at: Utc::now(),
        }
    }
}

/// Attachment associated with a memory
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct Attachment {
    pub id: String,
    pub memory_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub created_at: DateTime<Utc>,
}
