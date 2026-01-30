//! MCP Routes
//!
//! Model Context Protocol (MCP) JSON-RPC endpoint for AI assistant integration.
//!
//! Routes:
//! - POST /mcp - JSON-RPC 2.0 endpoint for MCP tools

use axum::{
    extract::State,
    middleware,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::middleware::require_token;
use crate::{AppState, Error, Result};

/// Build MCP routes.
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", post(handle_mcp_request))
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

/// Handle MCP JSON-RPC request.
///
/// POST /mcp
///
/// Implements the MCP protocol for AI assistant tool integration.
/// Supports tools/list and tools/call methods.
#[axum::debug_handler]
async fn handle_mcp_request(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Json(JsonRpcResponse::error(
            request.id,
            INVALID_REQUEST,
            "Invalid JSON-RPC version".into(),
            None,
        ));
    }

    // Route to appropriate handler
    let response = match request.method.as_str() {
        "initialize" => handle_initialize(request.id.clone()),
        "tools/list" => handle_tools_list(request.id.clone()),
        "tools/call" => handle_tools_call(&state, request.id.clone(), request.params).await,
        "resources/list" => handle_resources_list(request.id.clone()),
        "resources/read" => handle_resources_read(request.id.clone(), request.params),
        _ => JsonRpcResponse::error(
            request.id,
            METHOD_NOT_FOUND,
            format!("Method not found: {}", request.method),
            None,
        ),
    };

    Json(response)
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
                    "root_path": { "type": "string", "description": "Local path to codebase" },
                    "repo_url": { "type": "string", "description": "Git repository URL" }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "memory_add".into(),
            description: "Add a memory to a project. Auto-generates keywords, tags, and context.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "content": { "type": "string", "description": "Memory content" },
                    "type": {
                        "type": "string",
                        "enum": ["codebase", "session", "spec", "decision", "task", "general"],
                        "default": "general",
                        "description": "Memory type"
                    },
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
                    "type": {
                        "type": "string",
                        "enum": ["codebase", "session", "spec", "decision", "task", "general"],
                        "description": "Filter by memory type"
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
                    "type": { "type": "string", "description": "Filter by memory type" },
                    "author": { "type": "string", "description": "Filter by author" },
                    "limit": { "type": "integer", "default": 20 }
                },
                "required": ["project"]
            }),
        },
        ToolDefinition {
            name: "context_get".into(),
            description: "Get relevant context for a task (searches code, specs, decisions, sessions)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "task": { "type": "string", "description": "Task or question to get context for" },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["project", "task"]
            }),
        },
        ToolDefinition {
            name: "codebase_index".into(),
            description: "Index a project's codebase".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "path": { "type": "string", "description": "Path to codebase (optional, uses project default)" },
                    "author": { "type": "string", "description": "Who triggered the indexing" }
                },
                "required": ["project"]
            }),
        },
        ToolDefinition {
            name: "codebase_search".into(),
            description: "Search indexed codebase".into(),
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
            name: "team_status".into(),
            description: "View or update team activity".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "action": {
                        "type": "string",
                        "enum": ["view", "update"],
                        "default": "view"
                    },
                    "username": { "type": "string", "description": "Your username (for update)" },
                    "status": {
                        "type": "string",
                        "enum": ["active", "idle", "away"],
                        "default": "active"
                    },
                    "current_task": { "type": "string", "description": "What you're working on" }
                },
                "required": ["project"]
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
        "context_get" => execute_context_get(state, call_params.arguments).await,
        "codebase_index" => execute_codebase_index(state, call_params.arguments).await,
        "codebase_search" => execute_codebase_search(state, call_params.arguments).await,
        "team_status" => execute_team_status(state, call_params.arguments).await,
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

async fn execute_project_list(_state: &AppState) -> Result<String> {
    // TODO: Implement
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "count": 0,
        "projects": []
    }))?)
}

async fn execute_project_create(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("Project created: {:?}", args))
}

async fn execute_memory_add(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("Memory added: {:?}", args))
}

async fn execute_memory_search(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        query: String,
        #[serde(default = "default_limit")]
        limit: u32,
    }

    fn default_limit() -> u32 {
        10
    }

    let params: Params = serde_json::from_value(args)?;

    // Generate embedding
    let _embedding = state
        .embeddings
        .embed_single(&params.query)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    // TODO: Search Qdrant
    // TODO: Fetch full memories from database

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project": params.project,
        "query": params.query,
        "results": []
    }))?)
}

async fn execute_memory_list(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("Memory list for: {:?}", args))
}

async fn execute_context_get(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        task: String,
        #[serde(default = "default_limit")]
        limit: u32,
    }

    fn default_limit() -> u32 {
        10
    }

    let params: Params = serde_json::from_value(args)?;

    // Generate embedding
    let _embedding = state
        .embeddings
        .embed_single(&params.task)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    // TODO: Search for relevant context

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "task": params.task,
        "relevant_code": [],
        "specifications": [],
        "decisions": [],
        "recent_sessions": [],
        "other": []
    }))?)
}

async fn execute_codebase_index(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("Indexing triggered: {:?}", args))
}

async fn execute_codebase_search(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        query: String,
        #[serde(default = "default_limit")]
        limit: u32,
    }

    fn default_limit() -> u32 {
        10
    }

    let params: Params = serde_json::from_value(args)?;

    // Generate embedding
    let _embedding = state
        .embeddings
        .embed_single(&params.query)
        .await
        .map_err(|e| Error::Embedding(e.to_string()))?;

    // TODO: Search code index

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project": params.project,
        "query": params.query,
        "results": []
    }))?)
}

async fn execute_team_status(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("Team status: {:?}", args))
}

async fn execute_file_upload(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("File uploaded: {:?}", args))
}

async fn execute_files_upload(_state: &AppState, args: Value) -> Result<String> {
    // TODO: Implement
    Ok(format!("Files uploaded: {:?}", args))
}
