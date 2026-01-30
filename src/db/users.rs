//! User, session, and API token database queries.
//!
//! Handles OIDC user management, web sessions, and API token authentication.

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// User Types
// ============================================================================

/// User role enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    #[default]
    Member,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "admin" => Self::Admin,
            _ => Self::Member,
        }
    }
}

/// User record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub provider: String,
    pub subject: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub created_at: String,
    pub last_login: Option<String>,
}

impl User {
    pub fn role_enum(&self) -> UserRole {
        UserRole::from_str(&self.role)
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// Input for creating a new user.
#[derive(Debug, Clone)]
pub struct CreateUser {
    pub id: String,
    pub provider: String,
    pub subject: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: UserRole,
}

/// Input for updating a user.
#[derive(Debug, Clone, Default)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<UserRole>,
}

// ============================================================================
// Session Types
// ============================================================================

/// Web session record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub expires_at: String,
    pub created_at: String,
}

impl Session {
    pub fn is_expired(&self) -> bool {
        if let Ok(expires) = DateTime::parse_from_rfc3339(&self.expires_at) {
            expires < Utc::now()
        } else {
            true // If we can't parse, treat as expired
        }
    }
}

/// Input for creating a session.
#[derive(Debug, Clone)]
pub struct CreateSession {
    pub id: String,
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
}

// ============================================================================
// API Token Types
// ============================================================================

/// API token record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub project_ids: String, // JSON array
    pub created_at: String,
    pub last_used: Option<String>,
    pub expires_at: Option<String>,
}

impl ApiToken {
    /// Parse project_ids JSON into a vector.
    pub fn project_ids_vec(&self) -> Vec<String> {
        serde_json::from_str(&self.project_ids).unwrap_or_default()
    }

    /// Check if token grants access to a project.
    pub fn has_project_access(&self, project_id: &str) -> bool {
        let ids = self.project_ids_vec();
        ids.is_empty() || ids.contains(&project_id.to_string())
    }

    pub fn is_expired(&self) -> bool {
        if let Some(ref expires) = self.expires_at {
            if let Ok(dt) = DateTime::parse_from_rfc3339(expires) {
                return dt < Utc::now();
            }
        }
        false
    }
}

/// Input for creating an API token.
#[derive(Debug, Clone)]
pub struct CreateApiToken {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub project_ids: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ============================================================================
// User Queries
// ============================================================================

/// Create a new user.
pub async fn create_user(pool: &DbPool, input: CreateUser) -> Result<User> {
    sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, provider, subject, email, display_name, avatar_url, role)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.provider)
    .bind(&input.subject)
    .bind(&input.email)
    .bind(&input.display_name)
    .bind(&input.avatar_url)
    .bind(input.role.as_str())
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("User with provider {} and subject {} already exists", input.provider, input.subject))
        }
        _ => Error::Database(e),
    })
}

/// Get a user by ID.
pub async fn get_user(pool: &DbPool, id: &str) -> Result<User> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("User not found: {}", id)))
}

/// Get a user by provider and subject (OIDC lookup).
pub async fn get_user_by_oidc(pool: &DbPool, provider: &str, subject: &str) -> Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT * FROM users
        WHERE provider = ? AND subject = ?
        "#,
    )
    .bind(provider)
    .bind(subject)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Get a user by email.
/// Uses idx_users_email index.
pub async fn get_user_by_email(pool: &DbPool, email: &str) -> Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT * FROM users
        WHERE email = ?
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Update a user.
pub async fn update_user(pool: &DbPool, id: &str, input: UpdateUser) -> Result<User> {
    // Build dynamic update query
    let mut updates = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(email) = input.email {
        updates.push("email = ?");
        bindings.push(email);
    }
    if let Some(display_name) = input.display_name {
        updates.push("display_name = ?");
        bindings.push(display_name);
    }
    if let Some(avatar_url) = input.avatar_url {
        updates.push("avatar_url = ?");
        bindings.push(avatar_url);
    }
    if let Some(role) = input.role {
        updates.push("role = ?");
        bindings.push(role.as_str().to_string());
    }

    if updates.is_empty() {
        return get_user(pool, id).await;
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE users SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, User>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("User not found: {}", id)))
}

