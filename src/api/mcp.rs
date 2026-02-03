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
use crate::models::{MemoryCreate, MemorySource, MemoryType};
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
                tracing::debug!(removed, remaining = sessions.len(), "Cleaned up MCP sessions");
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
            error: Some(JsonRpcError { code, message, data }),
        }
    }
}

// JSON-RPC error codes
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
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

            MCP_SESSIONS.write().await.insert(session_id.clone(), session);
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
            let session_id = headers
                .get("mcp-session-id")
                .and_then(|v| v.to_str().ok());

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
                "tools/call" => {
                    handle_tools_call(&state, request.id.clone(), request.params).await
                }
                "resources/list" => handle_resources_list(request.id.clone()),
                "resources/read" => handle_resources_read(request.id.clone(), request.params),
                _ => JsonRpcResponse::error(
                    request.id,
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
            name: "project_create".into(),
            description: "Create a new project".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Project name" },
                    "description": { "type": "string", "description": "Project description" },
                    "root_path": { "type": "string", "description": "Local path to codebase" }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "memory_add".into(),
            description: "Add a memory to a project".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "content": { "type": "string", "description": "Memory content" },
                    "title": { "type": "string", "description": "Optional title" },
                    "author": { "type": "string", "description": "Who created this memory" },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tags"
                    }
                },
                "required": ["project", "content"]
            }),
        },
        ToolDefinition {
            name: "memory_search".into(),
            description: "Search memories using semantic similarity".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "query": { "type": "string", "description": "Search query" },
                    "source": {
                        "type": "string",
                        "enum": ["agent", "file", "git"],
                        "description": "Filter by memory source"
                    },
                    "limit": { "type": "integer", "default": 10, "description": "Max results" }
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
                    "source": {
                        "type": "string",
                        "enum": ["agent", "file", "git"],
                        "description": "Filter by memory source"
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
            name: "memory_link_add".into(),
            description: "Create a link between two memories".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "source_id": { "type": "string", "description": "Source memory ID" },
                    "target_id": { "type": "string", "description": "Target memory ID" },
                    "link_type": {
                        "type": "string",
                        "enum": ["references", "implements", "depends_on", "related"],
                        "description": "Type of link"
                    },
                    "context": { "type": "string", "description": "Optional context for the link" }
                },
                "required": ["project", "source_id", "target_id", "link_type"]
            }),
        },
        ToolDefinition {
            name: "memory_link_list".into(),
            description: "List links for a memory".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Memory ID" }
                },
                "required": ["memory_id"]
            }),
        },
        ToolDefinition {
            name: "codebase_index".into(),
            description: "Index a project's codebase".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "path": { "type": "string", "description": "Path to codebase (optional, uses project default)" }
                },
                "required": ["project"]
            }),
        },
        ToolDefinition {
            name: "codebase_search".into(),
            description: "Search indexed codebase (file-source memories)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["project", "query"]
            }),
        },
        ToolDefinition {
            name: "file_upload".into(),
            description: "Upload and index a single file".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "path": { "type": "string", "description": "File path (relative to project root)" },
                    "content": { "type": "string", "description": "File content" },
                    "author": { "type": "string", "description": "Who uploaded this file" }
                },
                "required": ["project", "path", "content"]
            }),
        },
        ToolDefinition {
            name: "files_upload".into(),
            description: "Upload and index multiple files at once".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "files": {
                        "type": "array",
                        "description": "Array of files to upload",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "File path" },
                                "content": { "type": "string", "description": "File content" }
                            },
                            "required": ["path", "content"]
                        }
                    },
                    "author": { "type": "string", "description": "Who uploaded these files" }
                },
                "required": ["project", "files"]
            }),
        },
    ];

    JsonRpcResponse::success(
        id,
        serde_json::to_value(ToolsListResponse { tools }).unwrap(),
    )
}

