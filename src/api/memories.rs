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
//! - GET /projects/:project_id/context/:id - Get context for a memory

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    middleware,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::warn;
use uuid::Uuid;

use crate::db;
use crate::middleware::{AuthContext, require_project_read, require_project_write};
use crate::models::MemorySource;
use crate::{AppState, Error, Result};

/// Build memory routes (project-scoped).
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Read operations (list, search, context)
        .route("/search", post(search_memories))
        .route("/context/:memory_id", get(get_context))
        .layer(middleware::from_fn_with_state(state.clone(), require_project_read))
        // Write operations (create, update, delete)
        .route("/", get(list_memories).post(create_memory))
        .route("/:memory_id", get(get_memory).put(update_memory).delete(delete_memory))
        .layer(middleware::from_fn_with_state(state, require_project_write))
}

/// Build global memory routes (cross-project).
///
/// Routes:
/// - GET /memories - List memories across all accessible projects
pub fn global_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_all_memories))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Query parameters for listing memories.
#[derive(Debug, Deserialize, Default)]
pub struct ListMemoriesQuery {
    /// Filter by source (agent, file, git)
    pub source: Option<MemorySource>,
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
    /// Author name
    pub author: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// File path (for file-source memories)
    pub file_path: Option<String>,
    /// Additional metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Request to update a memory.
#[derive(Debug, Deserialize)]
pub struct UpdateMemoryRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub author: Option<String>,
    pub tags: Option<Vec<String>>,
    pub file_path: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Request for semantic search.
#[derive(Debug, Deserialize)]
pub struct SearchMemoriesRequest {
    /// Query text for semantic search
    pub query: String,
    /// Filter by source
    pub source: Option<MemorySource>,
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

/// Query parameters for context retrieval.
#[derive(Debug, Deserialize, Default)]
pub struct ContextQuery {
    /// Depth of link traversal (default 1)
    #[serde(default = "default_depth")]
    pub depth: usize,
}

fn default_depth() -> usize {
    1
}

/// Memory response.
#[derive(Debug, Serialize)]
pub struct MemoryResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: Option<String>,
    pub content: Option<String>,
    pub source: MemorySource,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub file_path: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Links to related memories (only populated on get_memory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<MemoryLink>>,
}

/// A link between memories.
#[derive(Debug, Serialize)]
pub struct MemoryLink {
    pub id: String,
    pub target_id: String,
    pub link_type: String,
    pub context: Option<String>,
}

/// Search result with similarity score.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    #[serde(flatten)]
    pub memory: MemoryResponse,
    /// Semantic similarity score from Qdrant (0.0-1.0)
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

/// Context response for a memory.
#[derive(Debug, Serialize)]
pub struct ContextResponse {
    /// The memory itself
    pub memory: MemoryResponse,
    /// Related memories via links
    pub related: Vec<RelatedMemory>,
    /// Similar memories via vector search
    pub similar: Vec<SimilarMemory>,
}

/// A related memory via explicit link.
#[derive(Debug, Serialize)]
pub struct RelatedMemory {
    pub id: String,
    pub title: Option<String>,
    pub content_preview: String,
    pub link_type: String,
    pub link_context: Option<String>,
}

/// A similar memory via vector search.
#[derive(Debug, Serialize)]
pub struct SimilarMemory {
    pub id: String,
    pub title: Option<String>,
    pub content_preview: String,
    pub score: f32,
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

#[derive(Debug, Deserialize)]
pub struct ContextPath {
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

    // Parse source - default to agent if not specified
    let source = memory.source.as_deref()
        .and_then(MemorySource::from_str)
        .unwrap_or(MemorySource::Agent);
    let tags = memory.tags_vec();

