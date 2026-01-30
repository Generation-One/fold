//! Sessions Routes
//!
//! Development session tracking and workspace state management.
//!
//! Routes:
//! - GET /projects/:project_id/sessions - List sessions
//! - POST /projects/:project_id/sessions - Start a new session
//! - GET /projects/:project_id/sessions/:id - Get session details
//! - POST /projects/:project_id/sessions/:id/notes - Add notes to session
//! - POST /projects/:project_id/sessions/:id/end - End session with summary
//! - GET /projects/:project_id/workspace - Get current workspace state
//! - PUT /projects/:project_id/workspace - Update workspace state

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build session routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_sessions).post(start_session))
        .route("/:session_id", get(get_session))
        .route("/:session_id/notes", post(add_notes))
        .route("/:session_id/end", post(end_session))
        // Workspace routes
        .route("/workspace", get(get_workspace).put(update_workspace))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Query parameters for listing sessions.
#[derive(Debug, Deserialize, Default)]
pub struct ListSessionsQuery {
    /// Filter by status
    pub status: Option<SessionStatus>,
    /// Filter by author
    pub author: Option<String>,
    /// Only return sessions after this date
    pub after: Option<DateTime<Utc>>,
    /// Pagination
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

/// Session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    #[default]
    Active,
    Paused,
    Ended,
    Abandoned,
}

