//! Authentication Routes
//!
//! Handles OAuth2/OIDC authentication flows with support for multiple providers.
//!
//! Routes:
//! - GET /auth/providers - List available auth providers
//! - GET /auth/login/:provider - Redirect to provider login
//! - GET /auth/callback/:provider - Handle OAuth callback
//! - POST /auth/logout - End session
//! - GET /auth/me - Get current user info
//! - POST /auth/bootstrap - Create initial admin user with bootstrap token
//! - GET /auth/tokens - List user's API tokens
//! - POST /auth/tokens - Create new API token
//! - DELETE /auth/tokens/:id - Revoke an API token

use axum::{
    extract::{Extension, Path, Query, State},
    middleware,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time;

use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;

use crate::middleware::{require_auth, require_session, AuthUser};
use crate::{AppState, Error, Result};

// ============================================================================
// OIDC Discovery Cache
// ============================================================================

/// Cached OIDC discovery documents
static OIDC_DISCOVERY_CACHE: OnceLock<RwLock<HashMap<String, OidcDiscovery>>> = OnceLock::new();

fn discovery_cache() -> &'static RwLock<HashMap<String, OidcDiscovery>> {
    OIDC_DISCOVERY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// OIDC Discovery document (subset of fields we need)
#[derive(Debug, Clone, Deserialize)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

/// Fetch OIDC discovery document for an issuer (with caching)
async fn get_oidc_discovery(issuer: &str) -> Result<OidcDiscovery> {
    // Check cache first
    {
        let cache = discovery_cache().read().await;
        if let Some(discovery) = cache.get(issuer) {
            return Ok(discovery.clone());
        }
    }

    // Fetch from well-known endpoint
    let discovery_url = format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // Allow self-signed certs for local dev
        .build()
        .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| Error::Internal(format!("Failed to fetch OIDC discovery: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Internal(format!(
            "OIDC discovery failed with status: {}",
            response.status()
        )));
    }

    let discovery: OidcDiscovery = response
        .json()
        .await
        .map_err(|e| Error::Internal(format!("Failed to parse OIDC discovery: {}", e)))?;

    // Cache it
    {
        let mut cache = discovery_cache().write().await;
        cache.insert(issuer.to_string(), discovery.clone());
    }

    Ok(discovery)
}

/// Build authentication routes.
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Public routes
        .route("/providers", get(list_providers))
        .route("/login/:provider", get(login_redirect))
        .route("/callback/:provider", get(oauth_callback))
        .route("/bootstrap", post(bootstrap))
        // Protected routes (require session)
        .route(
            "/logout",
            post(logout).layer(middleware::from_fn_with_state(
                state.clone(),
                require_session,
            )),
        )
        .route(
            "/me",
            get(get_current_user)
                .layer(middleware::from_fn_with_state(state.clone(), require_auth)),
        )
        // API token management (require auth - supports both session and token)
        .route(
            "/tokens",
            get(list_tokens)
                .post(create_token)
                .layer(middleware::from_fn_with_state(state.clone(), require_auth)),
        )
        .route(
            "/tokens/:token_id",
            delete(revoke_token).layer(middleware::from_fn_with_state(state.clone(), require_auth)),
        )
        // Admin token management (admin only)
        .route(
            "/admin/users/:user_id/tokens",
            get(admin_list_user_tokens)
                .layer(middleware::from_fn_with_state(state.clone(), require_auth)),
        )
        .route(
            "/admin/users/:user_id/tokens/:token_id",
            delete(admin_revoke_user_token)
                .layer(middleware::from_fn_with_state(state, require_auth)),
        )
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Auth provider info returned to clients.
#[derive(Debug, Serialize)]
pub struct AuthProviderInfo {
    pub id: String,
    pub display_name: String,
    pub icon: Option<String>,
    #[serde(rename = "type")]
    pub provider_type: String,
}

/// Response containing list of available providers.
#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<AuthProviderInfo>,
}

