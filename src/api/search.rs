//! Search Routes
//!
//! Unified search and context retrieval endpoints.
//!
//! Routes:
//! - POST /projects/:project_id/search - Unified semantic search
//! - POST /projects/:project_id/context - Get context for a task

use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::memories::MemoryType;
use crate::{AppState, Error, Result};

/// Build search routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/:project_id/search", post(search))
        .route("/:project_id/context", post(get_context))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Unified search request.
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    /// Query text for semantic search
    pub query: String,

    /// Filter by memory types
    #[serde(default)]
    pub types: Vec<MemoryType>,

    /// Filter by tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Filter by author
    pub author: Option<String>,

    /// Filter by file path pattern (glob)
    pub file_pattern: Option<String>,

    /// Filter by date range
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,

    /// Include code from repositories
    #[serde(default = "default_true")]
    pub include_code: bool,

    /// Include memories
    #[serde(default = "default_true")]
    pub include_memories: bool,

    /// Maximum results per category
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Minimum similarity score (0.0 - 1.0)
    #[serde(default = "default_min_score")]
    pub min_score: f32,
}

fn default_true() -> bool {
    true
}

fn default_limit() -> u32 {
    10
}

fn default_min_score() -> f32 {
    0.5
}

/// Context retrieval request.
#[derive(Debug, Deserialize)]
pub struct ContextRequest {
    /// Task or question to get context for
    pub task: String,

    /// Types of context to include
    #[serde(default)]
    pub types: Vec<ContextType>,

    /// Maximum total context items
    #[serde(default = "default_context_limit")]
    pub limit: u32,

    /// Whether to include code snippets
    #[serde(default = "default_true")]
    pub include_code: bool,

    /// Whether to include related memories
    #[serde(default = "default_true")]
    pub include_related: bool,

    /// Whether to include recent session context
    #[serde(default = "default_true")]
    pub include_sessions: bool,
}

fn default_context_limit() -> u32 {
    20
}

/// Types of context to include.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    /// Code snippets and files
    Code,
    /// Specifications
    Spec,
    /// Architectural decisions
    Decision,
    /// Recent session notes
    Session,
    /// Task tracking
    Task,
    /// General memories
    General,
}

/// Search result item.
#[derive(Debug, Serialize)]
pub struct SearchResultItem {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub result_type: SearchResultType,
    pub title: Option<String>,
    pub content: String,
    pub snippet: String,
    pub score: f32,
    pub metadata: SearchResultMetadata,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultType {
    Memory,
    Code,
    Commit,
    PullRequest,
}

#[derive(Debug, Serialize)]
pub struct SearchResultMetadata {
    /// Memory type (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<MemoryType>,
    /// File path (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Author name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Tags
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    /// Language (for code)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Repository (for code/commits)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

/// Search response.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResultItem>,
    pub total: u32,
    pub took_ms: u64,
}

/// Context item.
#[derive(Debug, Serialize)]
pub struct ContextItem {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub context_type: ContextType,
    pub title: Option<String>,
    pub content: String,
    pub relevance: f32,
    pub metadata: ContextItemMetadata,
}

#[derive(Debug, Serialize)]
pub struct ContextItemMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_to: Option<Uuid>,
}