/// Request to start a new session.
#[derive(Debug, Deserialize)]
pub struct StartSessionRequest {
    /// Session title/description
    pub title: Option<String>,
    /// Focus area or task
    pub focus: Option<String>,
    /// Initial notes
    pub notes: Option<String>,
    /// Files being worked on
    #[serde(default)]
    pub active_files: Vec<String>,
    /// Session metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Request to add notes to a session.
#[derive(Debug, Deserialize)]
pub struct AddNotesRequest {
    /// Note content
    pub content: String,
    /// Note type
    #[serde(default)]
    pub note_type: NoteType,
    /// Associated file (optional)
    pub file_path: Option<String>,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoteType {
    #[default]
    Note,
    Decision,
    Todo,
    Question,
    Bug,
    Idea,
}

/// Request to end a session.
#[derive(Debug, Deserialize)]
pub struct EndSessionRequest {
    /// Final summary (optional - will be auto-generated if not provided)
    pub summary: Option<String>,
    /// Outcomes achieved
    #[serde(default)]
    pub outcomes: Vec<String>,
    /// Next steps / TODOs
    #[serde(default)]
    pub next_steps: Vec<String>,
    /// Save session as memory
    #[serde(default = "default_true")]
    pub save_as_memory: bool,
}

fn default_true() -> bool {
    true
}

/// Session note.
#[derive(Debug, Serialize)]
pub struct SessionNote {
    pub id: Uuid,
    pub content: String,
    pub note_type: NoteType,
    pub file_path: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Session response.
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: Option<String>,
    pub focus: Option<String>,
    pub status: SessionStatus,
    pub author: Option<String>,
    pub notes: Vec<SessionNote>,
    pub active_files: Vec<String>,
    pub summary: Option<String>,
    pub outcomes: Vec<String>,
    pub next_steps: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_minutes: Option<u32>,
    pub metadata: serde_json::Value,
}

/// List sessions response.
#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionSummary>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

/// Session summary for list view.
#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: Uuid,
    pub title: Option<String>,
    pub focus: Option<String>,
    pub status: SessionStatus,
    pub author: Option<String>,
    pub note_count: u32,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_minutes: Option<u32>,
}

/// Workspace state.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceState {
    /// Currently active session (if any)
    pub active_session_id: Option<Uuid>,
    /// Files currently being worked on
    pub active_files: Vec<ActiveFile>,
    /// Recent context items
    pub recent_context: Vec<RecentContextItem>,
    /// User's current status
    pub user_status: UserStatus,
    /// Custom workspace data
    pub custom: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveFile {
    pub path: String,
    pub language: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
    pub cursor_position: Option<CursorPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentContextItem {
    pub id: Uuid,
    pub title: Option<String>,
    pub snippet: String,
    pub accessed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    #[default]
    Active,
    Idle,
    Away,
    DoNotDisturb,
}

/// Request to update workspace.
#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub active_files: Option<Vec<ActiveFile>>,
    pub recent_context: Option<Vec<RecentContextItem>>,
    pub user_status: Option<UserStatus>,
    pub custom: Option<serde_json::Value>,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionPath {
    pub project_id: String,
    pub session_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// List sessions for a project.
///
/// GET /projects/:project_id/sessions
#[axum::debug_handler]
async fn list_sessions(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<ListSessionsResponse>> {
    let limit = query.limit.min(100);

    // TODO: Fetch sessions from database with filters

    Ok(Json(ListSessionsResponse {
        sessions: vec![],
        total: 0,
        offset: query.offset,
        limit,
    }))
}

/// Start a new development session.
///
/// POST /projects/:project_id/sessions
#[axum::debug_handler]
async fn start_session(
    State(_state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<StartSessionRequest>,
) -> Result<Json<SessionResponse>> {
    let _project_id = &path.project_id;

    // TODO: Check if user already has an active session
    // TODO: Create session in database

    let now = Utc::now();
    let session = SessionResponse {
        id: Uuid::new_v4(),
        project_id: Uuid::new_v4(), // TODO: Parse from path
        title: request.title,
        focus: request.focus,
        status: SessionStatus::Active,
        author: None, // TODO: Get from auth context
        notes: vec![],
        active_files: request.active_files,
        summary: None,
        outcomes: vec![],
        next_steps: vec![],
        started_at: now,
        ended_at: None,
        duration_minutes: None,
        metadata: request.metadata,
    };

    Ok(Json(session))
}

/// Get session details.
///
/// GET /projects/:project_id/sessions/:session_id
#[axum::debug_handler]
async fn get_session(
    State(_state): State<AppState>,
    Path(path): Path<SessionPath>,
) -> Result<Json<SessionResponse>> {
    // TODO: Fetch session from database

    Err(Error::NotFound(format!("Session: {}", path.session_id)))
}

/// Add notes to a session.
///
/// POST /projects/:project_id/sessions/:session_id/notes
#[axum::debug_handler]
async fn add_notes(
    State(_state): State<AppState>,
    Path(path): Path<SessionPath>,
    Json(request): Json<AddNotesRequest>,
) -> Result<Json<SessionNote>> {
    let _session_id = path.session_id;

    // Validate content
    if request.content.trim().is_empty() {
        return Err(Error::Validation("Note content cannot be empty".into()));
    }

    // TODO: Verify session exists and is active
    // TODO: Create note in database

    Ok(Json(SessionNote {
        id: Uuid::new_v4(),
        content: request.content,
        note_type: request.note_type,
        file_path: request.file_path,
        tags: request.tags,
        created_at: Utc::now(),
    }))
}

/// End a session.
///
/// POST /projects/:project_id/sessions/:session_id/end
#[axum::debug_handler]
async fn end_session(
    State(state): State<AppState>,
    Path(path): Path<SessionPath>,
    Json(request): Json<EndSessionRequest>,
) -> Result<Json<SessionResponse>> {
    let _session_id = path.session_id;

    // TODO: Fetch session from database
    // TODO: Verify session is active

    // Generate summary if not provided
    let summary = match request.summary {
        Some(s) => s,
        None => {
            // TODO: Fetch session notes and generate summary using LLM
            generate_session_summary(&state, path.session_id).await?
        }
    };

    // TODO: Update session in database
    // TODO: Create session memory if save_as_memory is true

    Err(Error::NotFound(format!("Session: {}", path.session_id)))
}

/// Get current workspace state.
///
/// GET /projects/:project_id/workspace
#[axum::debug_handler]
async fn get_workspace(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
) -> Result<Json<WorkspaceState>> {
    // TODO: Fetch workspace state from database/cache

    Ok(Json(WorkspaceState {
        active_session_id: None,
        active_files: vec![],
        recent_context: vec![],
        user_status: UserStatus::Active,
        custom: serde_json::Value::Null,
    }))
}

/// Update workspace state.
///
/// PUT /projects/:project_id/workspace
#[axum::debug_handler]
async fn update_workspace(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
    Json(request): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceState>> {
    // TODO: Fetch current workspace state
    // TODO: Apply updates
    // TODO: Save to database/cache

    Ok(Json(WorkspaceState {
        active_session_id: None,
        active_files: request.active_files.unwrap_or_default(),
        recent_context: request.recent_context.unwrap_or_default(),
        user_status: request.user_status.unwrap_or_default(),
        custom: request.custom.unwrap_or(serde_json::Value::Null),
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate a session summary using LLM.
async fn generate_session_summary(state: &AppState, session_id: Uuid) -> Result<String> {
    // TODO: Fetch session notes
    // TODO: Generate summary using LLM

    Ok(format!("Summary for session {}", session_id))
}
