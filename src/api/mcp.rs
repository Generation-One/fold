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

use crate::db;
use crate::middleware::require_token;
use crate::models::{MemoryCreate, MemoryType};
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
            description: "Search memories using semantic similarity with recency/frequency weighting".into(),
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
                    "limit": { "type": "integer", "default": 10, "description": "Max results" },
                    "strength_weight": {
                        "type": "number",
                        "default": 0.3,
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "description": "Weight for recency/frequency vs semantic similarity (0=pure semantic, 1=pure recency)"
                    },
                    "decay_half_life_days": {
                        "type": "number",
                        "default": 30,
                        "minimum": 1,
                        "description": "Half-life in days for memory decay (shorter=favour recent, longer=equal weight)"
                    }
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
            description: "Get relevant context for a task (searches code, specs, decisions, sessions) with recency weighting".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "task": { "type": "string", "description": "Task or question to get context for" },
                    "limit": { "type": "integer", "default": 10 },
                    "strength_weight": {
                        "type": "number",
                        "default": 0.3,
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "description": "Weight for recency/frequency vs semantic similarity"
                    },
                    "decay_half_life_days": {
                        "type": "number",
                        "default": 30,
                        "minimum": 1,
                        "description": "Half-life in days for memory decay"
                    }
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
            description: "Search indexed codebase with recency/frequency weighting".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project ID or slug" },
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "default": 10 },
                    "strength_weight": {
                        "type": "number",
                        "default": 0.3,
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "description": "Weight for recency/frequency vs semantic similarity"
                    },
                    "decay_half_life_days": {
                        "type": "number",
                        "default": 30,
                        "minimum": 1,
                        "description": "Half-life in days for memory decay"
                    }
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
        .create_collection(&project.slug, state.embeddings.dimension())
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
        #[serde(rename = "type", default)]
        memory_type: Option<String>,
        author: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project by ID or slug
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Parse memory type (default to general)
    let memory_type = params
        .memory_type
        .as_deref()
        .and_then(MemoryType::from_str)
        .unwrap_or(MemoryType::General);

    // Create memory via service (handles DB + Qdrant + auto-linking)
    let memory = state
        .memory
        .add(
            &project.id,
            &project.slug,
            MemoryCreate {
                memory_type,
                content: params.content,
                author: params.author,
                tags: params.tags,
                ..Default::default()
            },
            true, // auto-generate metadata
        )
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "id": memory.id,
        "type": memory.memory_type,
        "title": memory.title,
        "content": memory.content.chars().take(200).collect::<String>(),
        "author": memory.author,
        "created_at": memory.created_at.to_rfc3339()
    }))?)
}

async fn execute_memory_search(state: &AppState, args: Value) -> Result<String> {
    use crate::models::SearchParams;

    #[derive(Deserialize)]
    struct Params {
        project: String,
        query: String,
        #[serde(rename = "type")]
        memory_type: Option<String>,
        #[serde(default = "default_limit")]
        limit: usize,
        #[serde(default = "default_strength_weight")]
        strength_weight: f64,
        #[serde(default = "default_half_life")]
        decay_half_life_days: f64,
    }

    fn default_limit() -> usize {
        10
    }

    fn default_strength_weight() -> f64 {
        0.3
    }

    fn default_half_life() -> f64 {
        30.0
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Parse memory type filter
    let memory_type = params.memory_type.as_deref().and_then(MemoryType::from_str);

    // Build search params with decay configuration
    let search_params = SearchParams {
        query: params.query.clone(),
        memory_type,
        limit: params.limit,
        strength_weight: params.strength_weight,
        decay_half_life_days: params.decay_half_life_days,
        ..Default::default()
    };

    // Search via memory service with decay-aware ranking
    let results = state
        .memory
        .search_with_params(&project.id, &project.slug, search_params)
        .await?;

    let results_json: Vec<_> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.memory.id,
                "type": r.memory.memory_type,
                "title": r.memory.title,
                "content": r.memory.content.chars().take(300).collect::<String>(),
                "author": r.memory.author,
                "relevance": r.score,
                "strength": r.strength,
                "combined_score": r.combined_score,
                "file_path": r.memory.file_path,
                "created_at": r.memory.created_at.to_rfc3339()
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project": project.slug,
        "query": params.query,
        "decay_config": {
            "strength_weight": params.strength_weight,
            "half_life_days": params.decay_half_life_days
        },
        "count": results_json.len(),
        "results": results_json
    }))?)
}