    MemoryResponse {
        id,
        project_id,
        title: memory.title,
        content: memory.content,
        source,
        author: memory.author,
        tags,
        file_path: memory.file_path,
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        created_at,
        updated_at,
        links: None,
    }
}

/// Convert a models::Memory to MemoryResponse.
fn memory_to_response_from_model(memory: crate::models::Memory) -> MemoryResponse {
    // Parse UUID from string
    let id = Uuid::parse_str(&memory.id).unwrap_or_else(|_| Uuid::new_v4());
    let project_id = Uuid::parse_str(&memory.project_id).unwrap_or_else(|_| Uuid::new_v4());

    // Parse source
    let source = memory.source.as_deref()
        .and_then(MemorySource::from_str)
        .unwrap_or(MemorySource::Agent);

    // Call borrowing methods before moving fields
    let tags = memory.tags_vec();
    let metadata = memory.metadata_map();

    MemoryResponse {
        id,
        project_id,
        title: memory.title,
        content: memory.content,
        source,
        author: memory.author,
        tags,
        file_path: memory.file_path,
        metadata: serde_json::Value::Object(metadata.into_iter().collect()),
        created_at: memory.created_at,
        updated_at: memory.updated_at,
        links: None,
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
        source: query.source,
        author: query.author.clone(),
        tag: query.tags.as_ref().and_then(|t| t.split(',').next().map(|s| s.trim().to_string())),
        search_query: query.q.clone(),
        limit: Some(limit),
        offset: Some(offset),
        ..Default::default()
    };

    // Fetch memories
    let mut memories = db::list_memories(&state.db, filter).await?;

    // Resolve content from external storage
    for memory in &mut memories {
        if memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty()) {
            // Create minimal Memory for content resolution (only fields used by resolver)
            let mut models_memory = crate::models::Memory::new_with_id(
                memory.id.clone(),
                memory.project_id.clone(),
                crate::models::MemoryType::from_str(&memory.memory_type).unwrap_or_default(),
            );
            // Copy fields needed for resolution
            models_memory.repository_id = memory.repository_id.clone();
            models_memory.content_storage = memory.content_storage.clone();
            models_memory.file_path = memory.file_path.clone();

            let content = state
                .content_resolver
                .resolve_content(&models_memory, &project.slug, project.root_path.as_deref())
                .await
                .unwrap_or(None);
            memory.content = content;
        }
    }

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

    // Determine source - file if file_path is provided, otherwise agent
    let source = if request.file_path.is_some() {
        MemorySource::File
    } else {
        MemorySource::Agent
    };

    // Create memory in database
    let memory_id = Uuid::new_v4().to_string();
    let input = db::CreateMemory {
        id: memory_id,
        project_id: project.id,
        repository_id: None,
        memory_type: db::MemoryType::General, // Default type, source is more important now
        title: request.title,
        content: Some(request.content),
        content_hash: None,
        content_storage: "filesystem".to_string(),
        file_path: request.file_path,
        language: None,
        git_branch: None,
        git_commit_sha: None,
        author: request.author,
        keywords: None,
        tags: if request.tags.is_empty() { None } else { Some(request.tags) },
        source: Some(source),
    };

    let memory = db::create_memory(&state.db, input).await?;

    // Build metadata payload for Qdrant
    let mut payload = HashMap::new();
    payload.insert("memory_id".to_string(), json!(memory.id));
    payload.insert("project_id".to_string(), json!(memory.project_id));
    payload.insert("source".to_string(), json!(source.as_str()));
    if let Some(ref author) = memory.author {
        payload.insert("author".to_string(), json!(author));
    }
    if let Some(ref file_path) = memory.file_path {
        payload.insert("file_path".to_string(), json!(file_path));
    }

    // Store embedding in Qdrant (non-blocking)
    if let Err(e) = state.qdrant.upsert(
        &project.slug,
        &memory.id,
        _embedding,
        payload,
    ).await {
        warn!(error = %e, memory_id = %memory.id, "Failed to store embedding in Qdrant");
    }

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
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Fetch memory with content resolved from external storage
    let memory = state
        .memory
        .get(&project.id, &path.memory_id.to_string())
        .await?
        .ok_or_else(|| Error::NotFound("Memory not found".into()))?;

