//! Middleware for Fold.
//!
//! Provides authentication and authorization middleware:
//! - `token_auth` - API token validation for programmatic access (MCP, CLI, webhooks)
//! - `session_auth` - Session/cookie validation for web UI access
//! - `project_auth` - Project-level access control based on user/group membership

mod project_auth;
mod session_auth;
mod token_auth;

pub use project_auth::{
    require_project_read, require_project_write, require_admin, ProjectAccessContext, ProjectIdParams,
};
pub use session_auth::{
    require_session, SessionUser, SESSION_COOKIE_NAME,
};
pub use token_auth::{
    require_token,
    AuthContext,
};

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::CookieJar;
use crate::{error::Error, AppState};

/// User context that supports both session and token authentication.
///
/// This can be created from either a SessionUser (from session middleware)
/// or from an AuthContext (from token middleware), allowing endpoints to
/// accept either authentication method.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: String,
}

impl From<SessionUser> for AuthUser {
    fn from(user: SessionUser) -> Self {
        AuthUser {
            user_id: user.user_id,
            email: user.email,
            name: user.name,
            role: user.role,
        }
    }
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// Middleware that requires authentication via either session or token.
///
/// Tries session authentication first (for web UI), then falls back to
/// token authentication (for API/programmatic access). Injects `AuthUser`
/// into request extensions.
///
/// # Errors
///
/// Returns 401 Unauthorized if neither session nor token authentication succeeds.
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use fold::middleware::require_auth;
///
/// let app = Router::new()
///     .route("/auth/me", get(get_user))
///     .layer(middleware::from_fn_with_state(state.clone(), require_auth));
/// ```
pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    // Try session authentication first
    if let Some(session_id) = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string()) {
        if let Ok(session_user) = validate_session_internal(&state, &session_id).await {
            let auth_user: AuthUser = session_user.into();
            req.extensions_mut().insert(auth_user);
            return Ok(next.run(req).await);
        }
    }

    // Fall back to token authentication
    if let Some(auth_header) = req.headers().get(axum::http::header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                if let Ok(auth_context) = validate_token_internal(&state, token).await {
                    // Convert AuthContext to AuthUser
                    let user: Option<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
                        r#"
                        SELECT id, email, display_name, role
                        FROM users
                        WHERE id = ?
                        "#,
                    )
                    .bind(&auth_context.user_id)
                    .fetch_optional(&state.db)
                    .await?;

                    if let Some((user_id, email, name, role)) = user {
                        let auth_user = AuthUser {
                            user_id,
                            email,
                            name,
                            role, // Use user's actual role (supports admin API tokens)
                        };
                        req.extensions_mut().insert(auth_user);
                        return Ok(next.run(req).await);
                    }
                }
            }
        }
    }

    Err(Error::Unauthenticated)
}

// Internal helper functions (copy of validation logic)

async fn validate_session_internal(
    state: &AppState,
    session_id: &str,
) -> Result<SessionUser, Error> {
    let _config = crate::config::config();

    #[derive(sqlx::FromRow)]
    struct SessionRow {
        id: String,
        user_id: String,
        expires_at: chrono::DateTime<chrono::Utc>,
    }

    #[derive(sqlx::FromRow)]
    struct UserRow {
        id: String,
        email: Option<String>,
        name: Option<String>,
        role: String,
    }

    let session: Option<SessionRow> = sqlx::query_as(
        r#"
        SELECT id, user_id, expires_at
        FROM sessions
        WHERE id = ?
        "#,
    )
    .bind(session_id)
    .fetch_optional(&state.db)
    .await?;

    let session = session.ok_or(Error::Unauthenticated)?;

    if session.expires_at < chrono::Utc::now() {
        let db = state.db.clone();
        let sid = session_id.to_string();
        tokio::spawn(async move {
            let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
                .bind(&sid)
                .execute(&db)
                .await;
        });
        return Err(Error::Unauthenticated);
    }

    let user: Option<UserRow> = sqlx::query_as(
        r#"
        SELECT id, email, name, role
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&session.user_id)
    .fetch_optional(&state.db)
    .await?;

    let user = user.ok_or(Error::Unauthenticated)?;

    Ok(SessionUser {
        user_id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
    })
}

async fn validate_token_internal(
    state: &AppState,
    token: &str,
) -> Result<AuthContext, Error> {
    use sha2::{Digest, Sha256};
    use sqlx::FromRow;

    #[derive(Debug, FromRow)]
    struct ApiTokenRow {
        id: String,
        user_id: String,
        token_prefix: String,
        token_hash: String,
        project_ids: Option<String>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    if !token.starts_with("fold_") {
        return Err(Error::InvalidToken);
    }

    let token_body = &token[5..];
    if token_body.len() < 9 {
        return Err(Error::InvalidToken);
    }

    let prefix = &token_body[..8];

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

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

    // Constant-time comparison
    if !constant_time_eq(&token_hash, &token_row.token_hash) {
        return Err(Error::InvalidToken);
    }

    if token_row.revoked_at.is_some() {
        return Err(Error::InvalidToken);
    }

    if let Some(expires_at) = token_row.expires_at {
        if expires_at < chrono::Utc::now() {
            return Err(Error::TokenExpired);
        }
    }

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

fn parse_project_ids(s: &str) -> Vec<String> {
    let trimmed = s.trim();

    if trimmed.starts_with('[') {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(trimmed) {
            return ids;
        }
    }

    trimmed
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
