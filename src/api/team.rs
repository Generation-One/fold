//! Team Routes
//!
//! Team status and activity tracking.
//!
//! Routes:
//! - GET /projects/:project_id/team - View team status
//! - POST /projects/:project_id/team/status - Update own status
//! - GET /projects/:project_id/team/activity - Get recent team activity

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, Result};

/// Build team routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(view_team_status))
        .route("/status", post(update_status))
        .route("/activity", get(get_activity))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Team member status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    #[default]
    Active,
    Idle,
    Away,
    DoNotDisturb,
    Offline,
}

/// Request to update own status.
#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    /// New status
    pub status: MemberStatus,
    /// What you're currently working on
    pub current_task: Option<String>,
    /// Status message
    pub message: Option<String>,
    /// Auto-expire status after N minutes
    pub expire_after_minutes: Option<u32>,
}

/// Query parameters for activity.
#[derive(Debug, Deserialize, Default)]
pub struct ActivityQuery {
    /// Filter by member
    pub member: Option<String>,
    /// Filter by activity type
    #[serde(rename = "type")]
    pub activity_type: Option<ActivityType>,
    /// Only return activities after this time
    pub after: Option<DateTime<Utc>>,
    /// Pagination
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    50
}

/// Activity types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    StatusChange,
    SessionStart,
    SessionEnd,
    MemoryAdded,
    FileChanged,
    CodePushed,
    PrOpened,
    PrMerged,
}

/// Team member information.
#[derive(Debug, Serialize)]
pub struct TeamMember {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: MemberStatus,
    pub current_task: Option<String>,
    pub message: Option<String>,
    pub active_session_id: Option<Uuid>,
    pub last_seen_at: DateTime<Utc>,
}

/// Team status response.
#[derive(Debug, Serialize)]
pub struct TeamStatusResponse {
    pub members: Vec<TeamMember>,
    pub total_active: u32,
    pub total_members: u32,
}

/// Activity entry.
#[derive(Debug, Serialize)]
pub struct ActivityEntry {
    pub id: Uuid,
    pub member_id: Uuid,
    pub member_name: String,
    #[serde(rename = "type")]
    pub activity_type: ActivityType,
    pub description: String,
    pub metadata: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Activity response.
#[derive(Debug, Serialize)]
pub struct ActivityResponse {
    pub activities: Vec<ActivityEntry>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

/// Status update response.
#[derive(Debug, Serialize)]
pub struct StatusUpdateResponse {
    pub status: MemberStatus,
    pub message: String,
    pub expires_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// View team status for a project.
///
/// GET /projects/:project_id/team
///
/// Returns the current status of all team members on this project.
#[axum::debug_handler]
async fn view_team_status(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
) -> Result<Json<TeamStatusResponse>> {
    // TODO: Fetch team members from database
    // TODO: Get current status for each member

    Ok(Json(TeamStatusResponse {
        members: vec![],
        total_active: 0,
        total_members: 0,
    }))
}

/// Update own status.
///
/// POST /projects/:project_id/team/status
///
/// Updates the current user's status and optional task/message.
#[axum::debug_handler]
async fn update_status(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
    Json(request): Json<UpdateStatusRequest>,
) -> Result<Json<StatusUpdateResponse>> {
    // TODO: Get current user from auth context
    // TODO: Update status in database
    // TODO: Broadcast status change to team (WebSocket)

    let expires_at = request.expire_after_minutes.map(|minutes| {
        Utc::now() + chrono::Duration::minutes(minutes as i64)
    });

    // TODO: Log activity

    Ok(Json(StatusUpdateResponse {
        status: request.status,
        message: "Status updated".into(),
        expires_at,
    }))
}

/// Get recent team activity.
///
/// GET /projects/:project_id/team/activity
///
/// Returns a feed of recent team activity including status changes,
/// sessions, commits, and other notable events.
#[axum::debug_handler]
async fn get_activity(
    State(_state): State<AppState>,
    Path(_path): Path<ProjectPath>,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityResponse>> {
    let limit = query.limit.min(100);

    // TODO: Fetch activity from database with filters

    Ok(Json(ActivityResponse {
        activities: vec![],
        total: 0,
        offset: query.offset,
        limit,
    }))
}
