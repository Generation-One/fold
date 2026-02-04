//! Memory model for storing project knowledge.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "sqlx")]
use sqlx::FromRow;

use crate::{new_id, now};

/// Source of a memory - how it was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// Created by an AI agent (Claude, etc)
    Agent,
    /// Indexed from a source file
    File,
    /// Derived from git history (commits, PRs)
    Git,
}

impl MemorySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemorySource::Agent => "agent",
            MemorySource::File => "file",
            MemorySource::Git => "git",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "agent" => Some(MemorySource::Agent),
            "file" => Some(MemorySource::File),
            "git" => Some(MemorySource::Git),
            _ => None,
        }
    }
}

impl Default for MemorySource {
    fn default() -> Self {
        MemorySource::Agent
    }
}

impl std::fmt::Display for MemorySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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

/// Link type for memory relationships (legacy, see links.rs for simplified version)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyLinkType {
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

impl LegacyLinkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LegacyLinkType::Modifies => "modifies",
            LegacyLinkType::Contains => "contains",
            LegacyLinkType::Affects => "affects",
            LegacyLinkType::Implements => "implements",
            LegacyLinkType::Decides => "decides",
            LegacyLinkType::Supersedes => "supersedes",
            LegacyLinkType::References => "references",
            LegacyLinkType::Related => "related",
            LegacyLinkType::Parent => "parent",
            LegacyLinkType::Blocks => "blocks",
            LegacyLinkType::CausedBy => "caused_by",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "modifies" => Some(LegacyLinkType::Modifies),
            "contains" => Some(LegacyLinkType::Contains),
            "affects" => Some(LegacyLinkType::Affects),
            "implements" => Some(LegacyLinkType::Implements),
            "decides" => Some(LegacyLinkType::Decides),
            "supersedes" => Some(LegacyLinkType::Supersedes),
            "references" => Some(LegacyLinkType::References),
            "related" => Some(LegacyLinkType::Related),
            "parent" => Some(LegacyLinkType::Parent),
            "blocks" => Some(LegacyLinkType::Blocks),
            "caused_by" => Some(LegacyLinkType::CausedBy),
            _ => None,
        }
    }

    pub fn all() -> &'static [LegacyLinkType] {
        &[
            LegacyLinkType::Modifies,
            LegacyLinkType::Contains,
            LegacyLinkType::Affects,
            LegacyLinkType::Implements,
            LegacyLinkType::Decides,
            LegacyLinkType::Supersedes,
            LegacyLinkType::References,
            LegacyLinkType::Related,
            LegacyLinkType::Parent,
            LegacyLinkType::Blocks,
            LegacyLinkType::CausedBy,
        ]
    }
}

impl std::fmt::Display for LegacyLinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Request model for creating a memory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryCreate {
    /// Optional custom ID. If not provided, a UUID will be generated.
    /// For codebase files, this should be a hash of project_slug + file_path
    /// to ensure updates replace existing memories rather than creating duplicates.
    pub id: Option<String>,
    #[serde(default)]
    pub memory_type: MemoryType,
    pub content: String,
    pub author: Option<String>,
    /// Source of the memory (agent, file, git)
    pub source: Option<MemorySource>,

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
            id: None,
            memory_type: MemoryType::General,
            content: String::new(),
            author: None,
            source: None,
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

/// Parameters for memory search with decay weighting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchParams {
    /// The search query
    pub query: String,
    /// Filter by memory type
    #[serde(rename = "type")]
    pub memory_type: Option<MemoryType>,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Include graph-related memories
    #[serde(default = "default_true")]
    pub include_related: bool,
    /// Weight for retrieval strength vs semantic similarity (0.0-1.0).
    /// 0.0 = pure semantic, 1.0 = pure strength-based.
    /// Default: 0.3
    #[serde(default = "default_strength_weight")]
    pub strength_weight: f64,
    /// Half-life in days for memory decay.
    /// Shorter values favour very recent memories.
    /// Default: 30
    #[serde(default = "default_half_life")]
    pub decay_half_life_days: f64,
}

fn default_limit() -> usize {
    10
}

fn default_true() -> bool {
    true
}

fn default_strength_weight() -> f64 {
    0.3
}

fn default_half_life() -> f64 {
    30.0
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            memory_type: None,
            limit: 10,
            include_related: true,
            strength_weight: 0.3,
            decay_half_life_days: 30.0,
        }
    }
}

