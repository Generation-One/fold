//! API token authentication middleware.
//!
//! Validates Bearer tokens from the Authorization header for programmatic API access.
//! Used by MCP clients, CLI tools, webhooks, and other automated integrations.
//!
//! Token format: `fold_{prefix}_{random}` where:
//! - `fold_` is a fixed prefix for identification
//! - `{prefix}` is 8 chars used for database lookup (stored as `token_prefix`)
//! - `{random}` is the remaining secret (hashed and stored as `token_hash`)
//!
//! # Security Model
//!
//! - Tokens are looked up by prefix (fast index lookup)
//! - Full token is verified against stored hash (timing-safe comparison)
//! - Each token can be scoped to specific projects
//! - Tokens can be revoked or expired

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use sqlx::FromRow;

use crate::{error::Error, AppState};

/// Authentication context injected into request extensions after successful token validation.
#[derive(Clone, Debug)]
pub struct AuthContext {
    /// Unique identifier for the token (for audit logging)
    pub token_id: String,
    /// User ID that owns this token
    pub user_id: String,
    /// Project IDs this token has access to (empty = all projects for this user)
    pub project_ids: Vec<String>,
}

/// Database row for API tokens.
#[derive(Debug, FromRow)]
struct ApiTokenRow {
    id: String,
    user_id: String,
    token_prefix: String,
    token_hash: String,
    project_ids: Option<String>, // JSON array or comma-separated
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Middleware that requires a valid API token.
///
/// Extracts Bearer token from Authorization header, validates it against the database,
/// and injects `AuthContext` into request extensions.
///
/// # Errors
///
/// Returns 401 Unauthorized if:
/// - No Authorization header present
/// - Authorization header is not a Bearer token
/// - Token prefix not found in database
/// - Token hash doesn't match
/// - Token is expired or revoked
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use fold::middleware::require_token;
///
/// let app = Router::new()
///     .route("/api/memories", post(create_memory))
///     .layer(middleware::from_fn_with_state(state.clone(), require_token));
/// ```
pub async fn require_token(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(Error::Unauthenticated)?;

    // Parse Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(Error::Unauthenticated)?;

    // Validate token and get context
    let auth_context = validate_token(&state, token).await?;

    // Update last_used_at (fire and forget - don't block the request)
    let db = state.db.clone();
    let token_id = auth_context.token_id.clone();
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE api_tokens SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&token_id)
            .execute(&db)
            .await;
    });

    // Inject AuthContext into request extensions
    req.extensions_mut().insert(auth_context);

    Ok(next.run(req).await)
}

/// Middleware that requires token access to a specific project.
///
/// Must be used AFTER `require_token` middleware. Checks that the authenticated
/// token has access to the project specified in the path parameter.
///
/// # Path Parameters
///
/// Expects a `project` or `project_id` parameter in the path.
///
/// # Errors
///
/// Returns 403 Forbidden if the token doesn't have access to the project.
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use fold::middleware::{require_token, require_project_access};
///
/// let app = Router::new()
///     .route("/api/projects/:project/memories", post(create_memory))
///     .layer(middleware::from_fn(require_project_access))
///     .layer(middleware::from_fn_with_state(state.clone(), require_token));
/// ```
pub async fn require_project_access(
    Path(params): Path<std::collections::HashMap<String, String>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    // Get the project from path params
    let project_id = params
        .get("project")
        .or_else(|| params.get("project_id"))
        .ok_or_else(|| Error::Internal("Missing project path parameter".into()))?;

    // Get AuthContext from extensions (set by require_token)
    let auth_context = req
        .extensions()
        .get::<AuthContext>()
        .ok_or(Error::Unauthenticated)?;

    // Check project access
    // Empty project_ids means access to all projects (for this user)
    if !auth_context.project_ids.is_empty()
        && !auth_context.project_ids.contains(project_id)
    {
        return Err(Error::Forbidden);
    }

    Ok(next.run(req).await)
}

/// Validate a token string and return the auth context.
async fn validate_token(state: &AppState, token: &str) -> Result<AuthContext, Error> {
    // Token format: fold_{prefix}_{secret}
    // Prefix is first 8 chars after "fold_", used for lookup
    // Full token is hashed and compared

    // Basic format validation
    if !token.starts_with("fold_") {
        return Err(Error::InvalidToken);
    }

    let token_body = &token[5..]; // Skip "fold_"
    if token_body.len() < 9 {
        // At least 8 char prefix + 1 char separator/secret
        return Err(Error::InvalidToken);
    }

    let prefix = &token_body[..8];

    // Hash the full token for comparison
    let token_hash = hash_token(token);

    // Look up token by prefix
    let token_row: Option<ApiTokenRow> = sqlx::query_as(
        r#"
        SELECT id, user_id, token_prefix, token_hash, project_ids, expires_at, revoked_at
        FROM api_tokens
        WHERE token_prefix = ?
        "#,
    )
    .bind(prefix)
    .fetch_optional(&state.db)
    .await?;

    let token_row = token_row.ok_or(Error::InvalidToken)?;

    // Verify hash (timing-safe comparison)
    if !constant_time_eq(&token_hash, &token_row.token_hash) {
        return Err(Error::InvalidToken);
    }

    // Check if revoked
    if token_row.revoked_at.is_some() {
        return Err(Error::InvalidToken);
    }

    // Check if expired
    if let Some(expires_at) = token_row.expires_at {
        if expires_at < chrono::Utc::now() {
            return Err(Error::TokenExpired);
        }
    }

    // Parse project_ids
    let project_ids = token_row
        .project_ids
        .map(|s| parse_project_ids(&s))
        .unwrap_or_default();

    Ok(AuthContext {
        token_id: token_row.id,
        user_id: token_row.user_id,
        project_ids,
    })
}

