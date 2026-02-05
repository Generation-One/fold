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

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    middleware,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::db;
use crate::middleware::{require_project_read, require_project_write, AuthContext};
use crate::models::{MemoryCreate, MemorySource, MemoryType, MemoryUpdate};
use crate::{AppState, Error, Result};

/// Build memory routes (project-scoped).
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Read operations (list, search, context, source file download)
        .route("/search", post(search_memories))
        .route("/context/:memory_id", get(get_context))
        .route("/:memory_id/source", get(download_source_file))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_project_read,
        ))
        // Write operations (create, update, delete)
        .route("/", get(list_memories).post(create_memory))
        .route(
            "/:memory_id",
            get(get_memory).put(update_memory).delete(delete_memory),
        )
        .layer(middleware::from_fn_with_state(state, require_project_write))
}

/// Build global memory routes (cross-project).
///
/// Routes:
/// - GET /memories - List memories across all accessible projects
pub fn global_routes() -> Router<AppState> {
    Router::new().route("/", get(list_all_memories))
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
    /// Filter by created_at >= this date (ISO 8601 format)
    pub created_after: Option<String>,
    /// Filter by created_at <= this date (ISO 8601 format)
    pub created_before: Option<String>,
    /// Filter by updated_at >= this date (ISO 8601 format)
    pub updated_after: Option<String>,
    /// Filter by updated_at <= this date (ISO 8601 format)
    pub updated_before: Option<String>,
    /// Pagination offset
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Sort field (created_at, updated_at, title)
    #[serde(default)]
    pub sort_by: MemorySortField,
    /// Sort direction (asc, desc)
    #[serde(default)]
    pub sort_dir: SortDirection,
}

