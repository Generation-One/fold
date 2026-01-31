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
/// Searches across memories, code, commits, and pull requests.
/// Currently uses text-based search; Qdrant vector search can be added later.
#[axum::debug_handler]
async fn search(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>> {
    let project_id = &path.project_id;
    let start = std::time::Instant::now();

    // Validate query
    if request.query.trim().is_empty() {
        return Err(Error::Validation("Query cannot be empty".into()));
    }

    let mut results = Vec::new();

    // Search memories using database text search
    if request.include_memories {
        // Convert API memory types to DB memory types
        let db_types: Option<Vec<db::MemoryType>> = if request.types.is_empty() {
            None
        } else {
            Some(
                request
                    .types
                    .iter()
                    .filter_map(|t| api_to_db_memory_type(*t))
                    .collect(),
            )
        };

        let filter = db::MemoryFilter {
            project_id: Some(project_id.clone()),
            memory_types: db_types,
            author: request.author.clone(),
            tag: request.tags.first().cloned(),
            search_query: Some(request.query.clone()),
            limit: Some(request.limit as i64),
            ..Default::default()
        };

        let memories = db::list_memories(&state.db, filter).await?;

        for memory in memories {
            let memory_type = db::MemoryType::from_str(&memory.memory_type);

            results.push(SearchResultItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                result_type: SearchResultType::Memory,
                title: memory.title.clone(),
                content: memory.content.clone(),
                snippet: create_snippet(&memory.content, 200),
                score: 1.0, // Text search doesn't have scores; Qdrant will provide real scores
                metadata: SearchResultMetadata {
                    memory_type: memory_type.and_then(|t| db_to_api_memory_type(t)),
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                    language: memory.language.clone(),
                    repository: memory.repository_id.clone(),
                },
                created_at: parse_datetime(&memory.created_at),
            });
        }
    }

    // Search code (codebase type memories)
    if request.include_code {
        let filter = db::MemoryFilter {
            project_id: Some(project_id.clone()),
            memory_type: Some(db::MemoryType::Codebase),
            file_path_prefix: request.file_pattern.clone(),
            search_query: Some(request.query.clone()),
            limit: Some(request.limit as i64),
            ..Default::default()
        };

        let code_memories = db::list_memories(&state.db, filter).await?;

        for memory in code_memories {
            // Skip if already in results
            if results.iter().any(|r| r.id.to_string() == memory.id) {
                continue;
            }

            results.push(SearchResultItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                result_type: SearchResultType::Code,
                title: memory.title.clone(),
                content: memory.content.clone(),
                snippet: create_snippet(&memory.content, 200),
                score: 1.0,
                metadata: SearchResultMetadata {
                    memory_type: Some(MemoryType::Codebase),
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                    language: memory.language.clone(),
                    repository: memory.repository_id.clone(),
                },
                created_at: parse_datetime(&memory.created_at),
            });
        }
    }

    // Sort by score (once we have Qdrant, this will be meaningful)
    results.sort_by(|a: &SearchResultItem, b: &SearchResultItem| {
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
/// Returns curated context items relevant to the given task,
/// including code, specs, decisions, and recent session notes.
/// Optionally generates a summary of the context.
#[axum::debug_handler]
async fn get_context(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ContextRequest>,
) -> Result<Json<ContextResponse>> {
    let project_id = &path.project_id;

    // Validate task
    if request.task.trim().is_empty() {
        return Err(Error::Validation("Task cannot be empty".into()));
    }

    let mut context = Vec::new();
    let per_type_limit = (request.limit / 4).max(5) as i64;

    // Search for relevant code
    if request.include_code {
        let filter = db::MemoryFilter {
            project_id: Some(project_id.clone()),
            memory_type: Some(db::MemoryType::Codebase),
            search_query: Some(request.task.clone()),
            limit: Some(per_type_limit),
            ..Default::default()
        };

        let code_memories = db::list_memories(&state.db, filter).await?;

        for memory in code_memories {
            context.push(ContextItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                context_type: ContextType::Code,
                title: memory.title.clone(),
                content: memory.content.clone(),
                relevance: 1.0, // Placeholder until Qdrant provides real scores
                metadata: ContextItemMetadata {
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                    related_to: None,
                },
            });
        }
    }

    // Search for relevant memories by type
    let types_to_search: Vec<ContextType> = if request.types.is_empty() {
        // Default to all non-code types
        vec![
            ContextType::Spec,
            ContextType::Decision,
            ContextType::Session,
            ContextType::Task,
            ContextType::General,
        ]
    } else {
        request.types.clone()
    };

    for context_type in &types_to_search {
        let db_memory_type = match context_type {
            ContextType::Code => continue, // Already handled above
            ContextType::Spec => db::MemoryType::Spec,
            ContextType::Decision => db::MemoryType::Decision,
            ContextType::Session => db::MemoryType::Session,
            ContextType::Task => db::MemoryType::Task,
            ContextType::General => db::MemoryType::General,
        };

        let filter = db::MemoryFilter {
            project_id: Some(project_id.clone()),
            memory_type: Some(db_memory_type),
            search_query: Some(request.task.clone()),
            limit: Some(per_type_limit),
            ..Default::default()
        };

        let memories = db::list_memories(&state.db, filter).await?;

        for memory in memories {
            context.push(ContextItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                context_type: *context_type,
                title: memory.title.clone(),
                content: memory.content.clone(),
                relevance: 1.0,
                metadata: ContextItemMetadata {
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                    related_to: None,
                },
            });
        }
    }

    // Include recent session context
    if request.include_sessions {
        let session_memories = db::list_project_memories_by_type(
            &state.db,
            project_id,
            db::MemoryType::Session,
            per_type_limit,
            0,
        )
        .await?;

        for memory in session_memories {
            // Skip if already in context
            if context
                .iter()
                .any(|c| c.id.to_string() == memory.id)
            {
                continue;
            }

            context.push(ContextItem {
                id: Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4()),
                context_type: ContextType::Session,
                title: memory.title.clone(),
                content: memory.content.clone(),
                relevance: 0.8, // Slightly lower relevance for recent sessions
                metadata: ContextItemMetadata {
                    file_path: memory.file_path.clone(),
                    author: memory.author.clone(),
                    tags: memory.tags_vec(),
                    related_to: None,
                },
            });
        }
    }

    // Sort by relevance and truncate
    context.sort_by(|a: &ContextItem, b: &ContextItem| {
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

/// Convert API MemoryType to DB MemoryType.
fn api_to_db_memory_type(api_type: MemoryType) -> Option<db::MemoryType> {
    match api_type {
        MemoryType::Codebase => Some(db::MemoryType::Codebase),
        MemoryType::Session => Some(db::MemoryType::Session),
        MemoryType::Spec => Some(db::MemoryType::Spec),
        MemoryType::Decision => Some(db::MemoryType::Decision),
        MemoryType::Task => Some(db::MemoryType::Task),
        MemoryType::General => Some(db::MemoryType::General),
    }
}

/// Convert DB MemoryType to API MemoryType.
fn db_to_api_memory_type(db_type: db::MemoryType) -> Option<MemoryType> {
    match db_type {
        db::MemoryType::Codebase => Some(MemoryType::Codebase),
        db::MemoryType::Session => Some(MemoryType::Session),
        db::MemoryType::Spec => Some(MemoryType::Spec),
        db::MemoryType::Decision => Some(MemoryType::Decision),
        db::MemoryType::Task => Some(MemoryType::Task),
        db::MemoryType::General => Some(MemoryType::General),
        db::MemoryType::Commit => None, // No direct API equivalent
        db::MemoryType::Pr => None,     // No direct API equivalent
    }
}

/// Parse a datetime string to DateTime<Utc>.
fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .unwrap_or_else(|_| Utc::now())
}
