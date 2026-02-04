//! User management API endpoints.
//!
//! Provides CRUD operations for user management. Listing users is available to all
//! authenticated users (for project membership), while CRUD operations on users are
//! restricted to administrators.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx;

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
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<String>,
}

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
        .route("/", axum::routing::get(list_users).post(create_user))
        .route("/:id", axum::routing::get(get_user).patch(update_user).delete(delete_user))
}

// ============================================================================
// Handlers
// ============================================================================

/// List all users (authenticated users can search for users to add to projects).
async fn list_users(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<UserResponse>>> {
    // All authenticated users can list users (for adding to projects)
    let users = db::list_users(&state.db, None).await?;
    Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

/// Create a new user (admin only).
async fn create_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(request): Json<CreateUserRequest>,
) -> Result<impl IntoResponse> {
    // Only admins can create users
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    if request.email.is_empty() {
        return Err(Error::InvalidInput("Email is required".to_string()));
    }

    let user_id = uuid::Uuid::new_v4().to_string();
    let role_str = request.role.as_deref().unwrap_or("member");

    // Insert the user directly
    sqlx::query(
        r#"
        INSERT INTO users (id, provider, subject, email, display_name, role, created_at)
        VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(&user_id)
    .bind("admin-created")
    .bind(&request.email)
    .bind(&request.email)
    .bind(&request.display_name)
    .bind(role_str)
    .execute(&state.db)
    .await
    .map_err(|e| Error::Database(e))?;

    // Fetch the created user
    let user = db::get_user(&state.db, &user_id).await?;

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
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

/// Update a user (admins can edit any user, users can edit themselves but not roles).
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(auth): Extension<AuthUser>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>> {
    // Check permissions: admins can edit anyone, users can only edit themselves
    if id != auth.user_id && !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    // Non-admins cannot change roles
    let role = if auth.is_admin() {
        request.role.as_ref().map(|r| UserRole::from_str(r))
    } else {
        // Users can't change their own role
        None
    };

    // Prevent users from making themselves admin (even if role is somehow provided)
    if !auth.is_admin() && request.role.as_deref() == Some("admin") {
        return Err(Error::InvalidInput("Users cannot grant themselves admin role".to_string()));
    }

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
