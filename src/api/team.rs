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

use crate::{db, AppState, Result};

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
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
) -> Result<Json<TeamStatusResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Fetch all team members for this project
    let team_statuses = db::list_team_status(&state.db, &project.id).await?;

    // Convert database records to API response types
    let members: Vec<TeamMember> = team_statuses
        .iter()
        .map(|ts| {
            let last_seen = chrono::DateTime::parse_from_rfc3339(&ts.last_seen)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let active_session_id = ts.session_start.as_ref().map(|_| {
                // Parse existing ID or generate new UUID
                ts.id.parse().unwrap_or_else(|_| Uuid::new_v4())
            });

            TeamMember {
                id: ts.id.parse().unwrap_or_else(|_| Uuid::new_v4()),
                username: ts.username.clone(),
                display_name: ts.username.clone(), // Use username as display name
                avatar_url: None,
                status: match ts.status.as_str() {
                    "active" => MemberStatus::Active,
                    "idle" => MemberStatus::Idle,
                    "away" => MemberStatus::Away,
                    _ => MemberStatus::Idle,
                },
                current_task: ts.current_task.clone(),
                message: None,
                active_session_id,
                last_seen_at: last_seen,
            }
        })
        .collect();

    let total_active = members.iter().filter(|m| m.status == MemberStatus::Active).count() as u32;
    let total_members = members.len() as u32;

    Ok(Json(TeamStatusResponse {
        members,
        total_active,
        total_members,
    }))
}

/// Update own status.
///
/// POST /projects/:project_id/team/status
///
/// Updates the current user's status and optional task/message.
#[axum::debug_handler]
async fn update_status(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    axum::Extension(auth): axum::Extension<crate::middleware::AuthContext>,
    Json(request): Json<UpdateStatusRequest>,
) -> Result<Json<StatusUpdateResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    let username = &auth.user_id;

    // Map API status to database status
    let db_status = match request.status {
        MemberStatus::Active => db::TeamMemberStatus::Active,
        MemberStatus::Idle => db::TeamMemberStatus::Idle,
        MemberStatus::Away | MemberStatus::DoNotDisturb | MemberStatus::Offline => db::TeamMemberStatus::Away,
    };

    // Update status in database
    let _team_status = db::upsert_team_status(
        &state.db,
        &project.id,
        username,
        db::UpdateTeamStatus {
            status: db_status,
            current_task: request.current_task.clone(),
            current_files: None, // Could be extended to support files
        },
    )
    .await?;

    let expires_at = request.expire_after_minutes.map(|minutes| {
        Utc::now() + chrono::Duration::minutes(minutes as i64)
    });

    // TODO: Broadcast status change to team (WebSocket)

    Ok(Json(StatusUpdateResponse {
        status: request.status,
        message: request.message.unwrap_or_else(|| "Status updated".into()),
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
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;
    let limit = query.limit.min(100) as i64;
    let offset = query.offset as i64;

    // Synthesize activity from AI sessions (since there's no dedicated activity table)
    // This provides session start/end activity for the team feed
    let sessions = db::list_project_ai_sessions(&state.db, &project.id, limit, offset).await?;

    let mut activities: Vec<ActivityEntry> = Vec::new();

    for session in sessions {
        let timestamp = chrono::DateTime::parse_from_rfc3339(&session.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        // Apply time filter if provided
        if let Some(after) = query.after {
            if timestamp < after {
                continue;
            }
        }

        // Apply member filter if provided
        let member_name = session.agent_type.clone().unwrap_or_else(|| "unknown".to_string());
        if let Some(ref filter_member) = query.member {
            if &member_name != filter_member {
                continue;
            }
        }

        let member_id = session.id.parse().unwrap_or_else(|_| Uuid::new_v4());

        // Add session start activity
        if query.activity_type.is_none() || query.activity_type == Some(ActivityType::SessionStart) {
            activities.push(ActivityEntry {
                id: Uuid::new_v4(),
                member_id,
                member_name: member_name.clone(),
                activity_type: ActivityType::SessionStart,
                description: format!("Started session: {}", session.task),
                metadata: serde_json::json!({
                    "session_id": session.id,
                    "task": session.task,
                }),
                timestamp,
            });
        }

        // Add session end activity if session is ended
        if let Some(ref ended_at) = session.ended_at {
            let end_timestamp = chrono::DateTime::parse_from_rfc3339(ended_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            // Apply time filter for end activity
            let include_end = query.after.map(|after| end_timestamp >= after).unwrap_or(true);

            if include_end && (query.activity_type.is_none() || query.activity_type == Some(ActivityType::SessionEnd)) {
                activities.push(ActivityEntry {
                    id: Uuid::new_v4(),
                    member_id,
                    member_name: member_name.clone(),
                    activity_type: ActivityType::SessionEnd,
                    description: format!("Ended session: {}", session.task),
                    metadata: serde_json::json!({
                        "session_id": session.id,
                        "summary": session.summary,
                    }),
                    timestamp: end_timestamp,
                });
            }
        }
    }

    // Sort by timestamp descending (most recent first)
    activities.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Apply pagination
    let total = activities.len() as u32;
    let activities: Vec<ActivityEntry> = activities
        .into_iter()
        .skip(query.offset as usize)
        .take(limit as usize)
        .collect();

    Ok(Json(ActivityResponse {
        activities,
        total,
        offset: query.offset,
        limit: limit as u32,
    }))
}
