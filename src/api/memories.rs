//! Memories Routes
//!
//! CRUD and search operations for memories within a project.
//!
//! Routes:
//! - GET /projects/:project_id/memories - List memories
//! - POST /projects/:project_id/memories - Create a memory
//! - GET /projects/:project_id/memories/:id - Get memory details
//! - PUT /projects/:project_id/memories/:id - Update memory
//! - DELETE /projects/:project_id/memories/:id - Delete memory
//! - POST /projects/:project_id/memories/search - Semantic search
//! - POST /projects/:project_id/memories/bulk - Bulk create memories

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::attachments;
use crate::{AppState, Error, Result};

/// Build memory routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_memories).post(create_memory))
        .route(
            "/:memory_id",
            get(get_memory).put(update_memory).delete(delete_memory),
        )
        .route("/search", post(search_memories))
        .route("/bulk", post(bulk_create_memories))
        // Nested attachments routes
        .nest("/:memory_id/attachments", attachments::routes())
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Memory types supported by the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Code snippets and file contents
    Codebase,
    /// Session summaries and notes
    Session,
    /// Feature specifications
    Spec,
    /// Architectural decisions
    Decision,
    /// Task tracking
    Task,
    /// General memories
    General,
}

impl Default for MemoryType {
    fn default() -> Self {
        Self::General
    }
}

/// Query parameters for listing memories.
#[derive(Debug, Deserialize, Default)]
pub struct ListMemoriesQuery {
    /// Filter by memory type
    #[serde(rename = "type")]
    pub memory_type: Option<MemoryType>,
    /// Filter by author
    pub author: Option<String>,
    /// Filter by tags (comma-separated)
    pub tags: Option<String>,
    /// Search in content (basic text match)
    pub q: Option<String>,
    /// Pagination offset
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Sort field
    #[serde(default)]
    pub sort_by: MemorySortField,
    /// Sort direction
    #[serde(default)]
    pub sort_dir: SortDirection,
}

