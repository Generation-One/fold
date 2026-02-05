//! MCP Routes
//!
//! Model Context Protocol (MCP) Streamable HTTP endpoint for AI assistant integration.
//!
//! Routes:
//! - GET /mcp - SSE stream for server-to-client messages
//! - POST /mcp - JSON-RPC 2.0 requests from client

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Json, Router,
};
use futures::stream::StreamExt;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::wrappers::BroadcastStream;

use crate::db;
use crate::middleware::require_token;
use crate::models::{MemoryCreate, MemorySource, MemoryType, MemoryUpdate};
use crate::{AppState, Error, Result};

// ============================================================================
// Session Management
// ============================================================================

/// An active MCP session with a broadcast channel for SSE events.
struct McpSession {
    created_at: Instant,
    #[allow(dead_code)]
    tx: broadcast::Sender<String>,
}

/// Global session storage for MCP connections.
static MCP_SESSIONS: Lazy<RwLock<HashMap<String, McpSession>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Session expiry time (1 hour).
const SESSION_TTL_SECS: u64 = 3600;

/// Cleanup interval (5 minutes).
const CLEANUP_INTERVAL_SECS: u64 = 300;

/// Start background task to clean up expired sessions.
pub fn start_session_cleanup() {
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL_SECS)).await;

            let mut sessions = MCP_SESSIONS.write().await;
            let now = Instant::now();
            let before = sessions.len();

            sessions.retain(|_, session| {
                now.duration_since(session.created_at) < Duration::from_secs(SESSION_TTL_SECS)
            });

            let removed = before - sessions.len();
            if removed > 0 {
                tracing::debug!(
                    removed,
                    remaining = sessions.len(),
                    "Cleaned up MCP sessions"
                );
            }
        }
    });
}

/// Build MCP routes.
///
/// Supports MCP Streamable HTTP transport:
/// - GET /mcp - SSE stream for server-to-client messages
/// - POST /mcp - JSON-RPC 2.0 requests
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(handle_mcp_sse).post(handle_mcp_post))
        .layer(middleware::from_fn_with_state(state, require_token))
}

// ============================================================================
// JSON-RPC Types
// ============================================================================

/// JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data,
            }),
        }
    }
}

// JSON-RPC error codes
#[allow(dead_code)]
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
const INTERNAL_ERROR: i32 = -32603;

// ============================================================================
// MCP Tool Types
// ============================================================================

/// MCP tools/list response.
#[derive(Debug, Serialize)]
struct ToolsListResponse {
    tools: Vec<ToolDefinition>,
}

#[derive(Debug, Serialize)]
struct ToolDefinition {
    name: String,
    description: String,
    input_schema: Value,
}

/// MCP tools/call parameters.
#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// MCP tools/call response.
#[derive(Debug, Serialize)]
struct ToolCallResponse {
    content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
}

// ============================================================================
// Handlers
// ============================================================================

/// Handle SSE stream for server-to-client messages.
///
/// GET /mcp
///
/// Opens an SSE stream for the session. Requires valid Mcp-Session-Id header.
#[axum::debug_handler]
async fn handle_mcp_sse(
    headers: HeaderMap,
) -> std::result::Result<
    Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>>,
    Response,
> {
    // Extract session ID from header
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Missing Mcp-Session-Id header"
                })),
            )
                .into_response()
        })?;

    // Get session's broadcast receiver
    let sessions = MCP_SESSIONS.read().await;
    let session = sessions.get(session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Session not found"
            })),
        )
            .into_response()
    })?;

    let rx = session.tx.subscribe();
    drop(sessions); // Release the read lock

    // Convert broadcast receiver to SSE stream
    let stream = BroadcastStream::new(rx).filter_map(|result| async {
        match result {
            Ok(data) => Some(Ok(Event::default().event("message").data(data))),
            Err(_) => None, // Skip lagged messages
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    ))
}