impl SearchParams {
    /// Create new search params with just a query.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Default::default()
        }
    }

    /// Set the memory type filter.
    pub fn with_type(mut self, memory_type: MemoryType) -> Self {
        self.memory_type = Some(memory_type);
        self
    }

    /// Set the result limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set whether to include related memories.
    pub fn with_related(mut self, include: bool) -> Self {
        self.include_related = include;
        self
    }

    /// Set the strength weight for decay blending.
    pub fn with_strength_weight(mut self, weight: f64) -> Self {
        self.strength_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Set the decay half-life in days.
    pub fn with_half_life(mut self, days: f64) -> Self {
        self.decay_half_life_days = days.max(1.0);
        self
    }

    /// Configure for pure semantic search (no decay weighting).
    pub fn pure_semantic(mut self) -> Self {
        self.strength_weight = 0.0;
        self
    }
}

/// Content storage type for memories.
///
/// **DEPRECATED**: Use the `source` field instead to determine content location:
/// - `source: "agent"` → content in fold/ directory
/// - `source: "file"` → content (LLM summary) in SQLite
/// - `source: "git"` → content (commit summary) in SQLite
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentStorage {
    /// Content stored in filesystem (fold/{project}/memories/{id}.md)
    #[default]
    Filesystem,
    /// Content read from source file (for codebase type)
    SourceFile,
}

impl ContentStorage {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentStorage::Filesystem => "filesystem",
            ContentStorage::SourceFile => "source_file",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "filesystem" => Some(ContentStorage::Filesystem),
            "source_file" => Some(ContentStorage::SourceFile),
            _ => None,
        }
    }
}

/// A memory representing a piece of project knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(FromRow))]
#[serde(rename_all = "snake_case")]
pub struct Memory {
    pub id: String,
    pub project_id: String,
    /// Repository ID for codebase memories
    pub repository_id: Option<String>,
    /// 'codebase', 'session', 'spec', 'decision', 'task', 'general'
    #[serde(rename = "type")]
    #[cfg_attr(feature = "sqlx", sqlx(rename = "type"))]
    pub memory_type: String,
    /// Source of the memory: 'agent', 'file', 'git'.
    /// Determines where content is stored:
    /// - 'agent': content in fold/ directory (manual/AI-created memories)
    /// - 'file': content (LLM summary) in SQLite (indexed source files)
    /// - 'git': content (commit summary) in SQLite (git history)
    pub source: Option<String>,
    /// Memory content.
    /// - For agent memories: NULL here, content in fold/ directory
    /// - For file/git memories: LLM summary stored here in SQLite
    #[serde(default)]
    pub content: Option<String>,
    /// SHA256 hash prefix for change detection
    pub content_hash: Option<String>,
    /// **DEPRECATED**: Use `source` field instead.
    /// Where content is stored: 'filesystem' or 'source_file'
    #[serde(default)]
    pub content_storage: Option<String>,

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

    /// Get the content storage type.
    pub fn get_content_storage(&self) -> ContentStorage {
        self.content_storage
            .as_ref()
            .and_then(|s| ContentStorage::from_str(s))
            .unwrap_or(ContentStorage::Filesystem)
    }

    /// Check if this memory needs content resolution (content stored externally).
    pub fn needs_content_resolution(&self) -> bool {
        self.content.is_none() || self.content.as_ref().is_some_and(|c| c.is_empty())
    }

    /// Get the content, returning empty string if not loaded.
    pub fn content_or_empty(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }

