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
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{MemoryCreate, MemoryType};
use crate::{db, AppState, Error, Result};

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
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<ListSessionsResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;
    let limit = query.limit.min(100) as i64;
    let offset = query.offset as i64;

    let sessions = db::list_project_ai_sessions(&state.db, &project.id, limit, offset).await?;

    let session_summaries: Vec<SessionSummary> = sessions
        .iter()
        .map(|s| {
            let started = DateTime::parse_from_rfc3339(&s.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let ended = s.ended_at.as_ref().and_then(|e| {
                DateTime::parse_from_rfc3339(e).ok().map(|dt| dt.with_timezone(&Utc))
            });

            let duration = ended.map(|e| ((e - started).num_minutes() as u32).max(1));

            SessionSummary {
                id: Uuid::parse_str(&s.id).unwrap_or_else(|_| Uuid::new_v4()),
                title: Some(s.task.clone()),
                focus: s.summary.clone(),
                status: match s.status.as_str() {
                    "active" => SessionStatus::Active,
                    "paused" => SessionStatus::Paused,
                    "completed" => SessionStatus::Ended,
                    _ => SessionStatus::Active,
                },
                author: s.agent_type.clone(),
                note_count: 0, // Would need to count notes
                started_at: started,
                ended_at: ended,
                duration_minutes: duration,
            }
        })
        .collect();

    Ok(Json(ListSessionsResponse {
        sessions: session_summaries,
        total: sessions.len() as u32,
        offset: query.offset,
        limit: query.limit,
    }))
}

/// Start a new development session.
///
/// POST /projects/:project_id/sessions
#[axum::debug_handler]
async fn start_session(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<StartSessionRequest>,
) -> Result<Json<SessionResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Generate session ID
    let session_id = crate::models::new_id();

    // Create task description from title and focus
    let task = request.title.clone().unwrap_or_else(|| {
        request.focus.clone().unwrap_or_else(|| "Development session".to_string())
    });

    // Create session in database
    let session = db::create_ai_session(
        &state.db,
        db::CreateAiSession {
            id: session_id.clone(),
            project_id: project.id.clone(),
            task,
            local_root: None,
            repository_id: None,
            agent_type: Some("user".to_string()),
        },
    )
    .await?;

    // If initial notes provided, create a note
    if let Some(notes) = request.notes {
        if !notes.trim().is_empty() {
            db::create_session_note(
                &state.db,
                db::CreateSessionNote {
                    id: crate::models::new_id(),
                    session_id: session.id.clone(),
                    note_type: db::NoteType::Progress,
                    content: notes,
                },
            )
            .await?;
        }
    }

    let started_at = DateTime::parse_from_rfc3339(&session.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(Json(SessionResponse {
        id: Uuid::parse_str(&session.id).unwrap_or_else(|_| Uuid::new_v4()),
        project_id: Uuid::parse_str(&project.id).unwrap_or_else(|_| Uuid::new_v4()),
        title: Some(session.task.clone()),
        focus: request.focus,
        status: SessionStatus::Active,
        author: session.agent_type.clone(),
        notes: vec![],
        active_files: request.active_files,
        summary: None,
        outcomes: vec![],
        next_steps: session.next_steps_vec(),
        started_at,
        ended_at: None,
        duration_minutes: None,
        metadata: request.metadata,
    }))
}

