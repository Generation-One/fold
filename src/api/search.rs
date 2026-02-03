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

use crate::models::{ChunkMatch, MemorySource};
use crate::{db, AppState, Error, Result};

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

    /// Filter by source (agent, file, git)
    pub source: Option<MemorySource>,

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

    /// Maximum results
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Minimum similarity score (0.0 - 1.0)
    #[serde(default = "default_min_score")]
    pub min_score: f32,

    /// Include matched chunks (function/class/heading level) in results
    #[serde(default)]
    pub include_chunks: bool,
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

    /// Filter by source types
    #[serde(default)]
    pub sources: Vec<MemorySource>,

    /// Maximum total context items
    #[serde(default = "default_context_limit")]
    pub limit: u32,

    /// Whether to include similar memories
    #[serde(default = "default_true")]
    pub include_similar: bool,
}

fn default_context_limit() -> u32 {
    20
}

fn default_true() -> bool {
    true
}

/// Search result item.
#[derive(Debug, Serialize)]
pub struct SearchResultItem {
    pub id: Uuid,
    pub title: Option<String>,
    pub content: String,
    pub snippet: String,
    /// Semantic similarity score (0.0-1.0)
    pub score: f32,
    pub metadata: SearchResultMetadata,
    pub created_at: DateTime<Utc>,
    /// Matched chunks within this memory (when include_chunks=true)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_chunks: Vec<ChunkMatch>,
}

#[derive(Debug, Serialize)]
pub struct SearchResultMetadata {
    /// Memory source
    pub source: Option<MemorySource>,
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
    pub source: Option<MemorySource>,
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
/// Searches memories using Qdrant vector similarity for semantic matching.
#[axum::debug_handler]
async fn search(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>> {
    let start = std::time::Instant::now();

    // Resolve project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Validate query
    if request.query.trim().is_empty() {
        return Err(Error::Validation("Query cannot be empty".into()));
    }

    // Use MemoryService for search - with or without chunks
    let search_results = if request.include_chunks {
        state
            .memory
            .search_with_chunks(&project.id, &project.slug, &request.query, None, request.limit as usize * 2)
            .await?
    } else {
        state
            .memory
            .search(&project.id, &project.slug, &request.query, request.limit as usize * 2)
            .await?
    };

    // If no results, return empty
    if search_results.is_empty() {
        let took_ms = start.elapsed().as_millis() as u64;
        return Ok(Json(SearchResponse {
            query: request.query,
            total: 0,
            results: vec![],
            took_ms,
        }));
    }

    // Build results with filtering
    let mut results: Vec<SearchResultItem> = search_results
        .into_iter()
        .filter_map(|result| {
            let memory = result.memory;

            // Apply source filter
            if let Some(source_filter) = &request.source {
                let memory_source = memory.source.as_deref().and_then(MemorySource::from_str);
                if memory_source != Some(*source_filter) {
                    return None;
                }
            }

            // Apply score filter
            if result.score < request.min_score {
                return None;
            }

            let source = memory.source.as_deref().and_then(MemorySource::from_str);

            Some(SearchResultItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                title: memory.title.clone(),
                content: memory.content.clone().unwrap_or_default(),
                snippet: create_snippet(memory.content.as_deref().unwrap_or(""), 200),
                score: result.score,
                metadata: SearchResultMetadata {
                    source,
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                    language: memory.language.clone(),
                },
                created_at: memory.created_at,
                matched_chunks: result.matched_chunks,
            })
        })
        .collect();

    // Sort by score descending
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
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
/// Returns curated context items relevant to the given task.
#[axum::debug_handler]
async fn get_context(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ContextRequest>,
) -> Result<Json<ContextResponse>> {
    // Resolve project
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Validate task
    if request.task.trim().is_empty() {
        return Err(Error::Validation("Task cannot be empty".into()));
    }

    // Search for relevant memories
    let search_results = state
        .memory
        .search(&project.id, &project.slug, &request.task, request.limit as usize * 2)
        .await?;

    // Build context items with source filtering
    let mut context: Vec<ContextItem> = search_results
        .into_iter()
        .filter_map(|result| {
            let memory = result.memory;

            // Apply source filter if specified
            if !request.sources.is_empty() {
                let memory_source = memory.source.as_deref().and_then(MemorySource::from_str);
                if let Some(source) = memory_source {
                    if !request.sources.contains(&source) {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            let source = memory.source.as_deref().and_then(MemorySource::from_str);

            Some(ContextItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                source,
                title: memory.title.clone(),
                content: memory.content.clone().unwrap_or_default(),
                relevance: result.score,
                metadata: ContextItemMetadata {
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                },
            })
        })
        .collect();

    // Sort by relevance and truncate
    context.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    context.truncate(request.limit as usize);

    // Generate summary using LLM (only if we have context and LLM is available)
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

    // Suggest based on context found
    let has_file = context.iter().any(|c| c.source == Some(MemorySource::File));
    let has_agent = context.iter().any(|c| c.source == Some(MemorySource::Agent));

    if !has_file && (task.contains("code") || task.contains("implement")) {
        suggestions.push("Search for related code files in the repository".into());
    }

    if !has_agent && (task.contains("decision") || task.contains("design")) {
        suggestions.push("Document architectural decisions for this feature".into());
    }

    if context.is_empty() {
        suggestions.push("No relevant context found. Consider adding memories about this topic.".into());
    }

    suggestions
}

/// Create a text snippet from content.
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