async fn execute_memory_list(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        #[serde(rename = "type")]
        memory_type: Option<String>,
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

    // Parse memory type filter
    let memory_type = params.memory_type.as_deref().and_then(MemoryType::from_str);

    // List memories via service
    let memories = state
        .memory
        .list(
            &project.id,
            memory_type,
            params.author.as_deref(),
            params.limit,
            0,
        )
        .await?;

    let memories_json: Vec<_> = memories
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "type": m.memory_type,
                "title": m.title,
                "author": m.author,
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

async fn execute_context_get(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        task: String,
        #[serde(default = "default_limit")]
        limit: usize,
        #[serde(default = "default_strength_weight")]
        strength_weight: f64,
        #[serde(default = "default_half_life")]
        decay_half_life_days: f64,
    }

    fn default_limit() -> usize {
        10
    }

    fn default_strength_weight() -> f64 {
        0.3
    }

    fn default_half_life() -> f64 {
        30.0
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Get context via memory service with decay-aware ranking
    let context = state
        .memory
        .get_context_with_params(
            &project.id,
            &project.slug,
            &params.task,
            None,
            params.limit,
            params.strength_weight,
            params.decay_half_life_days,
        )
        .await?;

    // Collect all memory IDs from the search results
    let memory_ids: Vec<String> = context
        .code
        .iter()
        .chain(context.specs.iter())
        .chain(context.decisions.iter())
        .chain(context.sessions.iter())
        .map(|item| item.id.clone())
        .take(5) // Only get graph context for top 5 results
        .collect();

    // Get graph context for top results (related memories via links)
    let mut related_context = Vec::new();
    for memory_id in memory_ids.iter().take(3) {
        if let Ok(graph_context) = state.graph.get_context(memory_id).await {
            // Add parents
            for parent in graph_context.parents.iter().take(2) {
                related_context.push(serde_json::json!({
                    "id": parent.id,
                    "type": parent.memory_type,
                    "title": parent.title,
                    "relation": "parent",
                    "preview": parent.content_preview
                }));
            }
            // Add related decisions and specs
            for decision in graph_context.decisions.iter().take(2) {
                related_context.push(serde_json::json!({
                    "id": decision.id,
                    "type": "decision",
                    "title": decision.title,
                    "relation": "decision",
                    "preview": decision.content_preview
                }));
            }
            for spec in graph_context.specs.iter().take(2) {
                related_context.push(serde_json::json!({
                    "id": spec.id,
                    "type": "spec",
                    "title": spec.title,
                    "relation": "specification",
                    "preview": spec.content_preview
                }));
            }
        }
    }

    // Deduplicate related context by id
    let mut seen = std::collections::HashSet::new();
    let unique_related: Vec<_> = related_context
        .into_iter()
        .filter(|item| {
            let id = item["id"].as_str().unwrap_or("");
            if seen.contains(id) || memory_ids.contains(&id.to_string()) {
                false
            } else {
                seen.insert(id.to_string());
                true
            }
        })
        .take(5)
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "task": context.task,
        "code": context.code,
        "specifications": context.specs,
        "decisions": context.decisions,
        "sessions": context.sessions,
        "other": context.other,
        "related_context": unique_related
    }))?)
}

async fn execute_codebase_index(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        path: Option<String>,
        author: Option<String>,
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
    use crate::models::SearchParams;

    #[derive(Deserialize)]
    struct Params {
        project: String,
        query: String,
        #[serde(default = "default_limit")]
        limit: usize,
        #[serde(default = "default_strength_weight")]
        strength_weight: f64,
        #[serde(default = "default_half_life")]
        decay_half_life_days: f64,
    }

    fn default_limit() -> usize {
        10
    }

    fn default_strength_weight() -> f64 {
        0.3
    }

    fn default_half_life() -> f64 {
        30.0
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    // Build search params with decay configuration
    let search_params = SearchParams {
        query: params.query.clone(),
        memory_type: Some(MemoryType::Codebase),
        limit: params.limit,
        strength_weight: params.strength_weight,
        decay_half_life_days: params.decay_half_life_days,
        ..Default::default()
    };

    // Search for codebase memories with decay-aware ranking
    let results = state
        .memory
        .search_with_params(&project.id, &project.slug, search_params)
        .await?;

    let results_json: Vec<_> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.memory.id,
                "file_path": r.memory.file_path,
                "language": r.memory.language,
                "title": r.memory.title,
                "content": r.memory.content.chars().take(500).collect::<String>(),
                "relevance": r.score,
                "strength": r.strength,
                "combined_score": r.combined_score
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "project": project.slug,
        "query": params.query,
        "decay_config": {
            "strength_weight": params.strength_weight,
            "half_life_days": params.decay_half_life_days
        },
        "count": results_json.len(),
        "results": results_json
    }))?)
}

async fn execute_team_status(state: &AppState, args: Value) -> Result<String> {
    #[derive(Deserialize)]
    struct Params {
        project: String,
        #[serde(default = "default_action")]
        action: String,
        username: Option<String>,
        status: Option<String>,
        current_task: Option<String>,
    }

    fn default_action() -> String {
        "view".to_string()
    }

    let params: Params = serde_json::from_value(args)?;

    // Get project
    let project = db::get_project_by_id_or_slug(&state.db, &params.project).await?;

    match params.action.as_str() {
        "view" => {
            // List all team status
            let team_status = db::list_team_status(&state.db, &project.id).await?;

            let status_json: Vec<_> = team_status
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "username": s.username,
                        "status": s.status,
                        "current_task": s.current_task,
                        "last_seen": s.last_seen
                    })
                })
                .collect();

            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "project": project.slug,
                "team": status_json
            }))?)
        }
        "update" => {
            let username = params
                .username
                .ok_or_else(|| Error::InvalidInput("username is required for update".into()))?;

            let status = params.status.unwrap_or_else(|| "active".to_string());
            let status_enum = db::TeamMemberStatus::from_str(&status);

            let team_status = db::upsert_team_status(
                &state.db,
                &project.id,
                &username,
                db::UpdateTeamStatus {
                    status: status_enum,
                    current_task: params.current_task,
                    current_files: None,
                },
            )
            .await?;

            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "message": "Team status updated",
                "username": team_status.username,
                "status": team_status.status,
                "current_task": team_status.current_task,
                "last_seen": team_status.last_seen
            }))?)
        }
        _ => Err(Error::InvalidInput(format!(
            "Invalid action: {}. Use 'view' or 'update'",
            params.action
        ))),
    }
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
                ..Default::default()
            },
            true,
        )
        .await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "message": "File uploaded and indexed",
        "memory_id": memory.id,
        "path": params.path,
        "type": memory.memory_type
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
