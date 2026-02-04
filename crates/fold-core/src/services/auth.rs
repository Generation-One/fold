//! Auth service for OIDC flows and session management.
//!
//! Handles authentication via:
//! - OIDC providers (Google, Auth0, etc.)
//! - GitHub OAuth
//! - GitLab OAuth
//! - API tokens

use std::time::Duration;

use chrono::{DateTime, Utc};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::config::{AuthConfig, AuthProvider, AuthProviderType};
use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{ApiToken, OidcState, User, UserSession};

/// Session duration
const SESSION_DURATION_DAYS: i64 = 7;

/// OIDC state duration
const STATE_DURATION_MINUTES: i64 = 10;

/// Service for authentication and authorization.
#[derive(Clone)]
pub struct AuthService {
    db: DbPool,
    config: AuthConfig,
    http_client: Client,
}

/// OIDC/OAuth authorization URL result
#[derive(Debug, Clone, Serialize)]
pub struct AuthorizationUrl {
    pub url: String,
    pub state: String,
    pub provider: String,
}

/// Token exchange result
#[derive(Debug, Clone, Serialize)]
pub struct TokenResult {
    pub user: User,
    pub session_token: String,
    pub expires_at: DateTime<Utc>,
}

/// User info from OIDC provider
#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    pub picture: Option<String>,
}