/// Get session details.
///
/// GET /projects/:project_id/sessions/:session_id
#[axum::debug_handler]
async fn get_session(
    State(state): State<AppState>,
    Path(path): Path<SessionPath>,
) -> Result<Json<SessionResponse>> {
    let session = db::get_ai_session(&state.db, &path.session_id.to_string()).await?;
    let project = db::get_project(&state.db, &session.project_id).await?;

    // Get session notes
    let notes = db::list_session_notes(&state.db, &session.id).await?;

    let session_notes: Vec<SessionNote> = notes
        .iter()
        .map(|n| {
            let created_at = DateTime::parse_from_rfc3339(&n.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            SessionNote {
                id: Uuid::parse_str(&n.id).unwrap_or_else(|_| Uuid::new_v4()),
                content: n.content.clone(),
                note_type: match n.note_type.as_str() {
                    "decision" => NoteType::Decision,
                    "todo" => NoteType::Todo,
                    "question" => NoteType::Question,
                    "bug" => NoteType::Bug,
                    "finding" => NoteType::Idea,
                    _ => NoteType::Note,
                },
                file_path: None,
                tags: vec![],
                created_at,
            }
        })
        .collect();

    let started_at = DateTime::parse_from_rfc3339(&session.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let ended_at = session.ended_at.as_ref().and_then(|e| {
        DateTime::parse_from_rfc3339(e).ok().map(|dt| dt.with_timezone(&Utc))
    });

    let duration = ended_at.map(|e| ((e - started_at).num_minutes() as u32).max(1));

    let status = match session.status.as_str() {
        "active" => SessionStatus::Active,
        "paused" => SessionStatus::Paused,
        "completed" => SessionStatus::Ended,
        "blocked" => SessionStatus::Paused,
        _ => SessionStatus::Active,
    };

    let next_steps = session.next_steps_vec();

    Ok(Json(SessionResponse {
        id: Uuid::parse_str(&session.id).unwrap_or_else(|_| Uuid::new_v4()),
        project_id: Uuid::parse_str(&project.id).unwrap_or_else(|_| Uuid::new_v4()),
        title: Some(session.task.clone()),
        focus: session.summary.clone(),
        status,
        author: session.agent_type,
        notes: session_notes,
        active_files: vec![],
        summary: session.summary,
        outcomes: vec![],
        next_steps,
        started_at,
        ended_at,
        duration_minutes: duration,
        metadata: serde_json::Value::Null,
    }))
}

/// Add notes to a session.
///
/// POST /projects/:project_id/sessions/:session_id/notes
#[axum::debug_handler]
async fn add_notes(
    State(state): State<AppState>,
    Path(path): Path<SessionPath>,
    Json(request): Json<AddNotesRequest>,
) -> Result<Json<SessionNote>> {
    // Validate content
    if request.content.trim().is_empty() {
        return Err(Error::Validation("Note content cannot be empty".into()));
    }

    // Verify session exists
    let session = db::get_ai_session(&state.db, &path.session_id.to_string()).await?;

    // Check session is active
    if session.is_ended() {
        return Err(Error::Validation("Cannot add notes to ended session".into()));
    }

    // Map note type to db type
    let db_note_type = match request.note_type {
        NoteType::Decision => db::NoteType::Decision,
        NoteType::Todo => db::NoteType::Progress,
        NoteType::Question => db::NoteType::Question,
        NoteType::Bug => db::NoteType::Finding,
        NoteType::Idea => db::NoteType::Finding,
        NoteType::Note => db::NoteType::Progress,
    };

    // Create note in database
    let note = db::create_session_note(
        &state.db,
        db::CreateSessionNote {
            id: crate::models::new_id(),
            session_id: session.id,
            note_type: db_note_type,
            content: request.content.clone(),
        },
    )
    .await?;

    let created_at = DateTime::parse_from_rfc3339(&note.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(Json(SessionNote {
        id: Uuid::parse_str(&note.id).unwrap_or_else(|_| Uuid::new_v4()),
        content: note.content,
        note_type: request.note_type,
        file_path: request.file_path,
        tags: request.tags,
        created_at,
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
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Get session
    let session = db::get_ai_session(&state.db, &path.session_id.to_string()).await?;

    if session.is_ended() {
        return Err(Error::Validation("Session is already ended".into()));
    }

    // Generate summary if not provided
    let summary = match request.summary {
        Some(s) => s,
        None => generate_session_summary(&state, &session.id).await?,
    };

    // End session in database
    let ended_session = db::end_ai_session(&state.db, &session.id, Some(&summary)).await?;

    // Save session as memory if requested
    if request.save_as_memory {
        let memory_content = format!(
            "# Session: {}\n\n## Summary\n{}\n\n## Next Steps\n{}",
            ended_session.task,
            summary,
            request.next_steps.join("\n- ")
        );

        state
            .memory
            .add(
                &project.id,
                &project.slug,
                MemoryCreate {
                    memory_type: MemoryType::Session,
                    content: memory_content,
                    author: ended_session.agent_type.clone(),
                    title: Some(ended_session.task.clone()),
                    ..Default::default()
                },
                true,
            )
            .await?;
    }

    let started_at = DateTime::parse_from_rfc3339(&ended_session.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let ended_at = ended_session.ended_at.as_ref().and_then(|e| {
        DateTime::parse_from_rfc3339(e).ok().map(|dt| dt.with_timezone(&Utc))
    });

    let duration = ended_at.map(|e| ((e - started_at).num_minutes() as u32).max(1));

    Ok(Json(SessionResponse {
        id: Uuid::parse_str(&ended_session.id).unwrap_or_else(|_| Uuid::new_v4()),
        project_id: Uuid::parse_str(&project.id).unwrap_or_else(|_| Uuid::new_v4()),
        title: Some(ended_session.task),
        focus: None,
        status: SessionStatus::Ended,
        author: ended_session.agent_type,
        notes: vec![],
        active_files: vec![],
        summary: Some(summary),
        outcomes: request.outcomes,
        next_steps: request.next_steps,
        started_at,
        ended_at,
        duration_minutes: duration,
        metadata: serde_json::Value::Null,
    }))
}

/// Get current workspace state.
///
/// GET /projects/:project_id/workspace
#[axum::debug_handler]
async fn get_workspace(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
) -> Result<Json<WorkspaceState>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Get active sessions for this project
    let active_sessions = db::list_active_ai_sessions(&state.db, &project.id).await?;
    let active_session_id = active_sessions.first().map(|s| {
        Uuid::parse_str(&s.id).unwrap_or_else(|_| Uuid::new_v4())
    });

    Ok(Json(WorkspaceState {
        active_session_id,
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
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceState>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Get active sessions for this project
    let active_sessions = db::list_active_ai_sessions(&state.db, &project.id).await?;
    let active_session_id = active_sessions.first().map(|s| {
        Uuid::parse_str(&s.id).unwrap_or_else(|_| Uuid::new_v4())
    });

    // For now, just return the updated state (would persist to cache/DB in production)
    Ok(Json(WorkspaceState {
        active_session_id,
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
async fn generate_session_summary(state: &AppState, session_id: &str) -> Result<String> {
    // Fetch session notes
    let notes = db::list_session_notes(&state.db, session_id).await?;

    if notes.is_empty() {
        return Ok("No notes recorded during this session.".to_string());
    }

    // Build context from notes
    let notes_text: Vec<String> = notes
        .iter()
        .map(|n| format!("[{}] {}", n.note_type, n.content))
        .collect();

    let context = notes_text.join("\n");

    // Generate summary using LLM
    let summary = state
        .llm
        .summarize_session(&context)
        .await
        .unwrap_or_else(|_| {
            format!(
                "Session with {} notes covering: {}",
                notes.len(),
                notes.first().map(|n| n.content.chars().take(100).collect::<String>()).unwrap_or_default()
            )
        });

    Ok(summary)
}