fn default_limit() -> u32 {
    20
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemorySortField {
    #[default]
    CreatedAt,
    UpdatedAt,
    Title,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

/// Request to create a memory.
#[derive(Debug, Deserialize)]
pub struct CreateMemoryRequest {
    /// Memory title
    pub title: Option<String>,
    /// Main content
    pub content: String,
    /// Memory type
    #[serde(rename = "type", default)]
    pub memory_type: MemoryType,
    /// Author name
    pub author: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// File path (for codebase memories)
    pub file_path: Option<String>,
    /// Related memory IDs
    #[serde(default)]
    pub related_ids: Vec<Uuid>,
    /// Additional metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Request to update a memory.
#[derive(Debug, Deserialize)]
pub struct UpdateMemoryRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<MemoryType>,
    pub author: Option<String>,
    pub tags: Option<Vec<String>>,
    pub file_path: Option<String>,
    pub related_ids: Option<Vec<Uuid>>,
    pub metadata: Option<serde_json::Value>,
}

/// Request for semantic search.
#[derive(Debug, Deserialize)]
pub struct SearchMemoriesRequest {
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
    /// Maximum results to return
    #[serde(default = "default_search_limit")]
    pub limit: u32,
    /// Minimum similarity score (0.0 - 1.0)
    #[serde(default = "default_min_score")]
    pub min_score: f32,
}

fn default_search_limit() -> u32 {
    10
}

fn default_min_score() -> f32 {
    0.5
}

/// Request for bulk memory creation.
#[derive(Debug, Deserialize)]
pub struct BulkCreateMemoriesRequest {
    pub memories: Vec<CreateMemoryRequest>,
}

/// Memory response.
#[derive(Debug, Serialize)]
pub struct MemoryResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: Option<String>,
    pub content: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub file_path: Option<String>,
    pub related_ids: Vec<Uuid>,
    pub metadata: serde_json::Value,
    pub attachment_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Search result with similarity score.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    #[serde(flatten)]
    pub memory: MemoryResponse,
    pub score: f32,
}

/// List memories response.
#[derive(Debug, Serialize)]
pub struct ListMemoriesResponse {
    pub memories: Vec<MemoryResponse>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

/// Search memories response.
#[derive(Debug, Serialize)]
pub struct SearchMemoriesResponse {
    pub results: Vec<SearchResult>,
    pub query: String,
}

/// Bulk create response.
#[derive(Debug, Serialize)]
pub struct BulkCreateResponse {
    pub created: u32,
    pub failed: u32,
    pub errors: Vec<BulkCreateError>,
}

#[derive(Debug, Serialize)]
pub struct BulkCreateError {
    pub index: u32,
    pub error: String,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryPath {
    pub project_id: String,
    pub memory_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// List memories in a project.
///
/// GET /projects/:project_id/memories
#[axum::debug_handler]
async fn list_memories(
    State(_state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Query(query): Query<ListMemoriesQuery>,
) -> Result<Json<ListMemoriesResponse>> {
    let _project_id = &path.project_id;
    let limit = query.limit.min(100);

    // TODO: Fetch memories from database with filters

    Ok(Json(ListMemoriesResponse {
        memories: vec![],
        total: 0,
        offset: query.offset,
        limit,
    }))
}

/// Create a new memory.
///
/// POST /projects/:project_id/memories
#[axum::debug_handler]
async fn create_memory(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<CreateMemoryRequest>,
) -> Result<Json<MemoryResponse>> {
    let _project_id = &path.project_id;

    // Validate content
    if request.content.trim().is_empty() {
        return Err(Error::Validation("Content cannot be empty".into()));
    }

    // Generate embedding for the content
    let _embedding = state
        .embeddings
        .embed_single(&request.content)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    // TODO: Store memory in database
    // TODO: Store embedding in Qdrant

    let now = Utc::now();
    Ok(Json(MemoryResponse {
        id: Uuid::new_v4(),
        project_id: Uuid::new_v4(), // TODO: Parse from path
        title: request.title,
        content: request.content,
        memory_type: request.memory_type,
        author: request.author,
        tags: request.tags,
        file_path: request.file_path,
        related_ids: request.related_ids,
        metadata: request.metadata,
        attachment_count: 0,
        created_at: now,
        updated_at: now,
    }))
}

/// Get a memory by ID.
///
/// GET /projects/:project_id/memories/:memory_id
#[axum::debug_handler]
async fn get_memory(
    State(_state): State<AppState>,
    Path(path): Path<MemoryPath>,
) -> Result<Json<MemoryResponse>> {
    // TODO: Fetch memory from database

    Err(Error::NotFound(format!("Memory: {}", path.memory_id)))
}

/// Update a memory.
///
/// PUT /projects/:project_id/memories/:memory_id
#[axum::debug_handler]
async fn update_memory(
    State(state): State<AppState>,
    Path(path): Path<MemoryPath>,
    Json(request): Json<UpdateMemoryRequest>,
) -> Result<Json<MemoryResponse>> {
    // TODO: Fetch existing memory
    // TODO: Apply updates

    // If content changed, regenerate embedding
    if let Some(ref content) = request.content {
        let _embedding = state
            .embeddings
            .embed_single(content)
            .await
            .map_err(|e| Error::Embedding(e.to_string()))?;
        // TODO: Update embedding in Qdrant
    }

    // TODO: Save to database

    Err(Error::NotFound(format!("Memory: {}", path.memory_id)))
}

/// Delete a memory.
///
/// DELETE /projects/:project_id/memories/:memory_id
#[axum::debug_handler]
async fn delete_memory(
    State(_state): State<AppState>,
    Path(path): Path<MemoryPath>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Delete memory from database
    // TODO: Delete embedding from Qdrant
    // TODO: Delete associated attachments

    Err(Error::NotFound(format!("Memory: {}", path.memory_id)))
}

/// Semantic search for memories.
///
/// POST /projects/:project_id/memories/search
#[axum::debug_handler]
async fn search_memories(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<SearchMemoriesRequest>,
) -> Result<Json<SearchMemoriesResponse>> {
    let _project_id = &path.project_id;

    // Validate query
    if request.query.trim().is_empty() {
        return Err(Error::Validation("Query cannot be empty".into()));
    }

    // Generate embedding for query
    let _query_embedding = state
        .embeddings
        .embed_single(&request.query)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    // TODO: Search Qdrant for similar vectors
    // TODO: Fetch full memory data from database
    // TODO: Apply type/tag/author filters

    Ok(Json(SearchMemoriesResponse {
        results: vec![],
        query: request.query,
    }))
}

/// Bulk create memories.
///
/// POST /projects/:project_id/memories/bulk
///
/// Creates multiple memories in a single request. Useful for importing
/// or batch processing.
#[axum::debug_handler]
async fn bulk_create_memories(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<BulkCreateMemoriesRequest>,
) -> Result<Json<BulkCreateResponse>> {
    let _project_id = &path.project_id;

    if request.memories.is_empty() {
        return Err(Error::Validation("No memories provided".into()));
    }

    if request.memories.len() > 100 {
        return Err(Error::Validation(
            "Maximum 100 memories per bulk request".into(),
        ));
    }

    let mut created = 0u32;
    let mut errors = Vec::new();

    for (index, memory) in request.memories.iter().enumerate() {
        // Validate
        if memory.content.trim().is_empty() {
            errors.push(BulkCreateError {
                index: index as u32,
                error: "Content cannot be empty".into(),
            });
            continue;
        }

        // Generate embedding
        match state.embeddings.embed_single(&memory.content).await {
            Ok(_embedding) => {
                // TODO: Store memory and embedding
                created += 1;
            }
            Err(e) => {
                errors.push(BulkCreateError {
                    index: index as u32,
                    error: format!("Embedding failed: {}", e),
                });
            }
        }
    }

    Ok(Json(BulkCreateResponse {
        created,
        failed: errors.len() as u32,
        errors,
    }))
}