fn default_limit() -> u32 {
    20
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MemorySortField {
    CreatedAt,
    #[default]
    UpdatedAt,
    Title,
}

impl MemorySortField {
    fn to_db(&self) -> db::MemorySortField {
        match self {
            Self::CreatedAt => db::MemorySortField::CreatedAt,
            Self::UpdatedAt => db::MemorySortField::UpdatedAt,
            Self::Title => db::MemorySortField::Title,
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

impl SortDirection {
    fn to_db(&self) -> db::SortDirection {
        match self {
            Self::Asc => db::SortDirection::Asc,
            Self::Desc => db::SortDirection::Desc,
        }
    }
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
    /// Custom slug (optional - auto-generated from title if not provided)
    pub slug: Option<String>,
    /// Additional metadata
    #[serde(default)]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    /// Filter by created_at >= this date (ISO 8601 format)
    pub created_after: Option<String>,
    /// Filter by created_at <= this date (ISO 8601 format)
    pub created_before: Option<String>,
    /// Filter by updated_at >= this date (ISO 8601 format)
    pub updated_after: Option<String>,
    /// Filter by updated_at <= this date (ISO 8601 format)
    pub updated_before: Option<String>,
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
    0.4
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
    let source = memory
        .source
        .as_deref()
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
    let source = memory
        .source
        .as_deref()
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

    // Parse tags filter (comma-separated, AND logic)
    let required_tags: Vec<String> = query
        .tags
        .as_ref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();

    // Build filter (without tag - we'll filter in-memory for AND logic)
    let filter = db::MemoryFilter {
        project_id: Some(project.id.clone()),
        source: query.source,
        author: query.author.clone(),
        tag: None, // Filter tags in-memory for AND logic
        search_query: query.q.clone(),
        created_after: query.created_after.clone(),
        created_before: query.created_before.clone(),
        updated_after: query.updated_after.clone(),
        updated_before: query.updated_before.clone(),
        sort_by: Some(query.sort_by.to_db()),
        sort_dir: Some(query.sort_dir.to_db()),
        limit: Some(limit * 2), // Fetch more to account for tag filtering
        offset: Some(offset),
        ..Default::default()
    };

    // Fetch memories
    let mut memories = db::list_memories(&state.db, filter).await?;

    // Resolve content from external storage (fold/)
    let project_root = std::path::PathBuf::from(&project.root_path);

    for memory in &mut memories {
        if memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty()) {
            // Use fold storage to read memory content
            if let Ok((_, content)) = state
                .fold_storage
                .read_memory(&project_root, &memory.id)
                .await
            {
                memory.content = Some(content);
            }
        }
    }

    // Filter by tags (AND - must have ALL specified tags)
    if !required_tags.is_empty() {
        memories.retain(|m| {
            let memory_tags = m.tags_vec();
            required_tags.iter().all(|required_tag| {
                memory_tags.iter().any(|t| t.eq_ignore_ascii_case(required_tag))
            })
        });
    }

    // Apply limit after filtering
    memories.truncate(limit as usize);

    // Get total count for pagination
    let total = db::count_project_memories(&state.db, &project.id).await? as u32;

    // Convert to response
    let memory_responses: Vec<MemoryResponse> =
        memories.into_iter().map(memory_to_response).collect();

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
    let db_project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Validate content
    if request.content.trim().is_empty() {
        return Err(Error::Validation("Content cannot be empty".into()));
    }

    // Create memory via service (handles DB + Qdrant + fold/ storage)
    // This mirrors the MCP memory_add behaviour - source is "agent" and content
    // is written to the fold/ directory as a markdown file
    let memory = state
        .memory
        .add(
            &db_project.id,
            &db_project.slug,
            MemoryCreate {
                memory_type: MemoryType::General,
                content: request.content,
                author: request.author,
                title: request.title,
                tags: request.tags,
                file_path: request.file_path,
                slug: request.slug,
                source: Some(MemorySource::Agent),
                ..Default::default()
            },
            true, // auto-generate metadata via LLM
        )
        .await?;

    Ok(Json(memory_to_response_from_model(memory)))
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
///
/// For agent memories, this updates both SQLite metadata and the fold/ file.
/// For file/git memories, this updates SQLite only.
#[axum::debug_handler]
async fn update_memory(
    State(state): State<AppState>,
    Path(path): Path<MemoryPath>,
    Json(request): Json<UpdateMemoryRequest>,
) -> Result<Json<MemoryResponse>> {
    // Resolve project (to validate it exists and user has access)
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Build update struct
    let update = MemoryUpdate {
        title: request.title,
        content: request.content,
        keywords: None,
        tags: request.tags,
        context: None,
        status: None,
        assignee: None,
        metadata: None,
    };

    // Use memory service which handles both SQLite and fold/ storage
    let memory = state
        .memory
        .update(&project.id, &project.slug, &path.memory_id.to_string(), update)
        .await?;

    Ok(Json(memory_to_response_from_model(memory)))
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
    if let Err(e) = state
        .qdrant
        .delete(&project.slug, &path.memory_id.to_string())
        .await
    {
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
    // Fetch more to account for post-filtering
    let search_results = state
        .memory
        .search(
            &project.id,
            &project.slug,
            &request.query,
            (request.limit * 3) as usize,
        )
        .await?;

    // Parse date filters
    let created_after = request.created_after.as_ref();
    let created_before = request.created_before.as_ref();
    let updated_after = request.updated_after.as_ref();
    let updated_before = request.updated_before.as_ref();

    // Filter by source, min_score, and date ranges
    let filtered_results: Vec<_> = search_results
        .into_iter()
        .filter(|r| {
            // Check source filter
            if let Some(source) = request.source {
                let matches_source = r.memory
                    .source
                    .as_deref()
                    .and_then(MemorySource::from_str)
                    .map(|s| s == source)
                    .unwrap_or(false);
                if !matches_source {
                    return false;
                }
            }
            // Check min_score
            if r.score < request.min_score {
                return false;
            }
            // Check date filters
            let created_at = r.memory.created_at.to_rfc3339();
            let updated_at = r.memory.updated_at.to_rfc3339();
            if let Some(after) = created_after {
                if created_at.as_str() < after.as_str() {
                    return false;
                }
            }
            if let Some(before) = created_before {
                if created_at.as_str() > before.as_str() {
                    return false;
                }
            }
            if let Some(after) = updated_after {
                if updated_at.as_str() < after.as_str() {
                    return false;
                }
            }
            if let Some(before) = updated_before {
                if updated_at.as_str() > before.as_str() {
                    return false;
                }
            }
            true
        })
        .take(request.limit as usize)
        .collect();

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
                content_preview: linked_memory
                    .content
                    .as_deref()
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

    // Collect related memory IDs to exclude from similar
    let related_ids: std::collections::HashSet<_> = related.iter().map(|r| r.id.as_str()).collect();

    // Filter out the memory itself AND any already-related memories from similar results
    let similar: Vec<SimilarMemory> = similar_results
        .into_iter()
        .filter(|r| r.memory.id != memory_id && !related_ids.contains(r.memory.id.as_str()))
        .map(|r| SimilarMemory {
            id: r.memory.id,
            title: r.memory.title,
            content_preview: r
                .memory
                .content
                .as_deref()
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
    /// Filter by created_at >= this date (ISO 8601 format)
    pub created_after: Option<String>,
    /// Filter by created_at <= this date (ISO 8601 format)
    pub created_before: Option<String>,
    /// Filter by updated_at >= this date (ISO 8601 format)
    pub updated_after: Option<String>,
    /// Filter by updated_at <= this date (ISO 8601 format)
    pub updated_before: Option<String>,
    /// Sort field (created_at, updated_at, title)
    #[serde(default)]
    pub sort_by: MemorySortField,
    /// Sort direction (asc, desc)
    #[serde(default)]
    pub sort_dir: SortDirection,
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
        tag: query
            .tags
            .as_ref()
            .and_then(|t| t.split(',').next().map(|s| s.trim().to_string())),
        search_query: query.q.clone(),
        created_after: query.created_after.clone(),
        created_before: query.created_before.clone(),
        updated_after: query.updated_after.clone(),
        updated_before: query.updated_before.clone(),
        sort_by: Some(query.sort_by.to_db()),
        sort_dir: Some(query.sort_dir.to_db()),
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

/// Download the original source file for a memory.
///
/// GET /projects/:project_id/memories/:memory_id/source
///
/// Returns the raw file content with appropriate Content-Type header.
/// Only works for memories that have a file_path set.
#[axum::debug_handler]
async fn download_source_file(
    State(state): State<AppState>,
    Path(path): Path<MemoryPath>,
) -> Result<Response> {
    // Resolve project
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Get the memory
    let memory = db::get_memory(&state.db, &path.memory_id.to_string()).await?;

    // Ensure the memory belongs to this project
    if memory.project_id != project.id {
        return Err(Error::NotFound("Memory not found in this project".into()));
    }

    // Check if memory has a file_path
    let file_path = memory.file_path.ok_or_else(|| {
        Error::NotFound("Memory does not have an associated source file".into())
    })?;

    // Build the full path: project_root + file_path
    let project_root = std::path::PathBuf::from(&project.root_path);
    let full_path = project_root.join(&file_path);

    // Security: ensure the resolved path is within the project root
    let canonical_root = project_root.canonicalize().map_err(|e| {
        Error::Internal(format!("Failed to resolve project root: {}", e))
    })?;
    let canonical_path = full_path.canonicalize().map_err(|_| {
        Error::NotFound(format!("Source file not found: {}", file_path))
    })?;

    if !canonical_path.starts_with(&canonical_root) {
        return Err(Error::Validation("Invalid file path".into()));
    }

    // Read the file
    let content = tokio::fs::read(&canonical_path).await.map_err(|e| {
        Error::NotFound(format!("Failed to read source file: {}", e))
    })?;

    // Determine content type based on file extension
    let content_type = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .to_string();

    // Get the filename for Content-Disposition
    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("source");

    // Build response with appropriate headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from(content))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}