/// Handle MCP JSON-RPC POST request.
///
/// POST /mcp
///
/// Implements the MCP Streamable HTTP protocol.
/// Creates sessions on initialize, validates sessions on other methods.
#[axum::debug_handler]
async fn handle_mcp_post(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Json(JsonRpcResponse::error(
            request.id,
            INVALID_REQUEST,
            "Invalid JSON-RPC version".into(),
            None,
        ))
        .into_response();
    }

    // Route based on method
    match request.method.as_str() {
        "initialize" => {
            // Create new session
            let session_id = uuid::Uuid::new_v4().to_string();
            let (tx, _) = broadcast::channel(100);

            let session = McpSession {
                created_at: Instant::now(),
                tx,
            };

            MCP_SESSIONS
                .write()
                .await
                .insert(session_id.clone(), session);
            tracing::debug!(session_id = %session_id, "Created MCP session");

            // Return response with session ID header
            let response = handle_initialize(request.id);
            (
                [(
                    axum::http::header::HeaderName::from_static("mcp-session-id"),
                    axum::http::header::HeaderValue::from_str(&session_id).unwrap(),
                )],
                Json(response),
            )
                .into_response()
        }
        "notifications/initialized" | "initialized" => {
            // Acknowledge notification - no response body needed
            StatusCode::ACCEPTED.into_response()
        }
        _ => {
            // Validate session ID for all other methods (optional but recommended)
            let session_id = headers.get("mcp-session-id").and_then(|v| v.to_str().ok());

            if let Some(sid) = session_id {
                if !MCP_SESSIONS.read().await.contains_key(sid) {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({
                            "error": "Session not found or expired"
                        })),
                    )
                        .into_response();
                }
            }

            // Route to existing handlers
            let response = match request.method.as_str() {
                "tools/list" => handle_tools_list(request.id.clone()),
                "tools/call" => handle_tools_call(&state, request.id.clone(), request.params).await,
                "resources/list" => handle_resources_list(request.id.clone()),
                "resources/read" => handle_resources_read(request.id.clone(), request.params),
                _ => JsonRpcResponse::error(
                    request.id.clone(),
                    METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                    None,
                ),
            };

            Json(response).into_response()
        }
    }
}

/// Handle MCP initialize.
fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "fold",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {},
                "resources": {}
            }
        }),
    )
}