/// Delete a user and cascade to sessions/tokens.
pub async fn delete_user(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("User not found: {}", id)));
    }

    Ok(())
}

/// Update user's last login timestamp.
pub async fn update_last_login(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("UPDATE users SET last_login = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List all users with optional role filter.
pub async fn list_users(pool: &DbPool, role: Option<UserRole>) -> Result<Vec<User>> {
    match role {
        Some(r) => {
            sqlx::query_as::<_, User>(
                "SELECT * FROM users WHERE role = ? ORDER BY created_at DESC",
            )
            .bind(r.as_str())
            .fetch_all(pool)
            .await
            .map_err(Error::Database)
        }
        None => {
            sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY created_at DESC")
                .fetch_all(pool)
                .await
                .map_err(Error::Database)
        }
    }
}

/// Find or create a user from OIDC login.
pub async fn find_or_create_user(pool: &DbPool, input: CreateUser) -> Result<User> {
    // First try to find existing user
    if let Some(user) = get_user_by_oidc(pool, &input.provider, &input.subject).await? {
        // Update last login
        update_last_login(pool, &user.id).await?;
        return Ok(user);
    }

    // Create new user
    create_user(pool, input).await
}

// ============================================================================
// Session Queries
// ============================================================================

/// Create a new session.
pub async fn create_session(pool: &DbPool, input: CreateSession) -> Result<Session> {
    sqlx::query_as::<_, Session>(
        r#"
        INSERT INTO sessions (id, user_id, expires_at)
        VALUES (?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.user_id)
    .bind(input.expires_at.to_rfc3339())
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get a session by ID.
/// Uses primary key index.
pub async fn get_session(pool: &DbPool, id: &str) -> Result<Option<Session>> {
    sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get a valid (non-expired) session by ID.
pub async fn get_valid_session(pool: &DbPool, id: &str) -> Result<Option<Session>> {
    sqlx::query_as::<_, Session>(
        r#"
        SELECT * FROM sessions
        WHERE id = ? AND expires_at > datetime('now')
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Get session with associated user.
pub async fn get_session_with_user(pool: &DbPool, session_id: &str) -> Result<Option<(Session, User)>> {
    let session = match get_valid_session(pool, session_id).await? {
        Some(s) => s,
        None => return Ok(None),
    };

    let user = get_user(pool, &session.user_id).await?;
    Ok(Some((session, user)))
}

/// Delete a session.
pub async fn delete_session(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete all sessions for a user.
/// Uses idx_sessions_user index.
pub async fn delete_user_sessions(pool: &DbPool, user_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Delete expired sessions.
/// Uses idx_sessions_expires index.
pub async fn cleanup_expired_sessions(pool: &DbPool) -> Result<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < datetime('now')")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// List sessions for a user.
/// Uses idx_sessions_user index.
pub async fn list_user_sessions(pool: &DbPool, user_id: &str) -> Result<Vec<Session>> {
    sqlx::query_as::<_, Session>(
        r#"
        SELECT * FROM sessions
        WHERE user_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// API Token Queries
// ============================================================================

/// Create a new API token.
pub async fn create_api_token(pool: &DbPool, input: CreateApiToken) -> Result<ApiToken> {
    let project_ids_json = serde_json::to_string(&input.project_ids)?;
    let expires_at = input.expires_at.map(|dt| dt.to_rfc3339());

    sqlx::query_as::<_, ApiToken>(
        r#"
        INSERT INTO api_tokens (id, user_id, name, token_hash, token_prefix, project_ids, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.user_id)
    .bind(&input.name)
    .bind(&input.token_hash)
    .bind(&input.token_prefix)
    .bind(&project_ids_json)
    .bind(&expires_at)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get an API token by ID.
pub async fn get_api_token(pool: &DbPool, id: &str) -> Result<Option<ApiToken>> {
    sqlx::query_as::<_, ApiToken>("SELECT * FROM api_tokens WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get an API token by token hash.
pub async fn get_api_token_by_hash(pool: &DbPool, token_hash: &str) -> Result<Option<ApiToken>> {
    sqlx::query_as::<_, ApiToken>(
        r#"
        SELECT * FROM api_tokens
        WHERE token_hash = ?
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Get API tokens by prefix (for faster lookup).
/// Uses idx_api_tokens_prefix index.
pub async fn get_api_tokens_by_prefix(pool: &DbPool, prefix: &str) -> Result<Vec<ApiToken>> {
    sqlx::query_as::<_, ApiToken>(
        r#"
        SELECT * FROM api_tokens
        WHERE token_prefix = ?
        "#,
    )
    .bind(prefix)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Get a valid (non-expired) API token by hash.
pub async fn get_valid_api_token_by_hash(pool: &DbPool, token_hash: &str) -> Result<Option<ApiToken>> {
    sqlx::query_as::<_, ApiToken>(
        r#"
        SELECT * FROM api_tokens
        WHERE token_hash = ?
        AND (expires_at IS NULL OR expires_at > datetime('now'))
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Update token's last_used timestamp.
pub async fn update_api_token_last_used(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("UPDATE api_tokens SET last_used = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete an API token.
pub async fn delete_api_token(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM api_tokens WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("API token not found: {}", id)));
    }

    Ok(())
}

/// List all API tokens for a user.
/// Uses idx_api_tokens_user index.
pub async fn list_user_api_tokens(pool: &DbPool, user_id: &str) -> Result<Vec<ApiToken>> {
    sqlx::query_as::<_, ApiToken>(
        r#"
        SELECT * FROM api_tokens
        WHERE user_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Delete expired API tokens.
pub async fn cleanup_expired_api_tokens(pool: &DbPool) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM api_tokens WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Verify an API token and get associated user.
/// This is the main authentication flow for API requests.
pub async fn verify_api_token(pool: &DbPool, token_hash: &str) -> Result<Option<(ApiToken, User)>> {
    let token = match get_valid_api_token_by_hash(pool, token_hash).await? {
        Some(t) => t,
        None => return Ok(None),
    };

    // Update last used
    update_api_token_last_used(pool, &token.id).await?;

    // Get associated user
    let user = get_user(pool, &token.user_id).await?;

    Ok(Some((token, user)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_pool, migrate};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_create_and_get_user() {
        let pool = setup_test_db().await;

        let input = CreateUser {
            id: "user-1".to_string(),
            provider: "google".to_string(),
            subject: "sub-123".to_string(),
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            avatar_url: None,
            role: UserRole::Member,
        };

        let user = create_user(&pool, input).await.unwrap();
        assert_eq!(user.id, "user-1");
        assert_eq!(user.email, Some("test@example.com".to_string()));

        let fetched = get_user(&pool, "user-1").await.unwrap();
        assert_eq!(fetched.id, user.id);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let pool = setup_test_db().await;

        // Create user first
        let user = create_user(&pool, CreateUser {
            id: "user-1".to_string(),
            provider: "google".to_string(),
            subject: "sub-123".to_string(),
            email: None,
            display_name: None,
            avatar_url: None,
            role: UserRole::Member,
        }).await.unwrap();

        // Create session
        let session = create_session(&pool, CreateSession {
            id: "session-1".to_string(),
            user_id: user.id.clone(),
            expires_at: Utc::now() + chrono::Duration::hours(24),
        }).await.unwrap();

        assert_eq!(session.user_id, user.id);

        // Verify session exists and is valid
        let (sess, usr) = get_session_with_user(&pool, "session-1").await.unwrap().unwrap();
        assert_eq!(sess.id, "session-1");
        assert_eq!(usr.id, user.id);
    }
}