    // Get links for this memory
    let db_links = db::get_memory_links(&state.db, &path.memory_id.to_string()).await?;
    let links: Vec<MemoryLink> = db_links
        .into_iter()
        .map(|l| MemoryLink {
            id: l.id,
            target_id: l.target_id,
            link_type: l.link_type,
            context: l.context,
        })
        .collect();

    let mut response = memory_to_response_from_model(memory);
    response.links = Some(links);

    Ok(Json(response))
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
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // If content changed, regenerate embedding
    if let Some(ref content) = request.content {
        let embedding = state
            .embeddings
            .embed_single(content)
            .await
            .map_err(|e| Error::Embedding(e.to_string()))?;

        // Build metadata payload for Qdrant update
        let mut payload = HashMap::new();
        payload.insert("memory_id".to_string(), json!(path.memory_id.to_string()));
        payload.insert("project_id".to_string(), json!(project.id));
        if let Some(ref author) = request.author {
            payload.insert("author".to_string(), json!(author));
        }
        if let Some(ref file_path) = request.file_path {
            payload.insert("file_path".to_string(), json!(file_path));
        }

        // Update embedding in Qdrant (non-blocking)
        if let Err(e) = state.qdrant.upsert(
            &project.slug,
            &path.memory_id.to_string(),
            embedding,
            payload,
        ).await {
            warn!(error = %e, memory_id = %path.memory_id, "Failed to update embedding in Qdrant");
        }
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
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Delete memory from database
    db::delete_memory(&state.db, &path.memory_id.to_string()).await?;

    // Delete embedding from Qdrant (non-blocking cleanup)
    if let Err(e) = state.qdrant.delete(
        &project.slug,
        &path.memory_id.to_string(),
    ).await {
        warn!(error = %e, memory_id = %path.memory_id, "Failed to delete embedding from Qdrant");
    }

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": path.memory_id
    })))
}