/// Handle tools/list method.
fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let tools = vec![
        ToolDefinition {
            name: "project_list".into(),
            description: "List all projects in the memory system".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "github_project_create".into(),
            description: "Create a new project from a GitHub repository".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "repo_url": { "type": "string", "description": "GitHub repository URL (https://github.com/owner/repo)" },
                    "name": { "type": "string", "description": "Project name" },
                    "description": { "type": "string", "description": "Project description" }
                },
                "required": ["repo_url", "name"]
            }),
        },
        ToolDefinition {
            name: "memory_add".into(),
            description: "Add a memory to a project. Agent memories are stored in the fold/ directory and indexed for semantic search. Use this to persist knowledge, decisions, context, or any information that should be recalled later. If a slug is provided, the memory ID is derived from it - using the same slug again will update the existing memory instead of creating a new one.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "content": { "type": "string", "description": "Memory content - the full text to remember" },
                    "title": { "type": "string", "description": "Short descriptive title for the memory" },
                    "author": { "type": "string", "description": "Who created this memory (e.g. 'claude', 'user')" },
                    "slug": { "type": "string", "description": "Optional unique slug. If provided, the memory ID is derived from the slug deterministically, enabling upsert behaviour - same slug always refers to the same memory." },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tags for categorisation"
                    }
                },
                "required": ["project", "content"]
            }),
        },
        ToolDefinition {
            name: "memory_search".into(),
            description: "Search memories using semantic similarity. The query is embedded and compared against stored memory embeddings. Use natural language descriptions of what you're looking for - the more descriptive and context-rich, the better. For example: 'authentication flow for user login' or 'how the API handles error responses'. Avoid keyword-style queries.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "query": { "type": "string", "description": "Natural language search query. Will be embedded for semantic matching - use descriptive phrases, not keywords." },
                    "source": {
                        "type": "string",
                        "enum": ["agent", "file", "git"],
                        "description": "Filter by memory source"
                    },
                    "limit": { "type": "integer", "default": 10, "description": "Max results" },
                    "min_score": { "type": "number", "default": 0.4, "description": "Minimum similarity score (0-1). Default 0.4 filters to relevant matches only." }
                },
                "required": ["project", "query"]
            }),
        },
        ToolDefinition {
            name: "memory_list".into(),
            description: "List memories with optional filters".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Filter by tags (AND - memory must have ALL specified tags)"
                    },
                    "author": { "type": "string", "description": "Filter by author" },
                    "limit": { "type": "integer", "default": 20 }
                },
                "required": ["project"]
            }),
        },
        ToolDefinition {
            name: "memory_context".into(),
            description: "Get context for a memory (related and similar memories)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "memory_id": { "type": "string", "description": "Memory ID to get context for" },
                    "depth": { "type": "integer", "default": 1, "description": "Link traversal depth" }
                },
                "required": ["project", "memory_id"]
            }),
        },
        ToolDefinition {
            name: "memory_update".into(),
            description: "Update an existing memory. Only memories with source 'agent' (stored in fold/) can be updated via MCP. The memory_id is required to identify which memory to update.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "memory_id": { "type": "string", "description": "ID of the memory to update" },
                    "content": { "type": "string", "description": "New content for the memory" },
                    "title": { "type": "string", "description": "New title for the memory" },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "New tags for the memory (replaces existing tags)"
                    }
                },
                "required": ["project", "memory_id"]
            }),
        },
        ToolDefinition {
            name: "memory_delete".into(),
            description: "Delete an agent memory. Only memories with source 'agent' (stored in fold/) can be deleted via MCP. File and git memories are managed by the indexer.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "memory_id": { "type": "string", "description": "ID of the memory to delete" }
                },
                "required": ["project", "memory_id"]
            }),
        },
        ToolDefinition {
            name: "project_stats".into(),
            description: "Get statistics for a project including memory counts, vector counts, and more".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" }
                },
                "required": ["project"]
            }),
        },
    ];

    JsonRpcResponse::success(
        id,
        serde_json::to_value(ToolsListResponse { tools }).unwrap(),
    )
}

/// Handle tools/call method.
async fn handle_tools_call(state: &AppState, id: Option<Value>, params: Value) -> JsonRpcResponse {
    let call_params: ToolCallParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                format!("Invalid params: {}", e),
                None,
            );
        }
    };

    let result = match call_params.name.as_str() {
        "project_list" => execute_project_list(state).await,
        "github_project_create" => execute_github_project_create(state, call_params.arguments).await,
        "project_stats" => execute_project_stats(state, call_params.arguments).await,
        "memory_add" => execute_memory_add(state, call_params.arguments).await,
        "memory_search" => execute_memory_search(state, call_params.arguments).await,
        "memory_list" => execute_memory_list(state, call_params.arguments).await,
        "memory_context" => execute_memory_context(state, call_params.arguments).await,
        "memory_update" => execute_memory_update(state, call_params.arguments).await,
        "memory_delete" => execute_memory_delete(state, call_params.arguments).await,
        _ => {
            return JsonRpcResponse::error(
                id,
                METHOD_NOT_FOUND,
                format!("Tool not found: {}", call_params.name),
                None,
            );
        }
    };

    match result {
        Ok(text) => {
            let response = ToolCallResponse {
                content: vec![ToolContent::Text { text }],
                is_error: None,
            };
            JsonRpcResponse::success(id, serde_json::to_value(response).unwrap())
        }
        Err(e) => {
            let response = ToolCallResponse {
                content: vec![ToolContent::Text {
                    text: format!("Error: {}", e),
                }],
                is_error: Some(true),
            };
            JsonRpcResponse::success(id, serde_json::to_value(response).unwrap())
        }
    }
}

/// Handle resources/list method.
fn handle_resources_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "resources": []
        }),
    )
}

/// Handle resources/read method.
fn handle_resources_read(id: Option<Value>, _params: Value) -> JsonRpcResponse {
    JsonRpcResponse::error(id, METHOD_NOT_FOUND, "Resource not found".into(), None)
}