/// Hash a token using SHA-256.
fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// Parse project IDs from stored format (JSON array or comma-separated).
fn parse_project_ids(s: &str) -> Vec<String> {
    let trimmed = s.trim();

    // Try JSON array first
    if trimmed.starts_with('[') {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(trimmed) {
            return ids;
        }
    }

    // Fall back to comma-separated
    trimmed
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Project access context with role information.
/// Injected into request extensions after project access check.
#[derive(Clone, Debug)]
pub struct ProjectAccess {
    /// Project ID
    pub project_id: String,
    /// User's role on this project: "member" (read/write) or "viewer" (read-only)
    pub role: String,
    /// Whether user can write (create/update/delete)
    pub can_write: bool,
}

/// Middleware that checks if user can access a project and loads their role.
///
/// Must be used AFTER `require_token` middleware. Checks that the authenticated
/// user is a member of the project and injects `ProjectAccess` into extensions.
///
/// This is a more comprehensive check than `require_project_access` as it also
/// verifies membership in the project_members table and loads the user's role.
///
/// # Errors
///
/// Returns 403 Forbidden if the user is not a member of the project.
pub async fn require_project_member(
    State(state): State<AppState>,
    Path(params): Path<std::collections::HashMap<String, String>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    // Get the project from path params
    let project_id_or_slug = params
        .get("project")
        .or_else(|| params.get("project_id"))
        .ok_or_else(|| Error::Internal("Missing project path parameter".into()))?;

    // Get AuthContext from extensions (set by require_token)
    let auth_context = req
        .extensions()
        .get::<AuthContext>()
        .ok_or(Error::Unauthenticated)?
        .clone();

    // Resolve project ID/slug to actual project
    let project = crate::db::get_project_by_id_or_slug(&state.db, project_id_or_slug).await?;

    // Check membership in project_members table
    let member = crate::db::get_project_member(&state.db, &project.id, &auth_context.user_id).await?;

    let project_access = match member {
        Some(m) => ProjectAccess {
            project_id: project.id,
            role: m.role.clone(),
            can_write: m.can_write(),
        },
        None => {
            // User is not a member of this project
            return Err(Error::Forbidden);
        }
    };

    // Inject ProjectAccess into request extensions
    req.extensions_mut().insert(project_access);

    Ok(next.run(req).await)
}

/// Middleware that requires write access to a project.
///
/// Must be used AFTER `require_project_member` middleware.
/// Checks that the user has a "member" role (not "viewer").
///
/// # Errors
///
/// Returns 403 Forbidden if the user only has read access (viewer role).
pub async fn require_project_write(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    // Get ProjectAccess from extensions (set by require_project_member)
    let project_access = req
        .extensions()
        .get::<ProjectAccess>()
        .ok_or_else(|| Error::Internal("require_project_write must be used after require_project_member".into()))?;

    if !project_access.can_write {
        return Err(Error::Forbidden);
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_token() {
        let token = "fold_abc12345_secretpart";
        let hash = hash_token(token);

        // Hash should be consistent
        assert_eq!(hash, hash_token(token));

        // Hash should be 64 hex chars (256 bits)
        assert_eq!(hash.len(), 64);

        // Different tokens should have different hashes
        assert_ne!(hash, hash_token("fold_abc12345_different"));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("hello", "hello"));
        assert!(!constant_time_eq("hello", "world"));
        assert!(!constant_time_eq("hello", "hello!"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn test_parse_project_ids_json() {
        let ids = parse_project_ids(r#"["proj1", "proj2", "proj3"]"#);
        assert_eq!(ids, vec!["proj1", "proj2", "proj3"]);
    }

    #[test]
    fn test_parse_project_ids_csv() {
        let ids = parse_project_ids("proj1, proj2, proj3");
        assert_eq!(ids, vec!["proj1", "proj2", "proj3"]);
    }

    #[test]
    fn test_parse_project_ids_empty() {
        assert!(parse_project_ids("").is_empty());
        assert!(parse_project_ids("[]").is_empty());
    }
}