/// Handle tools/call method.
async fn handle_tools_call(
    state: &AppState,
    id: Option<Value>,
    params: Value,
) -> JsonRpcResponse {
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
        "project_create" => execute_project_create(state, call_params.arguments).await,
        "memory_add" => execute_memory_add(state, call_params.arguments).await,
        "memory_search" => execute_memory_search(state, call_params.arguments).await,
        "memory_list" => execute_memory_list(state, call_params.arguments).await,
        "memory_context" => execute_memory_context(state, call_params.arguments).await,
        "memory_link_add" => execute_memory_link_add(state, call_params.arguments).await,
        "memory_link_list" => execute_memory_link_list(state, call_params.arguments).await,
        "codebase_index" => execute_codebase_index(state, call_params.arguments).await,
        "codebase_search" => execute_codebase_search(state, call_params.arguments).await,
        "file_upload" => execute_file_upload(state, call_params.arguments).await,
        "files_upload" => execute_files_upload(state, call_params.arguments).await,
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
    JsonRpcResponse::error(
        id,
        METHOD_NOT_FOUND,
        "Resource not found".into(),
        None,
    )
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

async fn execute_project_create(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        name: String,
        description: Option<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Generate slug from name
    let slug = params
        .name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
        .trim_matches('-')
        .to_string();

    // Generate ID
    let id = crate::models::new_id();

    let project = db::create_project(
        &state.db,
        db::CreateProject {
            id,
            slug,
            name: params.name,
            description: params.description,
        },
    )
    .await?;

    // Create Qdrant collection for this project
    state
        .qdrant
        .create_collection(&project.slug, state.embeddings.dimension().await)
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "id": project.id,
        "slug": project.slug,
        "name": project.name,
        "description": project.description,
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
    }

    fn default_limit() -> usize {
        10
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Search via memory service (pure similarity)
    let results = state
        .memory
        .search(&project.id, &project.slug, &params.query, params.limit)
        .await?;

    // Filter by source if specified
    let filtered_results: Vec<_> = if let Some(source_str) = &params.source {
        let source = MemorySource::from_str(source_str);
        results
            .into_iter()
            .filter(|r| {
                r.memory.source.as_deref()
                    .and_then(MemorySource::from_str)
                    .map(|s| Some(s) == source)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        results
    };

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
        source: Option<String>,
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

    // Parse source filter
    let source = params.source.as_deref().and_then(MemorySource::from_str);

    // List memories via service
    let memories = state
        .memory
        .list(
            &project.id,
            None, // No memory type filter
            params.author.as_deref(),
            params.limit,
            0,
        )
        .await?;

    // Filter by source if specified
    let filtered_memories: Vec<_> = if let Some(source_filter) = source {
        memories
            .into_iter()
            .filter(|m| {
                m.source.as_deref()
                    .and_then(MemorySource::from_str)
                    .map(|s| s == source_filter)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        memories
    };

    let memories_json: Vec<_> = filtered_memories
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "title": m.title,
                "author": m.author,
                "source": m.source,
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

    let similar: Vec<_> = similar_results
        .into_iter()
        .filter(|r| r.memory.id != params.memory_id)
        .map(|r| serde_json::json!({
            "id": r.memory.id,
            "title": r.memory.title,
            "content_preview": r.memory.content.as_deref()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>(),
            "score": r.score
        }))
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

async fn execute_memory_link_add(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        source_id: String,
        target_id: String,
        link_type: String,
        context: Option<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project (for validation)
    let _project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Create the link
    let link_id = crate::models::new_id();
    let link = db::create_memory_link(
        &state.db,
        db::CreateMemoryLink {
            id: link_id.clone(),
            source_id: params.source_id.clone(),
            target_id: params.target_id.clone(),
            link_type: params.link_type.clone(),
            context: params.context.clone(),
        },
    )
    .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "id": link.id,
        "source_id": link.source_id,
        "target_id": link.target_id,
        "link_type": link.link_type,
        "context": link.context
    }))?)
}

async fn execute_memory_link_list(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        memory_id: String,
    }

    let params: Params = serde_json::from_value(args)?;

    let links = db::get_memory_links(&state.db, &params.memory_id).await?;

    let links_json: Vec<_> = links
        .iter()
        .map(|l| serde_json::json!({
            "id": l.id,
            "target_id": l.target_id,
            "link_type": l.link_type,
            "context": l.context
        }))
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "memory_id": params.memory_id,
        "count": links_json.len(),
        "links": links_json
    }))?)
}

async fn execute_codebase_index(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        path: Option<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Create a background job for indexing
    let job_id = crate::models::new_id();

    let job = db::create_job(
        &state.db,
        db::CreateJob::new(job_id.clone(), db::JobType::IndexRepo)
            .with_project(project.id.clone()),
    )
    .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "message": "Indexing job created",
        "job_id": job.id,
        "status": job.status,
        "project": project.slug,
        "path": params.path
    }))?)
}

async fn execute_codebase_search(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        query: String,
        #[serde(default = "default_limit")]
        limit: usize,
    }

    fn default_limit() -> usize {
        10
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Search for file-source memories
    let results = state
        .memory
        .search(&project.id, &project.slug, &params.query, params.limit * 2)
        .await?;

    // Filter to only file-source memories
    let file_results: Vec<_> = results
        .into_iter()
        .filter(|r| {
            r.memory.source.as_deref()
                .and_then(MemorySource::from_str)
                .map(|s| s == MemorySource::File)
                .unwrap_or(false)
        })
        .take(params.limit)
        .collect();

    let results_json: Vec<_> = file_results
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.memory.id,
                "file_path": r.memory.file_path,
                "language": r.memory.language,
                "title": r.memory.title,
                "content": r.memory.content.as_deref().unwrap_or("").chars().take(500).collect::<String>(),
                "score": r.score
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

async fn execute_file_upload(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        path: String,
        content: String,
        author: Option<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Detect language from file extension
    let language = std::path::Path::new(&params.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase());

    // Create memory via service (handles DB + Qdrant)
    let memory = state
        .memory
        .add(
            &project.id,
            &project.slug,
            MemoryCreate {
                memory_type: MemoryType::Codebase,
                content: params.content,
                author: params.author,
                title: Some(params.path.clone()),
                file_path: Some(params.path.clone()),
                language,
                source: Some(MemorySource::File),
                ..Default::default()
            },
            true,
        )
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "message": "File uploaded and indexed",
        "memory_id": memory.id,
        "path": params.path,
        "source": "file"
    }))?)
}

async fn execute_files_upload(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct FileItem {
        path: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct Params {
        project: String,
        files: Vec<FileItem>,
        author: Option<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut memories = Vec::new();

    for file in params.files {
        // Detect language
        let language = std::path::Path::new(&file.path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase());

        // Try to create memory
        match state
            .memory
            .add(
                &project.id,
                &project.slug,
                MemoryCreate {
                    memory_type: MemoryType::Codebase,
                    content: file.content,
                    author: params.author.clone(),
                    title: Some(file.path.clone()),
                    file_path: Some(file.path.clone()),
                    language,
                    source: Some(MemorySource::File),
                    ..Default::default()
                },
                false, // Skip auto-metadata for batch operations
            )
            .await
        {
            Ok(memory) => {
                success_count += 1;
                memories.push(serde_json::json!({
                    "id": memory.id,
                    "path": file.path
                }));
            }
            Err(e) => {
                failed_count += 1;
                tracing::warn!(path = %file.path, error = %e, "Failed to upload file");
            }
        }
    }

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "message": "Batch upload completed",
        "project": project.slug,
        "success_count": success_count,
        "failed_count": failed_count,
        "memories": memories
    }))?)
}