impl AuthService {
    /// Create a new auth service.
    pub fn new(db: DbPool, config: AuthConfig) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            db,
            config,
            http_client,
        }
    }

    /// Get available auth providers.
    pub fn providers(&self) -> Vec<ProviderInfo> {
        self.config
            .providers
            .iter()
            .filter(|(_, p)| p.enabled)
            .map(|(_, p)| ProviderInfo {
                id: p.id.clone(),
                name: p.display_name.clone(),
                icon: p.icon.clone(),
            })
            .collect()
    }

    /// Get provider by ID.
    fn get_provider(&self, provider_id: &str) -> Option<&AuthProvider> {
        self.config.providers.get(provider_id)
    }

    /// Generate authorization URL for a provider.
    pub async fn get_authorization_url(
        &self,
        provider_id: &str,
        redirect_uri: &str,
    ) -> Result<AuthorizationUrl> {
        let provider = self
            .get_provider(provider_id)
            .ok_or_else(|| Error::NotFound(format!("Provider {}", provider_id)))?;

        if !provider.enabled {
            return Err(Error::Forbidden);
        }

        // Generate PKCE verifier
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Generate state and nonce
        let state = CsrfToken::new_random();
        let nonce = nanoid::nanoid!(32);

        // Build OAuth client
        let client = self.build_oauth_client(provider, redirect_uri)?;

        // Build authorization URL
        let mut auth_request = client
            .authorize_url(|| state.clone())
            .set_pkce_challenge(pkce_challenge);

        for scope in &provider.scopes {
            auth_request = auth_request.add_scope(Scope::new(scope.clone()));
        }

        let (auth_url, _) = auth_request.url();

        // Store state for verification
        let oidc_state = OidcState {
            id: crate::models::new_id(),
            state: state.secret().clone(),
            nonce,
            pkce_verifier: Some(pkce_verifier.secret().clone()),
            provider: provider_id.to_string(),
            redirect_uri: Some(redirect_uri.to_string()),
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(STATE_DURATION_MINUTES),
        };

        self.store_oidc_state(&oidc_state).await?;

        Ok(AuthorizationUrl {
            url: auth_url.to_string(),
            state: oidc_state.state.clone(),
            provider: provider_id.to_string(),
        })
    }

    /// Exchange authorization code for tokens and create session.
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<TokenResult> {
        // Retrieve and validate state
        let oidc_state = self.get_oidc_state(state).await?;

        if oidc_state.is_expired() {
            self.delete_oidc_state(&oidc_state.id).await?;
            return Err(Error::TokenExpired);
        }

        let provider = self
            .get_provider(&oidc_state.provider)
            .ok_or_else(|| Error::NotFound(format!("Provider {}", oidc_state.provider)))?;

        let redirect_uri = oidc_state
            .redirect_uri
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("Missing redirect URI".to_string()))?;

        // Build OAuth client
        let client = self.build_oauth_client(provider, redirect_uri)?;

        // Exchange code for tokens
        let mut token_request = client.exchange_code(AuthorizationCode::new(code.to_string()));

        if let Some(ref verifier) = oidc_state.pkce_verifier {
            token_request =
                token_request.set_pkce_verifier(PkceCodeVerifier::new(verifier.clone()));
        }

        let token_response = token_request
            .request_async(async_http_client)
            .await
            .map_err(|_e| Error::InvalidToken)?;

        let access_token = token_response.access_token().secret();

        // Get user info
        let user_info = self.get_user_info(provider, access_token).await?;

        // Create or update user
        let user = self.upsert_user(&oidc_state.provider, &user_info).await?;

        // Create session
        let session_token = nanoid::nanoid!(64);
        let session = self.create_session(&user.id, &session_token).await?;

        // Clean up state
        self.delete_oidc_state(&oidc_state.id).await?;

        info!(
            user_id = %user.id,
            provider = %oidc_state.provider,
            "User authenticated"
        );

        Ok(TokenResult {
            user,
            session_token,
            expires_at: session.expires_at,
        })
    }

    /// Build OAuth client for a provider.
    fn build_oauth_client(
        &self,
        provider: &AuthProvider,
        redirect_uri: &str,
    ) -> Result<BasicClient> {
        let (auth_url, token_url) = match provider.provider_type {
            AuthProviderType::GitHub => (
                "https://github.com/login/oauth/authorize".to_string(),
                "https://github.com/login/oauth/access_token".to_string(),
            ),
            AuthProviderType::GitLab => (
                "https://gitlab.com/oauth/authorize".to_string(),
                "https://gitlab.com/oauth/token".to_string(),
            ),
            AuthProviderType::Oidc => {
                let issuer = provider
                    .issuer
                    .as_ref()
                    .ok_or_else(|| Error::Validation("OIDC provider missing issuer".to_string()))?;
                (
                    format!("{}/authorize", issuer),
                    format!("{}/oauth/token", issuer),
                )
            }
        };

        let client = BasicClient::new(
            ClientId::new(provider.client_id.clone()),
            Some(ClientSecret::new(provider.client_secret.clone())),
            AuthUrl::new(auth_url)
                .map_err(|e| Error::Internal(format!("Invalid auth URL: {}", e)))?,
            Some(
                TokenUrl::new(token_url)
                    .map_err(|e| Error::Internal(format!("Invalid token URL: {}", e)))?,
            ),
        )
        .set_redirect_uri(
            RedirectUrl::new(redirect_uri.to_string())
                .map_err(|e| Error::Internal(format!("Invalid redirect URL: {}", e)))?,
        );

        Ok(client)
    }

    /// Get user info from provider.
    async fn get_user_info(&self, provider: &AuthProvider, access_token: &str) -> Result<UserInfo> {
        let userinfo_url = match provider.provider_type {
            AuthProviderType::GitHub => "https://api.github.com/user",
            AuthProviderType::GitLab => "https://gitlab.com/api/v4/user",
            AuthProviderType::Oidc => {
                let issuer = provider
                    .issuer
                    .as_ref()
                    .ok_or_else(|| Error::Validation("OIDC provider missing issuer".to_string()))?;
                // Would need to fetch from .well-known/openid-configuration
                // For now, construct common pattern
                return self.get_oidc_user_info(issuer, access_token).await;
            }
        };

        let response = self
            .http_client
            .get(userinfo_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "Fold/1.0")
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Failed to get user info: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Internal("Failed to get user info".to_string()));
        }

        // Parse based on provider
        match provider.provider_type {
            AuthProviderType::GitHub => {
                #[derive(Deserialize)]
                struct GitHubUser {
                    id: i64,
                    login: String,
                    email: Option<String>,
                    name: Option<String>,
                    avatar_url: Option<String>,
                }

                let gh_user: GitHubUser = response
                    .json()
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to parse user info: {}", e)))?;

                Ok(UserInfo {
                    sub: gh_user.id.to_string(),
                    email: gh_user.email,
                    name: gh_user.name,
                    preferred_username: Some(gh_user.login),
                    picture: gh_user.avatar_url,
                })
            }
            AuthProviderType::GitLab => {
                #[derive(Deserialize)]
                struct GitLabUser {
                    id: i64,
                    username: String,
                    email: Option<String>,
                    name: Option<String>,
                    avatar_url: Option<String>,
                }

                let gl_user: GitLabUser = response
                    .json()
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to parse user info: {}", e)))?;

                Ok(UserInfo {
                    sub: gl_user.id.to_string(),
                    email: gl_user.email,
                    name: gl_user.name,
                    preferred_username: Some(gl_user.username),
                    picture: gl_user.avatar_url,
                })
            }
            _ => response
                .json()
                .await
                .map_err(|e| Error::Internal(format!("Failed to parse user info: {}", e))),
        }
    }

    /// Get user info from OIDC provider.
    async fn get_oidc_user_info(&self, issuer: &str, access_token: &str) -> Result<UserInfo> {
        let userinfo_url = format!("{}/userinfo", issuer.trim_end_matches('/'));

        let response = self
            .http_client
            .get(&userinfo_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Failed to get user info: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Internal("Failed to get user info".to_string()));
        }

        response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse user info: {}", e)))
    }

    /// Create or update user from provider info.
    async fn upsert_user(&self, provider: &str, info: &UserInfo) -> Result<User> {
        // Check if user exists
        let existing: Option<User> = sqlx::query_as(
            r#"
            SELECT * FROM users WHERE provider = ? AND provider_id = ?
            "#,
        )
        .bind(provider)
        .bind(&info.sub)
        .fetch_optional(&self.db)
        .await?;

        if let Some(mut user) = existing {
            // Update user info
            user.email = info.email.clone();
            user.display_name = info.name.clone();
            user.avatar_url = info.picture.clone();
            user.updated_at = Utc::now();

            sqlx::query(
                r#"
                UPDATE users
                SET email = ?, display_name = ?, avatar_url = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(&user.avatar_url)
            .bind(user.updated_at)
            .bind(&user.id)
            .execute(&self.db)
            .await?;

            return Ok(user);
        }

        // Create new user
        let username = info
            .preferred_username
            .clone()
            .or_else(|| {
                info.email
                    .as_ref()
                    .map(|e| e.split('@').next().unwrap_or("user").to_string())
            })
            .unwrap_or_else(|| format!("user_{}", &info.sub[..8.min(info.sub.len())]));

        let now = Utc::now();
        let user = User {
            id: crate::models::new_id(),
            username,
            email: info.email.clone(),
            display_name: info.name.clone(),
            avatar_url: info.picture.clone(),
            role: "member".to_string(),
            provider: Some(provider.to_string()),
            provider_id: Some(info.sub.clone()),
            default_project: None,
            last_active: Some(now),
            created_at: now,
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO users (
                id, username, email, display_name, avatar_url, role,
                provider, provider_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.display_name)
        .bind(&user.avatar_url)
        .bind(&user.role)
        .bind(&user.provider)
        .bind(&user.provider_id)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.db)
        .await?;

        info!(user_id = %user.id, username = %user.username, "Created new user");

        Ok(user)
    }

    /// Create a new session.
    async fn create_session(&self, user_id: &str, token: &str) -> Result<UserSession> {
        let token_hash = self.hash_token(token);
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(SESSION_DURATION_DAYS);

        let session = UserSession {
            id: crate::models::new_id(),
            user_id: user_id.to_string(),
            token_hash,
            user_agent: None,
            ip_address: None,
            created_at: now,
            expires_at,
            last_used: now,
        };

        sqlx::query(
            r#"
            INSERT INTO user_sessions (
                id, user_id, token_hash, created_at, expires_at, last_used
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.user_id)
        .bind(&session.token_hash)
        .bind(session.created_at)
        .bind(session.expires_at)
        .bind(session.last_used)
        .execute(&self.db)
        .await?;

        Ok(session)
    }

    /// Validate a session token and return the user.
    pub async fn validate_session(&self, token: &str) -> Result<User> {
        let token_hash = self.hash_token(token);

        let session: UserSession = sqlx::query_as(
            r#"
            SELECT * FROM user_sessions WHERE token_hash = ?
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&self.db)
        .await?
        .ok_or(Error::Unauthenticated)?;

        if session.is_expired() {
            self.delete_session(&session.id).await?;
            return Err(Error::TokenExpired);
        }

        // Update last used
        sqlx::query(r#"UPDATE user_sessions SET last_used = datetime('now') WHERE id = ?"#)
            .bind(&session.id)
            .execute(&self.db)
            .await?;

        // Get user
        let user: User = sqlx::query_as(r#"SELECT * FROM users WHERE id = ?"#)
            .bind(&session.user_id)
            .fetch_one(&self.db)
            .await?;

        Ok(user)
    }

    /// Validate an API token and return the user.
    pub async fn validate_api_token(&self, token: &str) -> Result<User> {
        let token_hash = self.hash_token(token);

        let api_token: ApiToken = sqlx::query_as(
            r#"
            SELECT * FROM api_tokens WHERE token_hash = ?
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&self.db)
        .await?
        .ok_or(Error::Unauthenticated)?;

        if api_token.is_expired() {
            return Err(Error::TokenExpired);
        }

        // Update last used
        sqlx::query(r#"UPDATE api_tokens SET last_used = datetime('now') WHERE id = ?"#)
            .bind(&api_token.id)
            .execute(&self.db)
            .await?;

        // Get user
        let user: User = sqlx::query_as(r#"SELECT * FROM users WHERE id = ?"#)
            .bind(&api_token.user_id)
            .fetch_one(&self.db)
            .await?;

        Ok(user)
    }

    /// Create an API token for a user.
    pub async fn create_api_token(
        &self,
        user_id: &str,
        name: &str,
        scopes: Vec<String>,
        expires_in_days: Option<i64>,
    ) -> Result<(ApiToken, String)> {
        let token = format!("fold_{}", nanoid::nanoid!(48));
        let token_hash = self.hash_token(&token);
        let now = Utc::now();
        let expires_at = expires_in_days.map(|d| now + chrono::Duration::days(d));

        let api_token = ApiToken {
            id: crate::models::new_id(),
            user_id: user_id.to_string(),
            name: name.to_string(),
            token_hash,
            scopes: Some(serde_json::to_string(&scopes).unwrap()),
            last_used: None,
            expires_at,
            created_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO api_tokens (
                id, user_id, name, token_hash, scopes, expires_at, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&api_token.id)
        .bind(&api_token.user_id)
        .bind(&api_token.name)
        .bind(&api_token.token_hash)
        .bind(&api_token.scopes)
        .bind(api_token.expires_at)
        .bind(api_token.created_at)
        .execute(&self.db)
        .await?;

        Ok((api_token, token))
    }

    /// Logout (delete session).
    pub async fn logout(&self, token: &str) -> Result<()> {
        let token_hash = self.hash_token(token);

        sqlx::query(r#"DELETE FROM user_sessions WHERE token_hash = ?"#)
            .bind(&token_hash)
            .execute(&self.db)
            .await?;

        Ok(())
    }

    /// Hash a token for storage.
    fn hash_token(&self, token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Store OIDC state.
    async fn store_oidc_state(&self, state: &OidcState) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO oidc_states (
                id, state, nonce, pkce_verifier, provider, redirect_uri,
                created_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&state.id)
        .bind(&state.state)
        .bind(&state.nonce)
        .bind(&state.pkce_verifier)
        .bind(&state.provider)
        .bind(&state.redirect_uri)
        .bind(state.created_at)
        .bind(state.expires_at)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Get OIDC state by state value.
    async fn get_oidc_state(&self, state: &str) -> Result<OidcState> {
        sqlx::query_as::<_, OidcState>(r#"SELECT * FROM oidc_states WHERE state = ?"#)
            .bind(state)
            .fetch_optional(&self.db)
            .await?
            .ok_or(Error::InvalidToken)
    }

    /// Delete OIDC state.
    async fn delete_oidc_state(&self, id: &str) -> Result<()> {
        sqlx::query(r#"DELETE FROM oidc_states WHERE id = ?"#)
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Delete session.
    async fn delete_session(&self, id: &str) -> Result<()> {
        sqlx::query(r#"DELETE FROM user_sessions WHERE id = ?"#)
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Clean up expired states and sessions.
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let states_deleted =
            sqlx::query(r#"DELETE FROM oidc_states WHERE expires_at < datetime('now')"#)
                .execute(&self.db)
                .await?
                .rows_affected();

        let sessions_deleted =
            sqlx::query(r#"DELETE FROM user_sessions WHERE expires_at < datetime('now')"#)
                .execute(&self.db)
                .await?
                .rows_affected();

        Ok((states_deleted + sessions_deleted) as usize)
    }
}

/// Provider info for UI
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}