// ============================================================================
// Tool Implementations
// ============================================================================

async fn execute_project_list(state: &AppState) -> Result<String> {
    let projects = db::list_projects(&state.db).await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "count": projects.len(),
        "projects": projects.iter().map(|p| serde_json::json!({
            "id": p.id,
            "slug": p.slug,
            "name": p.name,
            "description": p.description
        })).collect::<Vec<_>>()
    }))?)
}

async fn execute_project_stats(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Get total memories
    let total_memories = db::count_project_memories(&state.db, &project.id).await? as u64;

    // Get memories by type
    use db::MemoryType;
    let by_type = serde_json::json!({
        "codebase": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Codebase).await.unwrap_or(0),
        "session": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Session).await.unwrap_or(0),
        "decision": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Decision).await.unwrap_or(0),
        "spec": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Spec).await.unwrap_or(0),
        "commit": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Commit).await.unwrap_or(0),
        "pr": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Pr).await.unwrap_or(0),
        "task": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::Task).await.unwrap_or(0),
        "general": db::count_project_memories_by_type(&state.db, &project.id, MemoryType::General).await.unwrap_or(0),
    });

    // Get memories by source
    let by_source = serde_json::json!({
        "file": db::count_project_memories_by_source(&state.db, &project.id, "file").await.unwrap_or(0),
        "agent": db::count_project_memories_by_source(&state.db, &project.id, "agent").await.unwrap_or(0),
        "git": db::count_project_memories_by_source(&state.db, &project.id, "git").await.unwrap_or(0),
    });

    // Get total chunks
    let total_chunks = db::count_chunks_for_project(&state.db, &project.id).await.unwrap_or(0) as u64;

    // Get total links
    let total_links = db::count_project_links(&state.db, &project.id).await.unwrap_or(0) as u64;

    // Get vector count from Qdrant
    let total_vectors = state
        .qdrant
        .collection_info(&project.slug)
        .await
        .map(|info| info.points_count)
        .unwrap_or(0);

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project_id": project.id,
        "project_slug": project.slug,
        "provider": project.provider,
        "root_path": project.root_path,
        "total_memories": total_memories,
        "memories_by_type": by_type,
        "memories_by_source": by_source,
        "total_chunks": total_chunks,
        "total_links": total_links,
        "total_vectors": total_vectors
    }))?)
}

async fn execute_github_project_create(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        repo_url: String,
        name: String,
        description: Option<String>,
        local_path: Option<String>,
        access_token: Option<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Parse owner/repo from URL
    let url = params.repo_url.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() < 2 {
        return Err(Error::Validation("Invalid GitHub URL format".into()));
    }
    let owner = parts[parts.len() - 2].to_string();
    let repo_name = parts[parts.len() - 1].to_string();

    // Create base slug from repo name
    let base_slug = repo_name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
        .trim_matches('-')
        .to_string();

    // Add nonce to slug to prevent collisions
    let nonce = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let slug = format!("{}-{}", base_slug, nonce);

    // Generate project ID
    let project_id = crate::models::new_id();

    // For GitHub projects, we need a local path where the repo will be cloned
    let root_path = params.local_path.unwrap_or_else(|| {
        // Default to a temp directory based on slug
        format!("./repos/{}", slug)
    });

    // Create project with repository info merged in
    let project = db::create_project(
        &state.db,
        db::CreateProject {
            id: project_id.clone(),
            slug,
            name: params.name,
            description: params.description,
            provider: "github".to_string(),
            root_path: root_path.clone(),
            remote_owner: Some(owner.clone()),
            remote_repo: Some(repo_name.clone()),
            remote_branch: Some("main".to_string()),
            access_token: params.access_token,
        },
    )
    .await?;

    // Create Qdrant collection for this project
    state
        .qdrant
        .create_collection(&project.slug, state.embeddings.dimension().await)
        .await?;

    // Queue an index job immediately
    let job_id = crate::models::new_id();
    let job = db::create_job(
        &state.db,
        db::CreateJob::new(job_id, db::JobType::IndexRepo)
            .with_project(project.id.clone()),
    )
    .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "id": project.id,
        "slug": project.slug,
        "name": project.name,
        "description": project.description,
        "provider": "github",
        "root_path": root_path,
        "remote_owner": owner,
        "remote_repo": repo_name,
        "remote_branch": "main",
        "repo_url": params.repo_url,
        "index_job": {
            "id": job.id,
            "status": job.status
        },
        "created_at": project.created_at
    }))?)
}