/// Query params for OAuth callback.
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// Request body for bootstrap endpoint.
#[derive(Debug, Deserialize)]
pub struct BootstrapRequest {
    pub token: String,
    pub email: String,
    pub name: String,
}

/// Response from bootstrap endpoint.
#[derive(Debug, Serialize)]
pub struct BootstrapResponse {
    pub user_id: String,
    pub api_token: String,
    pub message: String,
}

/// Current user information.
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub provider: String,
    pub roles: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Request to create an API token.
#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub scopes: Vec<String>,
    /// Optional expiry in days from now
    pub expires_in_days: Option<i64>,
    /// Optional user_id for admin to create tokens for other users
    pub user_id: Option<String>,
}

/// Response containing a newly created API token.
/// The token value is only returned once at creation time.
#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub id: String,
    pub name: String,
    pub token: String,
    pub token_prefix: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

/// API token info (without the secret).
#[derive(Debug, Serialize)]
pub struct ApiTokenInfo {
    pub id: String,
    pub name: String,
    pub token_prefix: String,
    pub created_at: String,
    pub last_used: Option<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// Response containing list of API tokens.
#[derive(Debug, Serialize)]
pub struct ListTokensResponse {
    pub tokens: Vec<ApiTokenInfo>,
}

/// OAuth state for CSRF protection
#[derive(Debug, sqlx::FromRow)]
struct OAuthState {
    id: String,
    #[allow(dead_code)]
    state: String,
    provider: String,
    #[allow(dead_code)]
    created_at: String,
    expires_at: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// List available authentication providers.
///
/// GET /auth/providers
///
/// Returns all enabled auth providers that can be used for login.
#[axum::debug_handler]
async fn list_providers(State(_state): State<AppState>) -> Result<Json<ProvidersResponse>> {
    let config = crate::config();

    let providers: Vec<AuthProviderInfo> = config
        .auth
        .providers
        .iter()
        .filter(|(_, p)| p.enabled)
        .map(|(_, p)| AuthProviderInfo {
            id: p.id.clone(),
            display_name: p.display_name.clone(),
            icon: p.icon.clone(),
            provider_type: format!("{:?}", p.provider_type).to_lowercase(),
        })
        .collect();

    Ok(Json(ProvidersResponse { providers }))
}

/// Redirect to OAuth provider login page.
///
/// GET /auth/login/:provider
///
/// Initiates the OAuth flow by redirecting the user to the provider's
/// authorization endpoint. Stores state in session for CSRF protection.
#[axum::debug_handler]
async fn login_redirect(
    State(_state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Response> {
    let config = crate::config();

    let provider_config = config
        .auth
        .providers
        .get(&provider)
        .ok_or_else(|| Error::NotFound(format!("Auth provider: {}", provider)))?;

    if !provider_config.enabled {
        return Err(Error::NotFound(format!("Auth provider: {}", provider)));
    }

    // Generate state for CSRF protection
    let state = nanoid::nanoid!(32);

    // Build authorization URL based on provider type
    let auth_url = match provider_config.provider_type {
        crate::config::AuthProviderType::GitHub => {
            format!(
                "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}/auth/callback/{}&scope={}&state={}",
                provider_config.client_id,
                config.server.public_url,
                provider,
                provider_config.scopes.join(" "),
                state
            )
        }
        crate::config::AuthProviderType::GitLab => {
            let issuer = provider_config
                .issuer
                .as_deref()
                .unwrap_or("https://gitlab.com");
            format!(
                "{}/oauth/authorize?client_id={}&redirect_uri={}/auth/callback/{}&response_type=code&scope={}&state={}",
                issuer,
                provider_config.client_id,
                config.server.public_url,
                provider,
                provider_config.scopes.join(" "),
                state
            )
        }
        crate::config::AuthProviderType::Oidc => {
            let issuer = provider_config
                .issuer
                .as_ref()
                .ok_or_else(|| Error::InvalidInput("OIDC provider requires issuer".into()))?;

            // Use OIDC discovery to get the correct authorization endpoint
            let discovery = get_oidc_discovery(issuer).await?;

            format!(
                "{}?client_id={}&redirect_uri={}/auth/callback/{}&response_type=code&scope={}&state={}",
                discovery.authorization_endpoint,
                provider_config.client_id,
                config.server.public_url,
                provider,
                provider_config.scopes.join(" "),
                state
            )
        }
    };

    // Store state in database for CSRF verification
    let state_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires = now + chrono::Duration::minutes(10);

    sqlx::query(
        r#"
        INSERT INTO oauth_states (id, state, provider, created_at, expires_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&state_id)
    .bind(&state)
    .bind(&provider)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .execute(&_state.db)
    .await?;

    Ok(Redirect::temporary(&auth_url).into_response())
}

/// Handle OAuth callback from provider.
///
/// GET /auth/callback/:provider?code=...&state=...
///
/// Exchanges the authorization code for tokens, fetches user info,
/// and creates or updates the user in our database.
#[axum::debug_handler]
async fn oauth_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(provider): Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response> {
    // Check for OAuth errors
    if let Some(_error) = query.error {
        let _description = query.error_description.unwrap_or_default();
        return Err(Error::InvalidCredentials);
    }

    let code = query
        .code
        .ok_or_else(|| Error::InvalidInput("Missing authorization code".into()))?;

    let _state_param = query
        .state
        .ok_or_else(|| Error::InvalidInput("Missing state parameter".into()))?;

    // Verify state from database
    let oauth_state: Option<OAuthState> =
        sqlx::query_as("SELECT * FROM oauth_states WHERE state = ?")
            .bind(&_state_param)
            .fetch_optional(&state.db)
            .await?;

    let oauth_state =
        oauth_state.ok_or_else(|| Error::InvalidInput("Invalid or expired state".into()))?;

    // Check expiry
    let expires = chrono::DateTime::parse_from_rfc3339(&oauth_state.expires_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| Error::Internal("Invalid timestamp".into()))?;

    if expires < Utc::now() {
        sqlx::query("DELETE FROM oauth_states WHERE id = ?")
            .bind(&oauth_state.id)
            .execute(&state.db)
            .await?;
        return Err(Error::InvalidInput("State expired".into()));
    }

    // Verify provider matches
    if oauth_state.provider != provider {
        return Err(Error::InvalidInput("Provider mismatch".into()));
    }

    let config = crate::config();
    let provider_config = config
        .auth
        .providers
        .get(&provider)
        .ok_or_else(|| Error::NotFound(format!("Auth provider: {}", provider)))?;

    // Exchange code for tokens
    let (access_token, _id_token) = exchange_code_for_tokens(
        provider_config,
        &code,
        &format!("{}/auth/callback/{}", config.server.public_url, provider),
    )
    .await?;

    // Fetch user info from provider
    let user_info = fetch_user_info(provider_config, &access_token).await?;

    // Create or update user in database
    let user_id = upsert_user(&state, &provider, &user_info).await?;

    // Create session
    let session_id = nanoid::nanoid!(32);
    let session_expires = Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        r#"
        INSERT INTO sessions (id, user_id, expires_at)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(&session_id)
    .bind(&user_id)
    .bind(session_expires.to_rfc3339())
    .execute(&state.db)
    .await?;

    // Set session cookie
    let cookie = Cookie::build(("fold_session", session_id))
        .path("/")
        .http_only(true)
        .secure(config.server.public_url.starts_with("https"))
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    let jar = jar.add(cookie);

    // Clean up OAuth state
    sqlx::query("DELETE FROM oauth_states WHERE id = ?")
        .bind(&oauth_state.id)
        .execute(&state.db)
        .await?;

    // Redirect to frontend
    let redirect_url = format!("{}/?login=success", config.server.public_url);
    Ok((jar, Redirect::temporary(&redirect_url)).into_response())
}

/// End the current session.
///
/// POST /auth/logout
///
/// Clears the session cookie and invalidates the session server-side.
#[axum::debug_handler]
async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<impl IntoResponse> {
    // Get session ID from cookie
    if let Some(cookie) = jar.get("fold_session") {
        let session_id = cookie.value();

        // Delete from database
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&state.db)
            .await?;
    }

    // Clear cookie
    let cookie = Cookie::build(("fold_session", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    let jar = jar.add(cookie);

    Ok((
        jar,
        Json(serde_json::json!({
            "message": "Logged out successfully"
        })),
    ))
}

/// Get current authenticated user information.
///
/// GET /auth/me
///
/// Returns the profile information for the currently authenticated user.
/// Accepts authentication via either session cookie or Bearer token.
#[axum::debug_handler]
async fn get_current_user(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<UserInfo>> {
    // Fetch full user details from database
    let db_user: (String, Option<String>, Option<String>, String) = sqlx::query_as(
        r#"
        SELECT email, display_name, avatar_url, provider
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(&user.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(UserInfo {
        id: user.user_id.clone(),
        email: db_user.0,
        name: db_user
            .1
            .unwrap_or_else(|| user.name.clone().unwrap_or_default()),
        avatar_url: db_user.2,
        provider: db_user.3,
        roles: vec![user.role.clone()],
        created_at: Utc::now(),
    }))
}

/// Bootstrap initial admin user.
///
/// POST /auth/bootstrap
///
/// Creates the initial admin user using a bootstrap token. This endpoint
/// only works when no admin users exist in the system and a valid bootstrap
/// token is provided.
#[axum::debug_handler]
async fn bootstrap(
    State(state): State<AppState>,
    Json(request): Json<BootstrapRequest>,
) -> Result<Json<BootstrapResponse>> {
    let config = crate::config();

    // Verify bootstrap token
    let expected_token = config
        .auth
        .bootstrap_token
        .as_ref()
        .ok_or_else(|| Error::Forbidden)?;

    if request.token != *expected_token {
        return Err(Error::InvalidCredentials);
    }

    // Check if any admin users exist - only allow bootstrap if no admins
    let admin_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&state.db)
        .await?;

    if admin_count.0 > 0 {
        return Err(Error::InvalidInput(
            "Bootstrap not allowed: admin user already exists".into(),
        ));
    }

    // Create admin user in database
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO users (id, provider, subject, email, display_name, role, created_at)
        VALUES (?, 'bootstrap', ?, ?, ?, 'admin', datetime('now'))
        "#,
    )
    .bind(&user_id)
    .bind(&request.email) // Use email as subject for bootstrap users
    .bind(&request.email)
    .bind(&request.name)
    .execute(&state.db)
    .await?;

    // Generate API token in format: fold_{prefix}_{secret}
    let prefix = nanoid::nanoid!(8);
    let secret = nanoid::nanoid!(32);
    let api_token = format!("fold_{}_{}", prefix, secret);

    // Hash the full token for storage
    let mut hasher = Sha256::new();
    hasher.update(api_token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    // Store API token in database
    let token_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO api_tokens (id, user_id, name, token_hash, token_prefix, project_ids, created_at)
        VALUES (?, ?, 'Bootstrap Token', ?, ?, '[]', datetime('now'))
        "#,
    )
    .bind(&token_id)
    .bind(&user_id)
    .bind(&token_hash)
    .bind(&prefix)
    .execute(&state.db)
    .await?;

    // Ensure admin group exists
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO groups (id, name, description, is_system, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("group_admin")
    .bind("Admins")
    .bind("System administrators with global access")
    .bind(1)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Add user to admin group
    sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, created_at)
        VALUES (?, ?, ?)
        "#,
    )
    .bind("group_admin")
    .bind(&user_id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    Ok(Json(BootstrapResponse {
        user_id,
        api_token,
        message: "Admin user created successfully".into(),
    }))
}

// ============================================================================
// API Token Management Handlers
// ============================================================================

/// List all API tokens for the authenticated user.
///
/// GET /auth/tokens
///
/// Returns all tokens (active, expired, and revoked) for the current user.
/// Token secrets are never returned - only metadata and prefix for identification.
#[axum::debug_handler]
async fn list_tokens(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<ListTokensResponse>> {
    let tokens: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT id, name, token_prefix, created_at, last_used, expires_at, revoked_at
        FROM api_tokens
        WHERE user_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    let tokens = tokens
        .into_iter()
        .map(
            |(id, name, token_prefix, created_at, last_used, expires_at, revoked_at)| {
                ApiTokenInfo {
                    id,
                    name,
                    token_prefix,
                    created_at,
                    last_used,
                    expires_at,
                    revoked_at,
                }
            },
        )
        .collect();

    Ok(Json(ListTokensResponse { tokens }))
}

/// Create a new API token for the authenticated user or another user (admin only).
///
/// POST /auth/tokens
///
/// Creates a new API token with the specified name and optional expiry.
/// The full token value is returned only once in the response.
///
/// If user_id is provided, only admins can create tokens for other users.
#[axum::debug_handler]
async fn create_token(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(request): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>> {
    // Validate name
    let name = request.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Token name cannot be empty".into()));
    }
    if name.len() > 100 {
        return Err(Error::InvalidInput(
            "Token name too long (max 100 characters)".into(),
        ));
    }

    // Determine which user to create the token for
    eprintln!(
        "DEBUG create_token: request.user_id = {:?}, authenticated user = {}, is_admin = {}",
        request.user_id,
        user.user_id,
        user.is_admin()
    );
    let target_user_id = if let Some(user_id) = request.user_id {
        eprintln!("DEBUG: Creating token for specified user: {}", user_id);
        // Only admins can create tokens for other users
        if user_id != user.user_id && !user.is_admin() {
            return Err(Error::Forbidden);
        }
        user_id
    } else {
        eprintln!(
            "DEBUG: No user_id provided, using authenticated user: {}",
            user.user_id
        );
        user.user_id.clone()
    };
    eprintln!("DEBUG: Final target_user_id = {}", target_user_id);

    // Generate API token in format: fold_{prefix}_{secret}
    let prefix = nanoid::nanoid!(8);
    let secret = nanoid::nanoid!(32);
    let api_token = format!("fold_{}_{}", prefix, secret);

    // Hash the full token for storage
    let mut hasher = Sha256::new();
    hasher.update(api_token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    // Calculate expiry if specified
    let expires_at = request.expires_in_days.map(|days| {
        (Utc::now() + chrono::Duration::days(days))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    });

    let created_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let token_id = uuid::Uuid::new_v4().to_string();

    // Store token in database
    sqlx::query(
        r#"
        INSERT INTO api_tokens (id, user_id, name, token_hash, token_prefix, project_ids, created_at, expires_at)
        VALUES (?, ?, ?, ?, ?, '[]', ?, ?)
        "#,
    )
    .bind(&token_id)
    .bind(&target_user_id)
    .bind(name)
    .bind(&token_hash)
    .bind(&prefix)
    .bind(&created_at)
    .bind(&expires_at)
    .execute(&state.db)
    .await?;

    Ok(Json(CreateTokenResponse {
        id: token_id,
        name: name.to_string(),
        token: api_token,
        token_prefix: prefix,
        created_at,
        expires_at,
    }))
}

/// Revoke an API token.
///
/// DELETE /auth/tokens/:token_id
///
/// Marks the token as revoked. Revoked tokens cannot be used for authentication.
#[axum::debug_handler]
async fn revoke_token(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(token_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // Verify the token belongs to the user
    let token_exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM api_tokens WHERE id = ? AND user_id = ?")
            .bind(&token_id)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    if token_exists.is_none() {
        return Err(Error::NotFound(format!("Token {}", token_id)));
    }

    // Mark as revoked
    let revoked_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query("UPDATE api_tokens SET revoked_at = ? WHERE id = ?")
        .bind(&revoked_at)
        .bind(&token_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Token revoked successfully",
        "token_id": token_id
    })))
}

/// List API tokens for any user (admin only).
#[axum::debug_handler]
async fn admin_list_user_tokens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<String>,
) -> Result<Json<ListTokensResponse>> {
    // Only admins can list tokens for other users
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    let result: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT id, name, token_prefix, created_at, last_used, expires_at, revoked_at
        FROM api_tokens
        WHERE user_id = ? AND revoked_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await?;

    let tokens = result
        .into_iter()
        .map(
            |(id, name, token_prefix, created_at, last_used, expires_at, revoked_at)| {
                ApiTokenInfo {
                    id,
                    name,
                    token_prefix,
                    created_at,
                    last_used,
                    expires_at,
                    revoked_at,
                }
            },
        )
        .collect();

    Ok(Json(ListTokensResponse { tokens }))
}

/// Revoke an API token for any user (admin only).
#[axum::debug_handler]
async fn admin_revoke_user_token(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((user_id, token_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    // Only admins can revoke tokens for other users
    if !auth.is_admin() {
        return Err(Error::Forbidden);
    }

    // Verify the token belongs to the user
    let token_user: Option<(String,)> =
        sqlx::query_as("SELECT user_id FROM api_tokens WHERE id = ?")
            .bind(&token_id)
            .fetch_optional(&state.db)
            .await?;

    match token_user {
        None => return Err(Error::NotFound("Token not found".to_string())),
        Some((token_user_id,)) if token_user_id != user_id => {
            return Err(Error::InvalidInput(
                "Token does not belong to this user".to_string(),
            ))
        }
        _ => {}
    }

    // Mark as revoked
    let revoked_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query("UPDATE api_tokens SET revoked_at = ? WHERE id = ?")
        .bind(&revoked_at)
        .bind(&token_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Token revoked successfully",
        "token_id": token_id
    })))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Exchange authorization code for access/ID tokens.
async fn exchange_code_for_tokens(
    provider: &crate::config::AuthProvider,
    code: &str,
    redirect_uri: &str,
) -> Result<(String, Option<String>)> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // Allow self-signed certs for local dev
        .build()
        .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let token_url = match provider.provider_type {
        crate::config::AuthProviderType::GitHub => {
            "https://github.com/login/oauth/access_token".to_string()
        }
        crate::config::AuthProviderType::GitLab => {
            let issuer = provider.issuer.as_deref().unwrap_or("https://gitlab.com");
            format!("{}/oauth/token", issuer)
        }
        crate::config::AuthProviderType::Oidc => {
            let issuer = provider
                .issuer
                .as_ref()
                .ok_or_else(|| Error::InvalidInput("OIDC provider requires issuer".into()))?;

            // Use OIDC discovery to get the correct token endpoint
            let discovery = get_oidc_discovery(issuer).await?;
            discovery.token_endpoint
        }
    };

    let response = client
        .post(&token_url)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", provider.client_id.as_str()),
            ("client_secret", provider.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let _error_text = response.text().await.unwrap_or_default();
        return Err(Error::InvalidCredentials);
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        id_token: Option<String>,
    }

    let tokens: TokenResponse = response.json().await?;
    Ok((tokens.access_token, tokens.id_token))
}

/// Fetch user information from the OAuth provider.
async fn fetch_user_info(
    provider: &crate::config::AuthProvider,
    access_token: &str,
) -> Result<OAuthUserInfo> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // Allow self-signed certs for local dev
        .build()
        .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let (url, auth_header) = match provider.provider_type {
        crate::config::AuthProviderType::GitHub => (
            "https://api.github.com/user".to_string(),
            format!("token {}", access_token),
        ),
        crate::config::AuthProviderType::GitLab => {
            let issuer = provider.issuer.as_deref().unwrap_or("https://gitlab.com");
            (
                format!("{}/api/v4/user", issuer),
                format!("Bearer {}", access_token),
            )
        }
        crate::config::AuthProviderType::Oidc => {
            let issuer = provider
                .issuer
                .as_ref()
                .ok_or_else(|| Error::InvalidInput("OIDC provider requires issuer".into()))?;

            // Use OIDC discovery to get the correct userinfo endpoint
            let discovery = get_oidc_discovery(issuer).await?;
            (
                discovery.userinfo_endpoint,
                format!("Bearer {}", access_token),
            )
        }
    };

    let response = client
        .get(&url)
        .header("Authorization", auth_header)
        .header("User-Agent", "Fold/1.0")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(Error::Internal("Failed to fetch user info".into()));
    }

    // Parse response based on provider type
    match provider.provider_type {
        crate::config::AuthProviderType::GitHub => {
            #[derive(Deserialize)]
            struct GitHubUser {
                id: i64,
                login: String,
                email: Option<String>,
                name: Option<String>,
                avatar_url: Option<String>,
            }

            let user: GitHubUser = response.json().await?;
            Ok(OAuthUserInfo {
                provider_id: user.id.to_string(),
                email: user
                    .email
                    .unwrap_or_else(|| format!("{}@github.local", user.login)),
                name: user.name.unwrap_or(user.login),
                avatar_url: user.avatar_url,
            })
        }
        crate::config::AuthProviderType::GitLab => {
            #[derive(Deserialize)]
            struct GitLabUser {
                id: i64,
                #[allow(dead_code)]
                username: String,
                email: String,
                name: String,
                avatar_url: Option<String>,
            }

            let user: GitLabUser = response.json().await?;
            Ok(OAuthUserInfo {
                provider_id: user.id.to_string(),
                email: user.email,
                name: user.name,
                avatar_url: user.avatar_url,
            })
        }
        crate::config::AuthProviderType::Oidc => {
            #[derive(Deserialize)]
            struct OidcUserInfo {
                sub: String,
                email: Option<String>,
                name: Option<String>,
                picture: Option<String>,
            }

            let user: OidcUserInfo = response.json().await?;
            Ok(OAuthUserInfo {
                provider_id: user.sub.clone(),
                email: user
                    .email
                    .unwrap_or_else(|| format!("{}@oidc.local", user.sub)),
                name: user.name.unwrap_or_else(|| user.sub.clone()),
                avatar_url: user.picture,
            })
        }
    }
}

/// User info extracted from OAuth provider.
struct OAuthUserInfo {
    provider_id: String,
    email: String,
    name: String,
    avatar_url: Option<String>,
}

/// Create or update user in database.
async fn upsert_user(
    state: &AppState,
    provider: &str,
    user_info: &OAuthUserInfo,
) -> Result<String> {
    // Check if user exists by provider + subject
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE provider = ? AND subject = ?")
            .bind(provider)
            .bind(&user_info.provider_id)
            .fetch_optional(&state.db)
            .await?;

    if let Some((user_id,)) = existing {
        // Update existing user
        sqlx::query(
            r#"
            UPDATE users
            SET email = ?, display_name = ?, avatar_url = ?, last_login = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(&user_info.email)
        .bind(&user_info.name)
        .bind(&user_info.avatar_url)
        .bind(&user_id)
        .execute(&state.db)
        .await?;

        return Ok(user_id);
    }

    // Create new user
    let user_id = uuid::Uuid::new_v4().to_string();
    let role = "member"; // Default role

    sqlx::query(
        r#"
        INSERT INTO users (id, provider, subject, email, display_name, avatar_url, role, created_at, last_login)
        VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
        "#,
    )
    .bind(&user_id)
    .bind(provider)
    .bind(&user_info.provider_id)
    .bind(&user_info.email)
    .bind(&user_info.name)
    .bind(&user_info.avatar_url)
    .bind(role)
    .execute(&state.db)
    .await?;

    Ok(user_id)
}
