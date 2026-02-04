//! Project-level access control middleware.
//!
//! Provides middleware for checking if users have access to specific projects
//! based on direct membership, group membership, or admin role.

use axum::{
    body::Body,
    extract::{Path, Request, State},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::{error::Error, services::PermissionService, AppState};

/// Context injected into requests for project-scoped operations.
#[derive(Clone, Debug)]
pub struct ProjectAccessContext {
    pub project_id: String,
    pub user_id: String,
    pub user_role: String,
}

/// Extract project_id from path parameters.
#[derive(Deserialize)]
struct ProjectIdParams {
    #[serde(alias = "id")]
    project_id: String,
}

/// Middleware that requires read access to a specific project.
///
/// Extracts project ID from path params (supports both `project_id` and `id`)
/// and checks if the user has read access via the permission service.
///
/// Injects `ProjectAccessContext` into request extensions.
///
/// # Errors
///
/// Returns 403 Forbidden if the user lacks read access to the project.
/// Returns 400 Bad Request if project_id cannot be extracted from path.
pub async fn require_project_read(
    State(state): State<AppState>,
    Path(params): Path<ProjectIdParams>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    let auth_user = req
        .extensions()
        .get::<crate::middleware::AuthUser>()
        .cloned()
        .ok_or(Error::Unauthenticated)?;

    let perm_service = PermissionService::new(state.db.clone());

    if !perm_service
        .can_read_project(&auth_user.user_id, &params.project_id, &auth_user.role)
        .await?
    {
        warn!(
            user_id = %auth_user.user_id,
            project_id = %params.project_id,
            "Access denied: user lacks read permission"
        );
        return Err(Error::Forbidden);
    }

    let ctx = ProjectAccessContext {
        project_id: params.project_id,
        user_id: auth_user.user_id,
        user_role: auth_user.role,
    };

    debug!(project_id = %ctx.project_id, user_id = %ctx.user_id, "Project read access granted");

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Middleware that requires write access to a specific project.
///
/// Extracts project ID from path params (supports both `project_id` and `id`)
/// and checks if the user has write access via the permission service.
///
/// Injects `ProjectAccessContext` into request extensions.
///
/// # Errors
///
/// Returns 403 Forbidden if the user lacks write access to the project.
/// Returns 400 Bad Request if project_id cannot be extracted from path.
pub async fn require_project_write(
    State(state): State<AppState>,
    Path(params): Path<ProjectIdParams>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    let auth_user = req
        .extensions()
        .get::<crate::middleware::AuthUser>()
        .cloned()
        .ok_or(Error::Unauthenticated)?;

    let perm_service = PermissionService::new(state.db.clone());

    if !perm_service
        .can_write_project(&auth_user.user_id, &params.project_id, &auth_user.role)
        .await?
    {
        warn!(
            user_id = %auth_user.user_id,
            project_id = %params.project_id,
            "Access denied: user lacks write permission"
        );
        return Err(Error::Forbidden);
    }

    let ctx = ProjectAccessContext {
        project_id: params.project_id,
        user_id: auth_user.user_id,
        user_role: auth_user.role,
    };

    debug!(project_id = %ctx.project_id, user_id = %ctx.user_id, "Project write access granted");

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Middleware that requires admin role (global admin, not project-level).
///
/// Checks if the user has the `admin` global role.
/// Returns 403 Forbidden if the user is not an admin.
pub async fn require_admin(
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    let auth_user = req
        .extensions()
        .get::<crate::middleware::AuthUser>()
        .cloned()
        .ok_or(Error::Unauthenticated)?;

    if !auth_user.is_admin() {
        warn!(user_id = %auth_user.user_id, "Access denied: user is not an admin");
        return Err(Error::Forbidden);
    }

    debug!(user_id = %auth_user.user_id, "Admin access granted");

    Ok(next.run(req).await)
}