async fn execute_memory_add(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        content: String,
        title: Option<String>,
        author: Option<String>,
        /// Optional slug - if provided, the memory ID is derived from it deterministically.
        /// This enables upsert behaviour: same slug = same memory ID = update instead of create.
        slug: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Create memory via service (handles DB + Qdrant)
    let memory = state
        .memory
        .add(
            &project.id,
            &project.slug,
            MemoryCreate {
                memory_type: MemoryType::General,
                content: params.content,
                author: params.author,
                title: params.title,
                tags: params.tags,
                slug: params.slug,
                source: Some(MemorySource::Agent),
                ..Default::default()
            },
            true, // auto-generate metadata
        )
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "id": memory.id,
        "title": memory.title,
        "content": memory.content.as_deref().unwrap_or("").chars().take(200).collect::<String>(),
        "author": memory.author,
        "source": memory.source,
        "created_at": memory.created_at.to_rfc3339()
    }))?)
}

async fn execute_memory_search(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        query: String,
        source: Option<String>,
        #[serde(default = "default_limit")]
        limit: usize,
        #[serde(default = "default_min_score")]
        min_score: f32,
    }

    fn default_limit() -> usize {
        10
    }

    fn default_min_score() -> f32 {
        0.4
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Search via memory service (pure similarity)
    let results = state
        .memory
        .search(&project.id, &project.slug, &params.query, params.limit)
        .await?;

    // Filter by source and min_score
    let filtered_results: Vec<_> = results
        .into_iter()
        .filter(|r| {
            // Check source filter
            if let Some(source_str) = &params.source {
                let source = MemorySource::from_str(source_str);
                let matches_source = r.memory
                    .source
                    .as_deref()
                    .and_then(MemorySource::from_str)
                    .map(|s| Some(s) == source)
                    .unwrap_or(false);
                if !matches_source {
                    return false;
                }
            }
            // Check min_score
            r.score >= params.min_score
        })
        .collect();

    let results_json: Vec<_> = filtered_results
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.memory.id,
                "title": r.memory.title,
                "content": r.memory.content.as_deref().unwrap_or("").chars().take(300).collect::<String>(),
                "author": r.memory.author,
                "source": r.memory.source,
                "score": r.score,
                "file_path": r.memory.file_path,
                "created_at": r.memory.created_at.to_rfc3339()
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project": project.slug,
        "query": params.query,
        "count": results_json.len(),
        "results": results_json
    }))?)
}

async fn execute_memory_list(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        #[serde(default)]
        tags: Vec<String>,
        author: Option<String>,
        #[serde(default = "default_list_limit")]
        limit: i64,
    }

    fn default_list_limit() -> i64 {
        20
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // List memories via service (with content resolved from fold/)
    let memories = state
        .memory
        .list_with_content(
            &project.id,
            &project.slug,
            None, // No memory type filter
            params.author.as_deref(),
            params.limit * 2, // Fetch more to account for filtering
            0,
        )
        .await?;

    // Filter by tags (AND - must have ALL specified tags)
    let filtered_memories: Vec<_> = memories
        .into_iter()
        .filter(|m| {
            if !params.tags.is_empty() {
                let memory_tags = m.tags_vec();
                for required_tag in &params.tags {
                    if !memory_tags.iter().any(|t| t.eq_ignore_ascii_case(required_tag)) {
                        return false;
                    }
                }
            }
            true
        })
        .take(params.limit as usize)
        .collect();

    let memories_json: Vec<_> = filtered_memories
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "title": m.title,
                "content": m.content,
                "author": m.author,
                "source": m.source,
                "tags": m.tags_vec(),
                "file_path": m.file_path,
                "created_at": m.created_at.to_rfc3339()
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project": project.slug,
        "count": memories_json.len(),
        "memories": memories_json
    }))?)
}

