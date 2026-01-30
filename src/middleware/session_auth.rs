//! Session-based authentication middleware.
//!
//! Validates session cookies for web UI access. Used by the admin dashboard
//! and other browser-based interactions.
//!
//! # Session Flow
//!
//! 1. User authenticates via OIDC provider
//! 2. Server creates session and sets `fold_session` cookie
//! 3. Subsequent requests include cookie, validated by this middleware
//! 4. Session expires after configured duration or on logout
//!
//! # Security Model
//!
//! - Session IDs are cryptographically random (nanoid)
//! - Sessions are stored server-side in database
//! - Cookie is HttpOnly, Secure (in production), SameSite=Lax
//! - Sessions can be invalidated server-side (logout, security events)

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
    Extension,
};
use axum_extra::extract::CookieJar;
use sqlx::FromRow;

use crate::{config::config, error::Error, AppState};

/// Session cookie name.
pub const SESSION_COOKIE_NAME: &str = "fold_session";

/// User context injected into request extensions after successful session validation.
#[derive(Clone, Debug)]
pub struct SessionUser {
    /// Unique user identifier
    pub user_id: String,
    /// User's email address (if available)
    pub email: Option<String>,
    /// User's display name
    pub name: Option<String>,
    /// User's role: "admin" or "member"
    pub role: String,
}

impl SessionUser {
    /// Check if user has admin role.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// Database row for sessions.
#[derive(Debug, FromRow)]
struct SessionRow {
    id: String,
    user_id: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// Database row for users.
#[derive(Debug, FromRow)]
struct UserRow {
    id: String,
    email: Option<String>,
    name: Option<String>,
    role: String,
}

/// Middleware that requires a valid session.
///
/// Extracts session ID from cookie, validates it against the database,
/// and injects `SessionUser` into request extensions.
///
/// # Errors
///
/// Returns 401 Unauthorized if:
/// - No session cookie present
/// - Session not found in database
/// - Session is expired
/// - User not found
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use fold::middleware::require_session;
///
/// let app = Router::new()
///     .route("/admin/dashboard", get(dashboard))
///     .layer(middleware::from_fn_with_state(state.clone(), require_session));
/// ```
pub async fn require_session(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    // Extract session ID from cookie
    let session_id = jar
        .get(SESSION_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .ok_or(Error::Unauthenticated)?;

    // Validate session and get user
    let session_user = validate_session(&state, &session_id).await?;

    // Inject SessionUser into request extensions
    req.extensions_mut().insert(session_user);

    Ok(next.run(req).await)
}

/// Middleware that requires admin role.
///
/// Must be used AFTER `require_session` middleware. Checks that the authenticated
/// user has the "admin" role.
///
/// # Errors
///
/// Returns 403 Forbidden if the user is not an admin.
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use fold::middleware::{require_session, require_admin};
///
/// let app = Router::new()
///     .route("/admin/users", get(list_users))
///     .layer(middleware::from_fn(require_admin))
///     .layer(middleware::from_fn_with_state(state.clone(), require_session));
/// ```
pub async fn require_admin(
    Extension(user): Extension<SessionUser>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, Error> {
    if !user.is_admin() {
        return Err(Error::Forbidden);
    }

    Ok(next.run(req).await)
}

/// Validate a session ID and return the session user.
async fn validate_session(state: &AppState, session_id: &str) -> Result<SessionUser, Error> {
    let config = config();

    // Look up session
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

    // Check if expired
    if session.expires_at < chrono::Utc::now() {
        // Clean up expired session
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

    // Look up user
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

    // Optionally extend session if it's more than halfway through its lifetime
    let max_age = chrono::Duration::seconds(config.session.max_age_seconds as i64);
    let halfway = chrono::Utc::now() + (max_age / 2);

    if session.expires_at < halfway {
        let new_expires = chrono::Utc::now() + max_age;
        let db = state.db.clone();
        let sid = session_id.to_string();
        tokio::spawn(async move {
            let _ = sqlx::query("UPDATE sessions SET expires_at = ? WHERE id = ?")
                .bind(new_expires)
                .bind(&sid)
                .execute(&db)
                .await;
        });
    }

    Ok(SessionUser {
        user_id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
    })
}

/// Optional session middleware - doesn't fail if no session, just doesn't inject user.
///
/// Useful for routes that have different behavior for authenticated vs anonymous users.
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware, Extension};
/// use fold::middleware::{optional_session, SessionUser};
///
/// async fn handler(user: Option<Extension<SessionUser>>) -> impl IntoResponse {
///     if let Some(Extension(user)) = user {
///         format!("Hello, {}!", user.name.unwrap_or_default())
///     } else {
///         "Hello, guest!".to_string()
///     }
/// }
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(middleware::from_fn_with_state(state.clone(), optional_session));
/// ```
pub async fn optional_session(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Try to extract and validate session
    if let Some(session_id) = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string()) {
        if let Ok(session_user) = validate_session(&state, &session_id).await {
            req.extensions_mut().insert(session_user);
        }
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_user_is_admin() {
        let admin = SessionUser {
            user_id: "user1".to_string(),
            email: Some("admin@example.com".to_string()),
            name: Some("Admin".to_string()),
            role: "admin".to_string(),
        };

        let member = SessionUser {
            user_id: "user2".to_string(),
            email: Some("member@example.com".to_string()),
            name: Some("Member".to_string()),
            role: "member".to_string(),
        };

        assert!(admin.is_admin());
        assert!(!member.is_admin());
    }
}