/// Context response.
#[derive(Debug, Serialize)]
pub struct ContextResponse {
    pub task: String,
    pub context: Vec<ContextItem>,
    pub summary: Option<String>,
    pub suggestions: Vec<String>,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// Unified semantic search across all project content.
///
/// POST /projects/:project_id/search
///
/// Searches across memories, code, commits, and pull requests
/// using semantic similarity.
#[axum::debug_handler]
async fn search(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>> {
    let _project_id = &path.project_id;
    let start = std::time::Instant::now();

    // Validate query
    if request.query.trim().is_empty() {
        return Err(Error::Validation("Query cannot be empty".into()));
    }

    // Generate embedding for query
    let query_embedding = state
        .embeddings
        .embed_single(&request.query)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    let mut results = Vec::new();

    // Search memories
    if request.include_memories {
        // TODO: Search Qdrant memories collection
        // TODO: Apply type/tag/author/date filters
    }

    // Search code
    if request.include_code {
        // TODO: Search Qdrant code collection
        // TODO: Apply file pattern filter
    }

    // Sort by score
    results.sort_by(|a: &SearchResultItem, b: &SearchResultItem| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply limit
    results.truncate(request.limit as usize);

    let took_ms = start.elapsed().as_millis() as u64;

    Ok(Json(SearchResponse {
        query: request.query,
        total: results.len() as u32,
        results,
        took_ms,
    }))
}

/// Get relevant context for a task.
///
/// POST /projects/:project_id/context
///
/// Returns curated context items relevant to the given task,
/// including code, specs, decisions, and recent session notes.
/// Optionally generates a summary of the context.
#[axum::debug_handler]
async fn get_context(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ContextRequest>,
) -> Result<Json<ContextResponse>> {
    let _project_id = &path.project_id;

    // Validate task
    if request.task.trim().is_empty() {
        return Err(Error::Validation("Task cannot be empty".into()));
    }

    // Generate embedding for task
    let _task_embedding = state
        .embeddings
        .embed_single(&request.task)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    let mut context = Vec::new();

    // Search for relevant code
    if request.include_code {
        // TODO: Search Qdrant for relevant code
    }

    // Search for relevant memories by type
    for context_type in &request.types {
        let memory_type = match context_type {
            ContextType::Code => continue, // Already handled above
            ContextType::Spec => MemoryType::Spec,
            ContextType::Decision => MemoryType::Decision,
            ContextType::Session => MemoryType::Session,
            ContextType::Task => MemoryType::Task,
            ContextType::General => MemoryType::General,
        };
        // TODO: Search Qdrant with type filter
    }

    // Include recent session context
    if request.include_sessions {
        // TODO: Fetch recent session memories
    }

    // Follow related memory links
    if request.include_related {
        // TODO: Fetch related memories based on graph edges
    }

    // Sort by relevance and truncate
    context.sort_by(|a: &ContextItem, b: &ContextItem| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    context.truncate(request.limit as usize);

    // Generate summary using LLM
    let summary = if !context.is_empty() {
        match generate_context_summary(&state, &request.task, &context).await {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("Failed to generate context summary: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Generate suggestions
    let suggestions = generate_suggestions(&request.task, &context);

    Ok(Json(ContextResponse {
        task: request.task,
        context,
        summary,
        suggestions,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate a summary of the context using LLM.
async fn generate_context_summary(
    state: &AppState,
    task: &str,
    context: &[ContextItem],
) -> Result<String> {
    let context_text: String = context
        .iter()
        .map(|c| {
            format!(
                "### {}\n{}\n",
                c.title.as_deref().unwrap_or("Untitled"),
                c.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Given the following context about a codebase, provide a brief summary \
         relevant to this task: {}\n\nContext:\n{}\n\nSummary:",
        task, context_text
    );

    state
        .llm
        .complete(&prompt, 500)
        .await
        .map_err(|e| Error::Llm(e.to_string()))
}

/// Generate suggestions based on context.
fn generate_suggestions(task: &str, context: &[ContextItem]) -> Vec<String> {
    let mut suggestions = Vec::new();

    // Suggest based on context types found
    let has_spec = context.iter().any(|c| c.context_type == ContextType::Spec);
    let has_decision = context
        .iter()
        .any(|c| c.context_type == ContextType::Decision);
    let has_code = context.iter().any(|c| c.context_type == ContextType::Code);

    if !has_spec {
        suggestions.push("Consider creating a specification for this feature".into());
    }

    if !has_decision && task.contains("architecture") || task.contains("design") {
        suggestions.push("Document architectural decisions made".into());
    }

    if !has_code {
        suggestions.push("Search for related code in the repository".into());
    }

    suggestions
}

/// Create a text snippet from content.
#[allow(dead_code)]
fn create_snippet(content: &str, max_length: usize) -> String {
    if content.len() <= max_length {
        content.to_string()
    } else {
        let truncated = &content[..max_length];
        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &truncated[..last_space])
        } else {
            format!("{}...", truncated)
        }
    }
}