async fn execute_memory_context(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        memory_id: String,
        #[serde(default = "default_depth")]
        #[allow(dead_code)]
        depth: usize,
    }

    fn default_depth() -> usize {
        1
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Get the memory
    let memory = state
        .memory
        .get(&project.id, &params.memory_id)
        .await?
        .ok_or_else(|| Error::NotFound("Memory not found".into()))?;

    // Get linked memories
    let links = db::get_memory_links(&state.db, &params.memory_id).await?;
    let mut related = Vec::new();

    for link in &links {
        if let Ok(Some(linked_memory)) = state.memory.get(&project.id, &link.target_id).await {
            related.push(serde_json::json!({
                "id": linked_memory.id,
                "title": linked_memory.title,
                "content_preview": linked_memory.content.as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>(),
                "link_type": link.link_type,
                "link_context": link.context
            }));
        }
    }

    // Get similar memories
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
    let related_ids: std::collections::HashSet<String> = links.iter().map(|l| l.target_id.clone()).collect();

    // Filter out the memory itself AND any already-related memories from similar results
    let similar: Vec<_> = similar_results
        .into_iter()
        .filter(|r| r.memory.id != params.memory_id && !related_ids.contains(&r.memory.id))
        .map(|r| {
            serde_json::json!({
                "id": r.memory.id,
                "title": r.memory.title,
                "content_preview": r.memory.content.as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>(),
                "score": r.score
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "memory": {
            "id": memory.id,
            "title": memory.title,
            "content": memory.content,
            "author": memory.author,
            "source": memory.source
        },
        "related": related,
        "similar": similar
    }))?)
}

async fn execute_memory_update(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        memory_id: String,
        content: Option<String>,
        title: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Get the memory to check its source
    let memory = state
        .memory
        .get(&project.id, &params.memory_id)
        .await?
        .ok_or_else(|| Error::NotFound("Memory not found".into()))?;

    // Only allow updating agent memories via MCP
    let source = memory.source.as_deref().unwrap_or("agent");
    if source != "agent" {
        return Err(Error::Validation(format!(
            "Cannot update {} memory via MCP. Only agent memories can be updated. File and git memories are managed by the indexer.",
            source
        )));
    }

    // Build update struct
    let update = MemoryUpdate {
        content: params.content,
        title: params.title,
        tags: params.tags,
        keywords: None,
        context: None,
        status: None,
        assignee: None,
        metadata: None,
    };

    // Update memory via service (handles SQLite + fold/ + Qdrant)
    let updated = state
        .memory
        .update(&project.id, &project.slug, &params.memory_id, update)
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "id": updated.id,
        "title": updated.title,
        "content": updated.content.as_deref().unwrap_or("").chars().take(200).collect::<String>(),
        "author": updated.author,
        "source": updated.source,
        "tags": updated.tags_vec(),
        "updated_at": updated.updated_at.to_rfc3339()
    }))?)
}

async fn execute_memory_delete(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        memory_id: String,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Get the memory to check its source
    let memory = state
        .memory
        .get(&project.id, &params.memory_id)
        .await?
        .ok_or_else(|| Error::NotFound("Memory not found".into()))?;

    // Only allow deleting agent memories via MCP
    let source = memory.source.as_deref().unwrap_or("agent");
    if source != "agent" {
        return Err(Error::Validation(format!(
            "Cannot delete {} memory via MCP. Only agent memories can be deleted. File and git memories are managed by the indexer.",
            source
        )));
    }

    // Delete the memory (handles SQLite, Qdrant, and fold/ file cleanup)
    state
        .memory
        .delete(&project.id, &project.slug, &params.memory_id)
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "deleted": true,
        "memory_id": params.memory_id,
        "project": project.slug
    }))?)
}



