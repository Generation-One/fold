//! Group management API endpoints.
//!
//! Provides CRUD operations for user groups and group membership management.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    db::{self, Group, GroupMember},
    error::{Error, Result},
    middleware::AuthUser,
    AppState,
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddGroupMemberRequest {
    pub user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Group> for GroupResponse {
    fn from(group: Group) -> Self {
        GroupResponse {
            id: group.id,
            name: group.name,
            description: group.description,
            is_system: group.is_system != 0,
            created_by: group.created_by,
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupMemberResponse {
    pub group_id: String,
    pub user_id: String,
    pub added_by: Option<String>,
    pub created_at: String,
}

impl From<GroupMember> for GroupMemberResponse {
    fn from(member: GroupMember) -> Self {
        GroupMemberResponse {
            group_id: member.group_id,
            user_id: member.user_id,
            added_by: member.added_by,
            created_at: member.created_at,
        }
    }
}

// ============================================================================
// Routes
// ============================================================================

pub fn routes(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", axum::routing::get(list_groups).post(create_group))
        .route("/:id", axum::routing::get(get_group).patch(update_group).delete(delete_group))
        .route("/:id/members", axum::routing::get(list_group_members).post(add_group_member))
        .route("/:id/members/:user_id", axum::routing::delete(remove_group_member))
}

// ============================================================================
// Handlers
// ============================================================================

/// List all groups.
async fn list_groups(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
) -> Result<Json<Vec<GroupResponse>>> {
    let groups = db::list_groups(&state.db).await?;
    Ok(Json(groups.into_iter().map(GroupResponse::from).collect()))
}

/// Get a specific group by ID.
async fn get_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<GroupResponse>> {
    let group = db::get_group(&state.db, &id).await?;
    Ok(Json(GroupResponse::from(group)))
}

/// Create a new group (admin only).
async fn create_group(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(request): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<GroupResponse>)> {
    // Only admins can create groups
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    // Validate name
    let name = request.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(Error::InvalidInput("Group name must be 1-255 characters".to_string()));
    }

    // Check if name already exists
    if let Ok(Some(_)) = db::get_group_by_name(&state.db, name).await {
        return Err(Error::AlreadyExists(format!("Group '{}' already exists", name)));
    }

    let group = db::create_group(
        &state.db,
        &Uuid::new_v4().to_string(),
        name,
        request.description.as_deref(),
        Some(&auth.user_id),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(GroupResponse::from(group))))
}

/// Update a group (admin only).
async fn update_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
    Json(request): Json<UpdateGroupRequest>,
) -> Result<Json<GroupResponse>> {
    // Only admins can update groups
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    let group = db::update_group(&state.db, &id, request.into()).await?;
    Ok(Json(GroupResponse::from(group)))
}

/// Delete a group (admin only, cannot delete system groups).
async fn delete_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
) -> Result<()> {
    // Only admins can delete groups
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    db::delete_group(&state.db, &id).await?;
    Ok(())
}

/// List members of a group.
async fn list_group_members(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<GroupMemberResponse>>> {
    let members = db::list_group_members(&state.db, &id).await?;
    Ok(Json(members.into_iter().map(GroupMemberResponse::from).collect()))
}

/// Add a user to a group (admin only).
async fn add_group_member(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
    Json(request): Json<AddGroupMemberRequest>,
) -> Result<(StatusCode, Json<GroupMemberResponse>)> {
    // Only admins can manage group members
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    // Verify group exists
    let _ = db::get_group(&state.db, &id).await?;

    let member = db::add_group_member(&state.db, &id, &request.user_id, Some(&auth.user_id)).await?;
    Ok((StatusCode::CREATED, Json(GroupMemberResponse::from(member))))
}

/// Remove a user from a group (admin only).
async fn remove_group_member(
    State(state): State<AppState>,
    Path((group_id, user_id)): Path<(String, String)>,
    Extension(auth): Extension<AuthUser>,
) -> Result<()> {
    // Only admins can manage group members
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    // Special handling for admin group: ensure at least one admin remains
    if group_id == "group_admin" {
        let count = db::count_admin_users(&state.db, &group_id).await?;
        if count <= 1 {
            return Err(Error::InvalidInput(
                "Cannot remove the last admin from the admin group".to_string(),
            ));
        }
    }

    let removed = db::remove_group_member(&state.db, &group_id, &user_id).await?;
    if !removed {
        return Err(Error::NotFound(format!(
            "User {} not in group {}",
            user_id, group_id
        )));
    }

    Ok(())
}

// Required for axum extractors
use axum::extract::Extension;
