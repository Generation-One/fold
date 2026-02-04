//! User management API endpoints.
//!
//! Provides CRUD operations for user management (admin only).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{self, User, UpdateUser, UserRole},
    error::{Error, Result},
    middleware::AuthUser,
    AppState,
};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub provider: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub created_at: String,
    pub last_login: Option<String>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        UserResponse {
            id: user.id,
            provider: user.provider,
            email: user.email,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
            role: user.role,
            created_at: user.created_at,
            last_login: user.last_login,
        }
    }
}

// ============================================================================
// Routes
// ============================================================================

pub fn routes(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", axum::routing::get(list_users))
        .route("/:id", axum::routing::get(get_user).patch(update_user).delete(delete_user))
}

// ============================================================================
// Handlers
// ============================================================================

/// List all users (admin only).
async fn list_users(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<UserResponse>>> {
    // Only admins can list users
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    let users = db::list_users(&state.db, None).await?;
    Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

/// Get a specific user by ID (admin only).
async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<UserResponse>> {
    // Only admins can get user details
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    let user = db::get_user(&state.db, &id).await?;
    Ok(Json(UserResponse::from(user)))
}

/// Update a user (admin only).
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>> {
    // Only admins can update users
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    let role = request.role.as_ref().map(|r| UserRole::from_str(r));

    let update = UpdateUser {
        email: request.email,
        display_name: request.display_name,
        avatar_url: request.avatar_url,
        role,
    };

    let user = db::update_user(&state.db, &id, update).await?;
    Ok(Json(UserResponse::from(user)))
}

/// Delete a user (admin only).
async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
) -> Result<()> {
    // Only admins can delete users
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    // Prevent self-deletion
    if id == auth.user_id {
        return Err(Error::InvalidInput("Cannot delete your own user account".to_string()));
    }

    db::delete_user(&state.db, &id).await?;
    Ok(())
}

// Required for axum extractors
use axum::extract::Extension;
