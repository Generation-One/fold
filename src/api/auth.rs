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

use axum::{
    extract::{Path, Query, State},
    middleware,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::middleware::require_session;
use crate::{AppState, Error, Result};

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
            post(logout).layer(middleware::from_fn_with_state(state.clone(), require_session)),
        )
        .route(
            "/me",
            get(get_current_user).layer(middleware::from_fn_with_state(state, require_session)),
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
            let issuer = provider_config.issuer.as_deref().unwrap_or("https://gitlab.com");
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
            format!(
                "{}/authorize?client_id={}&redirect_uri={}/auth/callback/{}&response_type=code&scope={}&state={}",
                issuer,
                provider_config.client_id,
                config.server.public_url,
                provider,
                provider_config.scopes.join(" "),
                state
            )
        }
    };

    // TODO: Store state in session for verification on callback

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
    Path(provider): Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response> {
    // Check for OAuth errors
    if let Some(error) = query.error {
        let description = query.error_description.unwrap_or_default();
        return Err(Error::InvalidCredentials);
    }

    let code = query
        .code
        .ok_or_else(|| Error::InvalidInput("Missing authorization code".into()))?;

    let _state_param = query
        .state
        .ok_or_else(|| Error::InvalidInput("Missing state parameter".into()))?;

    // TODO: Verify state matches stored session state

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
    // TODO: Create session and set cookie

    // Redirect to frontend
    let redirect_url = format!("{}/?login=success", config.server.public_url);
    Ok(Redirect::temporary(&redirect_url).into_response())
}

/// End the current session.
///
/// POST /auth/logout
///
/// Clears the session cookie and invalidates the session server-side.
#[axum::debug_handler]
async fn logout(State(_state): State<AppState>) -> Result<impl IntoResponse> {
    // TODO: Clear session from database and cookie

    Ok(Json(serde_json::json!({
        "message": "Logged out successfully"
    })))
}

/// Get current authenticated user information.
///
/// GET /auth/me
///
/// Returns the profile information for the currently authenticated user.
#[axum::debug_handler]
async fn get_current_user(State(_state): State<AppState>) -> Result<Json<UserInfo>> {
    // TODO: Extract user from session/token and fetch full profile

    Err(Error::NotImplemented("get_current_user".into()))
}

/// Bootstrap initial admin user.
///
/// POST /auth/bootstrap
///
/// Creates the initial admin user using a bootstrap token. This endpoint
/// only works when no users exist in the system and a valid bootstrap
/// token is provided.
#[axum::debug_handler]
async fn bootstrap(
    State(_state): State<AppState>,
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

    // TODO: Check if any users exist - only allow bootstrap if empty

    // TODO: Create admin user in database

    // Generate API token
    let api_token = nanoid::nanoid!(48);

    // TODO: Store API token hash in database

    Ok(Json(BootstrapResponse {
        user_id: uuid::Uuid::new_v4().to_string(),
        api_token,
        message: "Admin user created successfully".into(),
    }))
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
    let client = reqwest::Client::new();

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
            format!("{}/token", issuer)
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
        let error_text = response.text().await.unwrap_or_default();
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
    let client = reqwest::Client::new();

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
            (
                format!("{}/userinfo", issuer),
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
                email: user.email.unwrap_or_else(|| format!("{}@github.local", user.login)),
                name: user.name.unwrap_or(user.login),
                avatar_url: user.avatar_url,
            })
        }
        crate::config::AuthProviderType::GitLab => {
            #[derive(Deserialize)]
            struct GitLabUser {
                id: i64,
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
                email: user.email.unwrap_or_else(|| format!("{}@oidc.local", user.sub)),
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
    _state: &AppState,
    _provider: &str,
    _user_info: &OAuthUserInfo,
) -> Result<String> {
    // TODO: Implement user upsert in database
    Ok(uuid::Uuid::new_v4().to_string())
}
