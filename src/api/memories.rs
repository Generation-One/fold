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
use crate::db;
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

impl MemoryType {
    /// Convert API MemoryType to DB MemoryType.
    fn to_db(&self) -> db::MemoryType {
        match self {
            Self::Codebase => db::MemoryType::Codebase,
            Self::Session => db::MemoryType::Session,
            Self::Spec => db::MemoryType::Spec,
            Self::Decision => db::MemoryType::Decision,
            Self::Task => db::MemoryType::Task,
            Self::General => db::MemoryType::General,
        }
    }

    /// Convert DB MemoryType to API MemoryType.
    fn from_db(db_type: &db::MemoryType) -> Self {
        match db_type {
            db::MemoryType::Codebase => Self::Codebase,
            db::MemoryType::Session => Self::Session,
            db::MemoryType::Spec => Self::Spec,
            db::MemoryType::Decision => Self::Decision,
            db::MemoryType::Task => Self::Task,
            db::MemoryType::General => Self::General,
            // Map commit and pr to general for now (they don't exist in API enum)
            db::MemoryType::Commit | db::MemoryType::Pr => Self::General,
        }
    }

    /// Convert from string (db type column value).
    fn from_db_str(s: &str) -> Self {
        match db::MemoryType::from_str(s) {
            Some(db_type) => Self::from_db(&db_type),
            None => Self::General,
        }
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
// Conversions
// ============================================================================

/// Convert a db::Memory to MemoryResponse.
fn memory_to_response(memory: db::Memory) -> MemoryResponse {
    // Parse timestamps
    let created_at = chrono::DateTime::parse_from_rfc3339(&memory.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let updated_at = chrono::DateTime::parse_from_rfc3339(&memory.updated_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    // Parse UUID from string
    let id = Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4());
    let project_id = Uuid::parse_str(&memory.project_id).unwrap_or_else(|_| Uuid::new_v4());

    // Parse memory type and tags before moving fields
    let memory_type = MemoryType::from_db_str(&memory.memory_type);
    let tags = memory.tags_vec();

    MemoryResponse {
        id,
        project_id,
        title: memory.title,
        content: memory.content,
        memory_type,
        author: memory.author,
        tags,
        file_path: memory.file_path,
        related_ids: vec![], // Related IDs not stored in db currently
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        attachment_count: 0, // Would need separate query
        created_at,
        updated_at,
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// List memories in a project.
///
/// GET /projects/:project_id/memories
#[axum::debug_handler]
async fn list_memories(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Query(query): Query<ListMemoriesQuery>,
) -> Result<Json<ListMemoriesResponse>> {
    // Resolve project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    let limit = query.limit.min(100) as i64;
    let offset = query.offset as i64;

    // Build filter
    let filter = db::MemoryFilter {
        project_id: Some(project.id.clone()),
        memory_type: query.memory_type.map(|t| t.to_db()),
        author: query.author.clone(),
        tag: query.tags.as_ref().and_then(|t| t.split(',').next().map(|s| s.trim().to_string())),
        search_query: query.q.clone(),
        limit: Some(limit),
        offset: Some(offset),
        ..Default::default()
    };

    // Fetch memories
    let memories = db::list_memories(&state.db, filter).await?;

    // Get total count for pagination
    let total = db::count_project_memories(&state.db, &project.id).await? as u32;

    // Convert to response
    let memory_responses: Vec<MemoryResponse> = memories
        .into_iter()
        .map(memory_to_response)
        .collect();

    Ok(Json(ListMemoriesResponse {
        memories: memory_responses,
        total,
        offset: query.offset,
        limit: limit as u32,
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
    // Resolve project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

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

    // Create memory in database
    let memory_id = Uuid::new_v4().to_string();
    let input = db::CreateMemory {
        id: memory_id,
        project_id: project.id,
        repository_id: None,
        memory_type: request.memory_type.to_db(),
        title: request.title,
        content: request.content,
        content_hash: None,
        file_path: request.file_path,
        language: None,
        git_branch: None,
        git_commit_sha: None,
        author: request.author,
        keywords: None,
        tags: if request.tags.is_empty() { None } else { Some(request.tags) },
    };

    let memory = db::create_memory(&state.db, input).await?;

    // TODO: Store embedding in Qdrant

    Ok(Json(memory_to_response(memory)))
}

/// Get a memory by ID.
///
/// GET /projects/:project_id/memories/:memory_id
#[axum::debug_handler]
async fn get_memory(
    State(state): State<AppState>,
    Path(path): Path<MemoryPath>,
) -> Result<Json<MemoryResponse>> {
    // Resolve project (to validate it exists and user has access)
    let _project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Fetch memory from database
    let memory = db::get_memory(&state.db, &path.memory_id.to_string()).await?;

    Ok(Json(memory_to_response(memory)))
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
    // Resolve project (to validate it exists and user has access)
    let _project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // If content changed, regenerate embedding
    if let Some(ref content) = request.content {
        let _embedding = state
            .embeddings
            .embed_single(content)
            .await
            .map_err(|e| Error::Embedding(e.to_string()))?;
        // TODO: Update embedding in Qdrant
    }

    // Build update input
    let update = db::UpdateMemory {
        title: request.title,
        content: request.content,
        content_hash: None,
        git_branch: None,
        git_commit_sha: None,
        author: request.author,
        keywords: None,
        tags: request.tags,
    };

    // Save to database
    let memory = db::update_memory(&state.db, &path.memory_id.to_string(), update).await?;

    Ok(Json(memory_to_response(memory)))
}

/// Delete a memory.
///
/// DELETE /projects/:project_id/memories/:memory_id
#[axum::debug_handler]
async fn delete_memory(
    State(state): State<AppState>,
    Path(path): Path<MemoryPath>,
) -> Result<Json<serde_json::Value>> {
    // Resolve project (to validate it exists and user has access)
    let _project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Delete memory from database
    db::delete_memory(&state.db, &path.memory_id.to_string()).await?;

    // TODO: Delete embedding from Qdrant
    // TODO: Delete associated attachments

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": path.memory_id
    })))
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
    // Resolve project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Validate query
    if request.query.trim().is_empty() {
        return Err(Error::Validation("Query cannot be empty".into()));
    }

    // Generate embedding for query (for future Qdrant search)
    let _query_embedding = state
        .embeddings
        .embed_single(&request.query)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    // For now, use database text search (Qdrant vector search can be added later)
    // Build filter with search query
    let filter = db::MemoryFilter {
        project_id: Some(project.id),
        memory_types: if request.types.is_empty() {
            None
        } else {
            Some(request.types.iter().map(|t| t.to_db()).collect())
        },
        author: request.author.clone(),
        tag: request.tags.first().cloned(),
        search_query: Some(request.query.clone()),
        limit: Some(request.limit as i64),
        offset: Some(0),
        ..Default::default()
    };

    let memories = db::list_memories(&state.db, filter).await?;

    // Convert to search results with placeholder score
    // (real scores will come from Qdrant when implemented)
    let results: Vec<SearchResult> = memories
        .into_iter()
        .map(|m| SearchResult {
            memory: memory_to_response(m),
            score: 1.0, // Placeholder score for text search
        })
        .collect();

    Ok(Json(SearchMemoriesResponse {
        results,
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
    // Resolve project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

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

    for (index, memory_req) in request.memories.iter().enumerate() {
        // Validate
        if memory_req.content.trim().is_empty() {
            errors.push(BulkCreateError {
                index: index as u32,
                error: "Content cannot be empty".into(),
            });
            continue;
        }

        // Generate embedding
        match state.embeddings.embed_single(&memory_req.content).await {
            Ok(_embedding) => {
                // Create memory in database
                let memory_id = Uuid::new_v4().to_string();
                let input = db::CreateMemory {
                    id: memory_id,
                    project_id: project.id.clone(),
                    repository_id: None,
                    memory_type: memory_req.memory_type.to_db(),
                    title: memory_req.title.clone(),
                    content: memory_req.content.clone(),
                    content_hash: None,
                    file_path: memory_req.file_path.clone(),
                    language: None,
                    git_branch: None,
                    git_commit_sha: None,
                    author: memory_req.author.clone(),
                    keywords: None,
                    tags: if memory_req.tags.is_empty() { None } else { Some(memory_req.tags.clone()) },
                };

                match db::create_memory(&state.db, input).await {
                    Ok(_) => {
                        created += 1;
                        // TODO: Store embedding in Qdrant
                    }
                    Err(e) => {
                        errors.push(BulkCreateError {
                            index: index as u32,
                            error: format!("Database error: {}", e),
                        });
                    }
                }
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