/// Semantic search for memories.
///
/// POST /projects/:project_id/memories/search
///
/// Uses Qdrant vector similarity search with query embeddings.
/// Returns results ranked by pure semantic similarity score.
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

    // Use MemoryService for search (pure similarity, no decay)
    let search_results = state
        .memory
        .search(&project.id, &project.slug, &request.query, request.limit as usize)
        .await?;

    // Filter by source if specified
    let filtered_results: Vec<_> = if let Some(source) = request.source {
        search_results
            .into_iter()
            .filter(|r| {
                r.memory.source.as_deref()
                    .and_then(MemorySource::from_str)
                    .map(|s| s == source)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        search_results
    };

    // Convert to API response format
    let results: Vec<SearchResult> = filtered_results
        .into_iter()
        .map(|r| SearchResult {
            memory: memory_to_response_from_model(r.memory),
            score: r.score,
        })
        .collect();

    Ok(Json(SearchMemoriesResponse {
        results,
        query: request.query,
    }))
}

/// Get context for a memory.
///
/// GET /projects/:project_id/context/:memory_id
///
/// Returns the memory along with related memories (via links) and similar memories (via vector search).
#[axum::debug_handler]
async fn get_context(
    State(state): State<AppState>,
    Path(path): Path<ContextPath>,
    Query(_query): Query<ContextQuery>,
) -> Result<Json<ContextResponse>> {
    // Resolve project
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;
    let memory_id = path.memory_id.to_string();

    // Get the memory itself
    let memory = state
        .memory
        .get(&project.id, &memory_id)
        .await?
        .ok_or_else(|| Error::NotFound("Memory not found".into()))?;

    // Get linked memories
    let links = db::get_memory_links(&state.db, &memory_id).await?;
    let mut related = Vec::new();

    for link in links {
        if let Ok(Some(linked_memory)) = state.memory.get(&project.id, &link.target_id).await {
            related.push(RelatedMemory {
                id: linked_memory.id.clone(),
                title: linked_memory.title.clone(),
                content_preview: linked_memory.content.as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect(),
                link_type: link.link_type,
                link_context: link.context,
            });
        }
    }

    // Get similar memories via vector search
    let content = memory.content.as_deref().unwrap_or("");
    let similar_results = if !content.is_empty() {
        state
            .memory
            .search(&project.id, &project.slug, content, 5)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Filter out the memory itself from similar results
    let similar: Vec<SimilarMemory> = similar_results
        .into_iter()
        .filter(|r| r.memory.id != memory_id)
        .map(|r| SimilarMemory {
            id: r.memory.id,
            title: r.memory.title,
            content_preview: r.memory.content.as_deref()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect(),
            score: r.score,
        })
        .collect();

    Ok(Json(ContextResponse {
        memory: memory_to_response_from_model(memory),
        related,
        similar,
    }))
}

// ============================================================================
// Global Handlers (cross-project)
// ============================================================================

/// Query parameters for listing all memories.
#[derive(Debug, Deserialize, Default)]
pub struct ListAllMemoriesQuery {
    /// Filter by project ID or slug
    pub project: Option<String>,
    /// Filter by source (agent, file, git)
    pub source: Option<MemorySource>,
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
}

/// Response for listing all memories.
#[derive(Debug, Serialize)]
pub struct ListAllMemoriesResponse {
    pub memories: Vec<MemoryWithProject>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

/// Memory response with project info.
#[derive(Debug, Serialize)]
pub struct MemoryWithProject {
    #[serde(flatten)]
    pub memory: MemoryResponse,
    pub project_id: String,
    pub project_slug: String,
    pub project_name: String,
}

/// List memories across all accessible projects.
///
/// GET /memories
#[axum::debug_handler]
async fn list_all_memories(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Query(query): Query<ListAllMemoriesQuery>,
) -> Result<Json<ListAllMemoriesResponse>> {
    let limit = query.limit.min(100) as i64;
    let offset = query.offset as i64;

    // Get projects the user has access to (via user/groups/projects model)
    let accessible_projects = db::list_user_projects(&state.db, &auth.user_id).await?;

    // If filtering by specific project, validate access
    let project_ids: Vec<String> = if let Some(ref project_filter) = query.project {
        let project = db::get_project_by_id_or_slug(&state.db, project_filter).await?;
        if !accessible_projects.iter().any(|p| p.id == project.id) {
            return Err(Error::Forbidden);
        }
        vec![project.id]
    } else {
        accessible_projects.iter().map(|p| p.id.clone()).collect()
    };

    if project_ids.is_empty() {
        return Ok(Json(ListAllMemoriesResponse {
            memories: vec![],
            total: 0,
            offset: query.offset,
            limit: limit as u32,
        }));
    }

    // Build filter for memories across projects
    let filter = db::MemoryFilter {
        project_ids: Some(project_ids.clone()),
        source: query.source,
        author: query.author.clone(),
        tag: query.tags.as_ref().and_then(|t| t.split(',').next().map(|s| s.trim().to_string())),
        search_query: query.q.clone(),
        limit: Some(limit),
        offset: Some(offset),
        ..Default::default()
    };

    // Fetch memories
    let memories = db::list_memories(&state.db, filter).await?;

    // Count total across all accessible projects
    let mut total = 0u32;
    for project_id in &project_ids {
        total += db::count_project_memories(&state.db, project_id).await? as u32;
    }

    // Build project lookup map
    let project_map: std::collections::HashMap<String, _> = accessible_projects
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();

    // Convert to response with project info
    let memory_responses: Vec<MemoryWithProject> = memories
        .into_iter()
        .filter_map(|memory| {
            let project = project_map.get(&memory.project_id)?;
            Some(MemoryWithProject {
                project_id: project.id.clone(),
                project_slug: project.slug.clone(),
                project_name: project.name.clone(),
                memory: memory_to_response(memory),
            })
        })
        .collect();

    Ok(Json(ListAllMemoriesResponse {
        memories: memory_responses,
        total,
        offset: query.offset,
        limit: limit as u32,
    }))
}