    /// Generate text for embedding/search.
    /// Note: content should be resolved before calling this.
    pub fn to_search_text(&self) -> String {
        let mut parts = Vec::new();

        if let Some(title) = &self.title {
            parts.push(title.clone());
        }

        if let Some(content) = &self.content {
            parts.push(content.clone());
        }

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

    /// Generate text for embedding/search using provided content.
    pub fn to_search_text_with_content(&self, content: &str) -> String {
        let mut parts = Vec::new();

        if let Some(title) = &self.title {
            parts.push(title.clone());
        }

        parts.push(content.to_string());

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

    /// Create a new memory with generated ID.
    /// Content is stored externally, so we only store metadata here.
    pub fn new(project_id: String, memory_type: MemoryType) -> Self {
        let now = now();
        let content_storage = match memory_type {
            MemoryType::Codebase => ContentStorage::SourceFile,
            _ => ContentStorage::Filesystem,
        };
        let source = match memory_type {
            MemoryType::Codebase => MemorySource::File,
            MemoryType::Commit | MemoryType::Pr => MemorySource::Git,
            _ => MemorySource::Agent,
        };
        Self {
            id: new_id(),
            project_id,
            repository_id: None,
            memory_type: memory_type.as_str().to_string(),
            source: Some(source.as_str().to_string()),
            content: None, // Content stored externally
            content_hash: None,
            content_storage: Some(content_storage.as_str().to_string()),
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

    /// Create a new memory with a specific ID (useful for deterministic codebase IDs).
    pub fn new_with_id(id: String, project_id: String, memory_type: MemoryType) -> Self {
        let now = now();
        let content_storage = match memory_type {
            MemoryType::Codebase => ContentStorage::SourceFile,
            _ => ContentStorage::Filesystem,
        };
        let source = match memory_type {
            MemoryType::Codebase => MemorySource::File,
            MemoryType::Commit | MemoryType::Pr => MemorySource::Git,
            _ => MemorySource::Agent,
        };
        Self {
            id,
            project_id,
            repository_id: None,
            memory_type: memory_type.as_str().to_string(),
            source: Some(source.as_str().to_string()),
            content: None,
            content_hash: None,
            content_storage: Some(content_storage.as_str().to_string()),
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

/// A matched chunk from search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMatch {
    /// Chunk ID
    pub id: String,
    /// Type of node: "function", "class", "heading", etc.
    pub node_type: String,
    /// Name of the node if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    /// Starting line number (1-indexed)
    pub start_line: i32,
    /// Ending line number (1-indexed)
    pub end_line: i32,
    /// Similarity score for this chunk
    pub score: f32,
    /// Short content snippet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Search result with score and decay-adjusted ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub memory: Memory,
    /// Raw semantic similarity score (0.0-1.0)
    #[serde(rename = "relevance")]
    pub score: f32,
    /// Retrieval strength based on recency decay and access frequency (0.0-1.0)
    #[serde(default)]
    pub strength: f32,
    /// Final combined score blending relevance and strength
    #[serde(default)]
    pub combined_score: f32,
    /// Matched chunks that contributed to this result (if chunk search was used)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_chunks: Vec<ChunkMatch>,
}

impl MemorySearchResult {
    /// Create a new search result with only semantic score (for backwards compatibility).
    pub fn new(memory: Memory, score: f32) -> Self {
        Self {
            memory,
            score,
            strength: 0.0,
            combined_score: score,
            matched_chunks: Vec::new(),
        }
    }

    /// Create a search result with decay-adjusted scoring.
    pub fn with_decay(memory: Memory, relevance: f32, strength: f32, combined_score: f32) -> Self {
        Self {
            memory,
            score: relevance,
            strength,
            combined_score,
            matched_chunks: Vec::new(),
        }
    }

    /// Create a search result with matched chunks.
    pub fn with_chunks(
        memory: Memory,
        relevance: f32,
        strength: f32,
        combined_score: f32,
        matched_chunks: Vec<ChunkMatch>,
    ) -> Self {
        Self {
            memory,
            score: relevance,
            strength,
            combined_score,
            matched_chunks,
        }
    }
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
    /// Extracted creation date (earliest date found in file)
    pub created_date: Option<String>,
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
            created_date: None,
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

/// Legacy link between two memories in the knowledge graph.
/// See `links` module for the new simplified MemoryLink.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(FromRow))]
#[serde(rename_all = "snake_case")]
pub struct LegacyMemoryLink {
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

impl LegacyMemoryLink {
    /// Get the typed link type
    pub fn get_link_type(&self) -> Option<LegacyLinkType> {
        LegacyLinkType::from_str(&self.link_type)
    }

    /// Create a new link with generated ID
    pub fn new(
        project_id: String,
        source_id: String,
        target_id: String,
        link_type: LegacyLinkType,
        created_by: String,
    ) -> Self {
        Self {
            id: new_id(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(FromRow))]
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

// Implement MemoryData trait when storage feature is enabled
#[cfg(feature = "storage")]
impl fold_storage::MemoryData for Memory {
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
