//! Provider Configuration Routes
//!
//! CRUD operations for LLM and embedding providers in the Fold system.
//! Also handles OAuth authentication flows for providers that support it.
//!
//! Routes:
//! - GET /providers/llm - List all LLM providers
//! - POST /providers/llm - Create LLM provider
//! - GET /providers/llm/:id - Get LLM provider details
//! - PUT /providers/llm/:id - Update LLM provider
//! - DELETE /providers/llm/:id - Delete LLM provider
//! - POST /providers/llm/:id/test - Test LLM provider connection
//!
//! - GET /providers/embedding - List all embedding providers
//! - POST /providers/embedding - Create embedding provider
//! - GET /providers/embedding/:id - Get embedding provider details
//! - PUT /providers/embedding/:id - Update embedding provider
//! - DELETE /providers/embedding/:id - Delete embedding provider
//! - POST /providers/embedding/:id/test - Test embedding provider connection
//!
//! OAuth routes:
//! - GET /providers/:type/:name/oauth/authorize - Start OAuth flow
//! - GET /providers/oauth/callback - OAuth callback handler

use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::db::{
    create_llm_provider, create_embedding_provider, create_provider_oauth_state,
    delete_llm_provider, delete_embedding_provider, delete_provider_oauth_state,
    get_llm_provider, get_llm_provider_by_name, get_embedding_provider,
    get_embedding_provider_by_name, get_valid_provider_oauth_state,
    list_llm_providers, list_embedding_providers, seed_claudecode_provider_async,
    update_llm_provider, update_embedding_provider, CreateLlmProvider,
    CreateEmbeddingProvider, CreateProviderOAuthState, LlmProviderRow,
    UpdateLlmProvider, UpdateEmbeddingProvider,
};
use crate::services::{ClaudeCodeInfo, ClaudeCodeService};
use crate::{AppState, Error, Result};

// ============================================================================
// Anthropic OAuth Constants
// ============================================================================

/// Anthropic OAuth client ID (public, same for all implementations)
const ANTHROPIC_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// Anthropic OAuth redirect URI
const ANTHROPIC_REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

/// Anthropic OAuth authorization endpoint (Claude Max/Pro)
const ANTHROPIC_AUTH_URL_MAX: &str = "https://claude.ai/oauth/authorize";

/// Anthropic OAuth authorization endpoint (Console/API key creation)
const ANTHROPIC_AUTH_URL_CONSOLE: &str = "https://console.anthropic.com/oauth/authorize";

/// Anthropic OAuth token endpoint
const ANTHROPIC_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";

/// OAuth scopes for Anthropic
const ANTHROPIC_SCOPES: &[&str] = &["org:create_api_key", "user:profile", "user:inference"];

// ============================================================================
// Routes
// ============================================================================

/// Build provider management routes (requires authentication).
pub fn routes() -> Router<AppState> {
    Router::new()
        // LLM provider routes
        .route("/llm", get(list_llm).post(create_llm))
        .route(
            "/llm/:id",
            get(get_llm).put(update_llm).delete(delete_llm),
        )
        .route("/llm/:id/test", post(test_llm))
        // Claude Code token management
        .route("/llm/claudecode/status", get(claudecode_status))
        .route("/llm/claudecode/import", post(import_claudecode_token))
        .route("/llm/claudecode/auto-import", post(auto_import_claudecode_token))
        // Embedding provider routes
        .route("/embedding", get(list_embedding).post(create_embedding))
        .route(
            "/embedding/:id",
            get(get_embedding).put(update_embedding).delete(delete_embedding),
        )
        .route("/embedding/:id/test", post(test_embedding))
}

/// Build OAuth routes (public, no authentication required).
/// These routes initiate OAuth flows and handle callbacks.
pub fn oauth_routes() -> Router<AppState> {
    Router::new()
        .route("/:provider_type/:provider_name/oauth/authorize", get(oauth_authorize))
        .route("/oauth/callback", get(oauth_callback))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create a new LLM provider.
#[derive(Debug, Deserialize)]
pub struct CreateLlmProviderRequest {
    /// Provider name: gemini, openai, anthropic, openrouter
    pub name: String,
    /// Whether the provider is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Priority (lower = higher priority)
    #[serde(default)]
    pub priority: i32,
    /// Authentication type: api_key or oauth
    #[serde(default = "default_api_key")]
    pub auth_type: String,
    /// API key (for api_key auth type)
    pub api_key: Option<String>,
    /// Provider-specific configuration
    #[serde(default = "default_config")]
    pub config: JsonValue,
}

/// Request to update an LLM provider.
#[derive(Debug, Deserialize)]
pub struct UpdateLlmProviderRequest {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub api_key: Option<String>,
    pub config: Option<JsonValue>,
}

/// Request to create a new embedding provider.
#[derive(Debug, Deserialize)]
pub struct CreateEmbeddingProviderRequest {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_api_key")]
    pub auth_type: String,
    pub api_key: Option<String>,
    #[serde(default = "default_config")]
    pub config: JsonValue,
}

/// Request to update an embedding provider.
#[derive(Debug, Deserialize)]
pub struct UpdateEmbeddingProviderRequest {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub api_key: Option<String>,
    pub config: Option<JsonValue>,
}

/// Request to import a Claude Code OAuth token.
/// Users can paste their token from `claude code setup-token` or ~/.claude/.credentials.json
#[derive(Debug, Deserialize)]
pub struct ImportClaudeCodeTokenRequest {
    /// The OAuth access token from Claude Code
    pub access_token: String,
    /// Optional refresh token
    pub refresh_token: Option<String>,
    /// Optional subscription type (max, pro)
    pub subscription_type: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_api_key() -> String {
    "api_key".to_string()
}

fn default_config() -> JsonValue {
    json!({})
}

/// LLM provider response (excludes sensitive fields).
#[derive(Debug, Serialize)]
pub struct LlmProviderResponse {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub auth_type: String,
    pub has_api_key: bool,
    pub has_oauth_token: bool,
    pub oauth_token_expired: bool,
    pub config: JsonValue,
    // Usage stats
    pub request_count: i64,
    pub token_count: i64,
    pub error_count: i64,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    // Metadata
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

/// Embedding provider response (excludes sensitive fields).
#[derive(Debug, Serialize)]
pub struct EmbeddingProviderResponse {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub auth_type: String,
    pub has_api_key: bool,
    pub has_oauth_token: bool,
    pub oauth_token_expired: bool,
    pub config: JsonValue,
    // Usage stats
    pub request_count: i64,
    pub token_count: i64,
    pub error_count: i64,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    // Metadata
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

/// Provider test result with detailed diagnostics.
#[derive(Debug, Serialize)]
pub struct ProviderTestResponse {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
    pub model: Option<String>,
    pub response_preview: Option<String>,
    pub error_code: Option<String>,
    pub error_details: Option<String>,
    pub usage: Option<ProviderTestUsage>,
}

/// Usage info from test request.
#[derive(Debug, Serialize)]
pub struct ProviderTestUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

/// OAuth authorization query parameters.
#[derive(Debug, Deserialize)]
pub struct OAuthAuthorizeQuery {
    /// OAuth mode: "max" (Claude Pro/Max) or "console" (API key creation)
    #[serde(default = "default_oauth_mode")]
    pub mode: String,
    /// Redirect URI after OAuth completes (for local callback server)
    pub redirect_uri: Option<String>,
}

fn default_oauth_mode() -> String {
    "max".to_string()
}

/// OAuth callback query parameters.
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    /// Authorization code (Anthropic uses code#state format)
    pub code: Option<String>,
    /// State token for CSRF verification
    pub state: Option<String>,
    /// Error from provider
    pub error: Option<String>,
    pub error_description: Option<String>,
}

// ============================================================================
// LLM Provider Handlers
// ============================================================================

/// List all LLM providers.
#[axum::debug_handler]
async fn list_llm(State(state): State<AppState>) -> Result<Json<Vec<LlmProviderResponse>>> {
    let providers = list_llm_providers(&state.db).await?;

    let responses: Vec<LlmProviderResponse> = providers
        .into_iter()
        .map(|p| {
            // Capture borrowed values before moving
            let enabled = p.is_enabled();
            let has_api_key = p.api_key.is_some();
            let has_oauth_token = p.oauth_access_token.is_some();
            let oauth_token_expired = p.is_oauth_token_expired();
            let config_json = serde_json::from_str(&p.config).unwrap_or(json!({}));
            LlmProviderResponse {
                id: p.id,
                name: p.name,
                enabled,
                priority: p.priority,
                auth_type: p.auth_type,
                has_api_key,
                has_oauth_token,
                oauth_token_expired,
                config: config_json,
                request_count: p.request_count,
                token_count: p.token_count,
                error_count: p.error_count,
                last_error: p.last_error,
                last_error_at: p.last_error_at,
                created_at: p.created_at,
                updated_at: p.updated_at,
                last_used_at: p.last_used_at,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// Create a new LLM provider.
#[axum::debug_handler]
async fn create_llm(
    State(state): State<AppState>,
    Json(req): Json<CreateLlmProviderRequest>,
) -> Result<Json<LlmProviderResponse>> {
    // Validate provider name
    let valid_names = ["gemini", "openai", "anthropic", "openrouter", "claudecode"];
    if !valid_names.contains(&req.name.as_str()) {
        return Err(Error::Validation(format!(
            "Invalid provider name '{}'. Must be one of: {:?}",
            req.name, valid_names
        )));
    }

    let provider = create_llm_provider(
        &state.db,
        CreateLlmProvider {
            name: req.name,
            enabled: req.enabled,
            priority: req.priority,
            auth_type: req.auth_type,
            api_key: req.api_key,
            config: req.config,
        },
    )
    .await?;

    info!(provider_id = %provider.id, name = %provider.name, "Created LLM provider");

    // Capture borrowed values before moving
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    Ok(Json(LlmProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }))
}

/// Get an LLM provider by ID.
#[axum::debug_handler]
async fn get_llm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LlmProviderResponse>> {
    let provider = get_llm_provider(&state.db, &id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("LLM provider not found: {}", id)))?;

    // Capture borrowed values before moving
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    Ok(Json(LlmProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }))
}

/// Update an LLM provider.
#[axum::debug_handler]
async fn update_llm(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLlmProviderRequest>,
) -> Result<Json<LlmProviderResponse>> {
    let provider = update_llm_provider(
        &state.db,
        &id,
        UpdateLlmProvider {
            enabled: req.enabled,
            priority: req.priority,
            api_key: req.api_key,
            config: req.config,
            ..Default::default()
        },
    )
    .await?;

    info!(provider_id = %provider.id, "Updated LLM provider");

    // Capture borrowed values before moving
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    Ok(Json(LlmProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }))
}

/// Delete an LLM provider.
#[axum::debug_handler]
async fn delete_llm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JsonValue>> {
    delete_llm_provider(&state.db, &id).await?;
    info!(provider_id = %id, "Deleted LLM provider");
    Ok(Json(json!({ "deleted": true })))
}

/// Test an LLM provider connection by making a real API call.
#[axum::debug_handler]
async fn test_llm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderTestResponse>> {
    let provider = get_llm_provider(&state.db, &id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("LLM provider not found: {}", id)))?;

    // Check if provider has credentials
    let api_key = provider.api_key.as_ref().or(provider.oauth_access_token.as_ref());
    if api_key.is_none() {
        return Ok(Json(ProviderTestResponse {
            success: false,
            message: "No credentials configured".to_string(),
            latency_ms: None,
            model: None,
            response_preview: None,
            error_code: Some("NO_CREDENTIALS".to_string()),
            error_details: Some("Provider has no API key or OAuth token configured".to_string()),
            usage: None,
        }));
    }
    let api_key = api_key.unwrap();

    // Get model with default fallback
    let model = provider.model().unwrap_or_else(|| default_model_for_provider(&provider.name));

    let client = reqwest::Client::new();
    let start = Instant::now();

    // Test based on provider type
    let result = match provider.name.as_str() {
        "gemini" => test_gemini_llm(&client, api_key, &model).await,
        "openai" => test_openai_llm(&client, api_key, &model).await,
        "anthropic" => test_anthropic_llm(&client, api_key, &model).await,
        "claudecode" => test_claudecode_llm(&client, api_key, &model).await,
        "openrouter" => test_openrouter_llm(&client, api_key, &model).await,
        _ => Err(("UNSUPPORTED_PROVIDER".to_string(), format!("Unknown provider: {}", provider.name))),
    };

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((response_preview, usage)) => {
            Ok(Json(ProviderTestResponse {
                success: true,
                message: format!("Provider '{}' is working correctly", provider.name),
                latency_ms: Some(latency_ms),
                model: Some(model),
                response_preview: Some(response_preview),
                error_code: None,
                error_details: None,
                usage: Some(usage),
            }))
        }
        Err((error_code, error_details)) => {
            Ok(Json(ProviderTestResponse {
                success: false,
                message: format!("Provider '{}' test failed", provider.name),
                latency_ms: Some(latency_ms),
                model: Some(model),
                response_preview: None,
                error_code: Some(error_code),
                error_details: Some(error_details),
                usage: None,
            }))
        }
    }
}

/// Default models for each provider.
fn default_model_for_provider(provider: &str) -> String {
    match provider {
        "gemini" => "gemini-1.5-flash".to_string(),
        "openai" => "gpt-4o-mini".to_string(),
        "anthropic" => "claude-3-5-sonnet-20241022".to_string(),
        "claudecode" => "claude-3-5-haiku-20241022".to_string(),
        "openrouter" => "openai/gpt-4o-mini".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Default embedding models for each provider.
fn default_embedding_model_for_provider(provider: &str) -> String {
    match provider {
        "gemini" => "embedding-001".to_string(),
        "openai" => "text-embedding-3-small".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Default embedding dimensions for each provider.
fn default_dimension_for_provider(provider: &str) -> usize {
    match provider {
        "gemini" => 768,
        "openai" => 1536,
        _ => 768,
    }
}

/// Test Gemini LLM API.
async fn test_gemini_llm(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
) -> std::result::Result<(String, ProviderTestUsage), (String, String)> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let body = json!({
        "contents": [{
            "parts": [{"text": "Say 'Hello' and nothing else."}]
        }],
        "generationConfig": {
            "maxOutputTokens": 10
        }
    });

    let response = client.post(&url).json(&body).send().await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let text = json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let usage = ProviderTestUsage {
        input_tokens: json["usageMetadata"]["promptTokenCount"].as_i64(),
        output_tokens: json["usageMetadata"]["candidatesTokenCount"].as_i64(),
        total_tokens: json["usageMetadata"]["totalTokenCount"].as_i64(),
    };

    Ok((text, usage))
}

/// Test OpenAI LLM API.
async fn test_openai_llm(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
) -> std::result::Result<(String, ProviderTestUsage), (String, String)> {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "Say 'Hello' and nothing else."}],
        "max_tokens": 10
    });

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let usage = ProviderTestUsage {
        input_tokens: json["usage"]["prompt_tokens"].as_i64(),
        output_tokens: json["usage"]["completion_tokens"].as_i64(),
        total_tokens: json["usage"]["total_tokens"].as_i64(),
    };

    Ok((text, usage))
}

/// Test Anthropic LLM API.
async fn test_anthropic_llm(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
) -> std::result::Result<(String, ProviderTestUsage), (String, String)> {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "Say 'Hello' and nothing else."}],
        "max_tokens": 10
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let usage = ProviderTestUsage {
        input_tokens: json["usage"]["input_tokens"].as_i64(),
        output_tokens: json["usage"]["output_tokens"].as_i64(),
        total_tokens: Some(
            json["usage"]["input_tokens"].as_i64().unwrap_or(0) +
            json["usage"]["output_tokens"].as_i64().unwrap_or(0)
        ),
    };

    Ok((text, usage))
}

/// Test Claude Code LLM API (uses OAuth Bearer token with special headers).
async fn test_claudecode_llm(
    client: &reqwest::Client,
    oauth_token: &str,
    model: &str,
) -> std::result::Result<(String, ProviderTestUsage), (String, String)> {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "Say 'Hello' and nothing else."}],
        "max_tokens": 10
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Authorization", format!("Bearer {}", oauth_token))
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "oauth-2025-04-20, claude-code-20250219, interleaved-thinking-2025-05-14")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let usage = ProviderTestUsage {
        input_tokens: json["usage"]["input_tokens"].as_i64(),
        output_tokens: json["usage"]["output_tokens"].as_i64(),
        total_tokens: Some(
            json["usage"]["input_tokens"].as_i64().unwrap_or(0) +
            json["usage"]["output_tokens"].as_i64().unwrap_or(0)
        ),
    };

    Ok((text, usage))
}

/// Test OpenRouter LLM API (uses OpenAI-compatible format).
async fn test_openrouter_llm(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
) -> std::result::Result<(String, ProviderTestUsage), (String, String)> {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "Say 'Hello' and nothing else."}],
        "max_tokens": 10
    });

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("HTTP-Referer", "https://fold.dev")
        .json(&body)
        .send()
        .await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let usage = ProviderTestUsage {
        input_tokens: json["usage"]["prompt_tokens"].as_i64(),
        output_tokens: json["usage"]["completion_tokens"].as_i64(),
        total_tokens: json["usage"]["total_tokens"].as_i64(),
    };

    Ok((text, usage))
}

// ============================================================================
// Embedding Provider Handlers
// ============================================================================

/// List all embedding providers.
#[axum::debug_handler]
async fn list_embedding(
    State(state): State<AppState>,
) -> Result<Json<Vec<EmbeddingProviderResponse>>> {
    let providers = list_embedding_providers(&state.db).await?;

    let responses: Vec<EmbeddingProviderResponse> = providers
        .into_iter()
        .map(|p| {
            // Capture borrowed values before moving
            let enabled = p.is_enabled();
            let has_api_key = p.api_key.is_some();
            let has_oauth_token = p.oauth_access_token.is_some();
            let oauth_token_expired = p.is_oauth_token_expired();
            let config_json = serde_json::from_str(&p.config).unwrap_or(json!({}));
            EmbeddingProviderResponse {
                id: p.id,
                name: p.name,
                enabled,
                priority: p.priority,
                auth_type: p.auth_type,
                has_api_key,
                has_oauth_token,
                oauth_token_expired,
                config: config_json,
                request_count: p.request_count,
                token_count: p.token_count,
                error_count: p.error_count,
                last_error: p.last_error,
                last_error_at: p.last_error_at,
                created_at: p.created_at,
                updated_at: p.updated_at,
                last_used_at: p.last_used_at,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// Create a new embedding provider.
#[axum::debug_handler]
async fn create_embedding(
    State(state): State<AppState>,
    Json(req): Json<CreateEmbeddingProviderRequest>,
) -> Result<Json<EmbeddingProviderResponse>> {
    // Validate provider name
    let valid_names = ["gemini", "openai"];
    if !valid_names.contains(&req.name.as_str()) {
        return Err(Error::Validation(format!(
            "Invalid embedding provider name '{}'. Must be one of: {:?}",
            req.name, valid_names
        )));
    }

    let provider = create_embedding_provider(
        &state.db,
        CreateEmbeddingProvider {
            name: req.name,
            enabled: req.enabled,
            priority: req.priority,
            auth_type: req.auth_type,
            api_key: req.api_key,
            config: req.config,
        },
    )
    .await?;

    info!(provider_id = %provider.id, name = %provider.name, "Created embedding provider");

    // Capture borrowed values before moving
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    Ok(Json(EmbeddingProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }))
}

/// Get an embedding provider by ID.
#[axum::debug_handler]
async fn get_embedding(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EmbeddingProviderResponse>> {
    let provider = get_embedding_provider(&state.db, &id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Embedding provider not found: {}", id)))?;

    // Capture borrowed values before moving
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    Ok(Json(EmbeddingProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }))
}

/// Update an embedding provider.
#[axum::debug_handler]
async fn update_embedding(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateEmbeddingProviderRequest>,
) -> Result<Json<EmbeddingProviderResponse>> {
    let provider = update_embedding_provider(
        &state.db,
        &id,
        UpdateEmbeddingProvider {
            enabled: req.enabled,
            priority: req.priority,
            api_key: req.api_key,
            config: req.config,
            ..Default::default()
        },
    )
    .await?;

    info!(provider_id = %provider.id, "Updated embedding provider");

    // Capture borrowed values before moving
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    Ok(Json(EmbeddingProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }))
}

/// Delete an embedding provider.
#[axum::debug_handler]
async fn delete_embedding(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JsonValue>> {
    delete_embedding_provider(&state.db, &id).await?;
    info!(provider_id = %id, "Deleted embedding provider");
    Ok(Json(json!({ "deleted": true })))
}

/// Test an embedding provider connection by making a real API call.
#[axum::debug_handler]
async fn test_embedding(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderTestResponse>> {
    let provider = get_embedding_provider(&state.db, &id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Embedding provider not found: {}", id)))?;

    // Check if provider has credentials
    let api_key = provider.api_key.as_ref().or(provider.oauth_access_token.as_ref());
    if api_key.is_none() {
        return Ok(Json(ProviderTestResponse {
            success: false,
            message: "No credentials configured".to_string(),
            latency_ms: None,
            model: None,
            response_preview: None,
            error_code: Some("NO_CREDENTIALS".to_string()),
            error_details: Some("Provider has no API key or OAuth token configured".to_string()),
            usage: None,
        }));
    }
    let api_key = api_key.unwrap();

    // Get model and dimension with defaults
    let model = provider.model().unwrap_or_else(|| default_embedding_model_for_provider(&provider.name));
    let dimension = provider.dimension().unwrap_or_else(|| default_dimension_for_provider(&provider.name));

    let client = reqwest::Client::new();
    let start = Instant::now();

    // Test based on provider type
    let result = match provider.name.as_str() {
        "gemini" => test_gemini_embedding(&client, api_key, &model).await,
        "openai" => test_openai_embedding(&client, api_key, &model).await,
        _ => Err(("UNSUPPORTED_PROVIDER".to_string(), format!("Unknown provider: {}", provider.name))),
    };

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((embedding_dim, usage)) => {
            let dimension_match = embedding_dim == dimension;
            let preview = format!(
                "Embedding dimension: {} (expected: {}){}",
                embedding_dim,
                dimension,
                if dimension_match { " ✓" } else { " ✗ MISMATCH" }
            );

            Ok(Json(ProviderTestResponse {
                success: dimension_match,
                message: if dimension_match {
                    format!("Provider '{}' is working correctly", provider.name)
                } else {
                    format!(
                        "Provider '{}' returned dimension {} but expected {}",
                        provider.name, embedding_dim, dimension
                    )
                },
                latency_ms: Some(latency_ms),
                model: Some(model),
                response_preview: Some(preview),
                error_code: if dimension_match { None } else { Some("DIMENSION_MISMATCH".to_string()) },
                error_details: None,
                usage: Some(usage),
            }))
        }
        Err((error_code, error_details)) => {
            Ok(Json(ProviderTestResponse {
                success: false,
                message: format!("Provider '{}' test failed", provider.name),
                latency_ms: Some(latency_ms),
                model: Some(model),
                response_preview: None,
                error_code: Some(error_code),
                error_details: Some(error_details),
                usage: None,
            }))
        }
    }
}

/// Test Gemini Embedding API.
async fn test_gemini_embedding(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
) -> std::result::Result<(usize, ProviderTestUsage), (String, String)> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:embedContent?key={}",
        model, api_key
    );

    let body = json!({
        "model": format!("models/{}", model),
        "content": {
            "parts": [{"text": "Hello"}]
        }
    });

    let response = client.post(&url).json(&body).send().await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let embedding = json["embedding"]["values"]
        .as_array()
        .ok_or_else(|| ("INVALID_RESPONSE".to_string(), "No embedding values in response".to_string()))?;

    let usage = ProviderTestUsage {
        input_tokens: Some(1), // Gemini doesn't return token count for embeddings
        output_tokens: None,
        total_tokens: Some(1),
    };

    Ok((embedding.len(), usage))
}

/// Test OpenAI Embedding API.
async fn test_openai_embedding(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
) -> std::result::Result<(usize, ProviderTestUsage), (String, String)> {
    let body = json!({
        "model": model,
        "input": "Hello"
    });

    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| ("REQUEST_FAILED".to_string(), e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_default();
        return Err((format!("HTTP_{}", status), error_text));
    }

    let json: JsonValue = response.json().await
        .map_err(|e| ("PARSE_ERROR".to_string(), e.to_string()))?;

    let embedding = json["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| ("INVALID_RESPONSE".to_string(), "No embedding in response".to_string()))?;

    let usage = ProviderTestUsage {
        input_tokens: json["usage"]["prompt_tokens"].as_i64(),
        output_tokens: None,
        total_tokens: json["usage"]["total_tokens"].as_i64(),
    };

    Ok((embedding.len(), usage))
}

// ============================================================================
// OAuth Handlers
// ============================================================================

/// Generate PKCE code verifier and challenge.
fn generate_pkce() -> (String, String) {
    use rand::Rng;

    // Generate 32 random bytes for verifier
    let verifier_bytes: [u8; 32] = rand::thread_rng().gen();
    let verifier = base64_url_encode(&verifier_bytes);

    // Generate challenge using SHA256
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge_bytes = hasher.finalize();
    let challenge = base64_url_encode(&challenge_bytes);

    (verifier, challenge)
}

/// Base64 URL-safe encoding without padding.
fn base64_url_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data)
}

/// Start OAuth authorization flow.
#[axum::debug_handler]
async fn oauth_authorize(
    State(state): State<AppState>,
    Path((provider_type, provider_name)): Path<(String, String)>,
    Query(query): Query<OAuthAuthorizeQuery>,
) -> Result<Redirect> {
    // Currently only Anthropic supports OAuth
    if provider_name != "anthropic" {
        return Err(Error::Validation(format!(
            "OAuth not supported for provider '{}'",
            provider_name
        )));
    }

    // Generate PKCE verifier and challenge
    let (pkce_verifier, pkce_challenge) = generate_pkce();

    // Generate state token for CSRF protection
    let state_token = nanoid::nanoid!(32);

    // Store OAuth state in database
    create_provider_oauth_state(
        &state.db,
        CreateProviderOAuthState {
            id: nanoid::nanoid!(),
            state: state_token.clone(),
            provider_type: provider_type.clone(),
            provider_name: provider_name.clone(),
            pkce_verifier: Some(pkce_verifier),
            redirect_uri: query.redirect_uri.clone(),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
        },
    )
    .await?;

    // Determine authorization URL based on mode
    let auth_url = if query.mode == "console" {
        ANTHROPIC_AUTH_URL_CONSOLE
    } else {
        ANTHROPIC_AUTH_URL_MAX
    };

    // Build authorization URL
    let scopes = ANTHROPIC_SCOPES.join(" ");
    let redirect_uri = query
        .redirect_uri
        .as_deref()
        .unwrap_or(ANTHROPIC_REDIRECT_URI);

    let authorize_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        auth_url,
        ANTHROPIC_CLIENT_ID,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&scopes),
        state_token,
        pkce_challenge
    );

    info!(
        provider = %provider_name,
        mode = %query.mode,
        "Starting OAuth flow"
    );

    Ok(Redirect::to(&authorize_url))
}

/// Handle OAuth callback.
#[axum::debug_handler]
async fn oauth_callback(
    State(state): State<AppState>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<JsonValue>> {
    // Check for errors
    if let Some(error) = query.error {
        return Err(Error::Validation(format!(
            "OAuth error: {} - {}",
            error,
            query.error_description.unwrap_or_default()
        )));
    }

    // Anthropic returns code#state format
    let (code, state_token) = if let Some(code_with_state) = &query.code {
        if let Some(hash_pos) = code_with_state.find('#') {
            let code = &code_with_state[..hash_pos];
            let state = &code_with_state[hash_pos + 1..];
            (code.to_string(), state.to_string())
        } else {
            // Fallback to separate query params
            (
                code_with_state.clone(),
                query.state.clone().unwrap_or_default(),
            )
        }
    } else {
        return Err(Error::Validation("Missing authorization code".to_string()));
    };

    // Validate state token
    let oauth_state = get_valid_provider_oauth_state(&state.db, &state_token)
        .await?
        .ok_or_else(|| Error::Validation("Invalid or expired state token".to_string()))?;

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let redirect_uri = oauth_state
        .redirect_uri
        .as_deref()
        .unwrap_or(ANTHROPIC_REDIRECT_URI);

    let mut form_data = vec![
        ("grant_type", "authorization_code"),
        ("client_id", ANTHROPIC_CLIENT_ID),
        ("code", &code),
        ("redirect_uri", redirect_uri),
    ];

    // Add PKCE verifier if present
    let pkce_verifier = oauth_state.pkce_verifier.clone();
    if let Some(ref verifier) = pkce_verifier {
        form_data.push(("code_verifier", verifier));
    }

    let token_response = client
        .post(ANTHROPIC_TOKEN_URL)
        .form(&form_data)
        .send()
        .await
        .map_err(|e| Error::Internal(format!("Token exchange failed: {}", e)))?;

    if !token_response.status().is_success() {
        let error_text = token_response.text().await.unwrap_or_default();
        warn!(error = %error_text, "OAuth token exchange failed");
        return Err(Error::Validation(format!(
            "Token exchange failed: {}",
            error_text
        )));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
    }

    let tokens: TokenResponse = token_response
        .json()
        .await
        .map_err(|e| Error::Internal(format!("Failed to parse token response: {}", e)))?;

    // Calculate token expiry
    let expires_at = tokens.expires_in.map(|secs| Utc::now() + chrono::Duration::seconds(secs));

    // Update provider with OAuth tokens
    if oauth_state.provider_type == "llm" {
        if let Some(provider) = get_llm_provider_by_name(&state.db, &oauth_state.provider_name).await? {
            update_llm_provider(
                &state.db,
                &provider.id,
                UpdateLlmProvider {
                    oauth_access_token: Some(tokens.access_token.clone()),
                    oauth_refresh_token: tokens.refresh_token.clone(),
                    oauth_token_expires_at: expires_at,
                    ..Default::default()
                },
            )
            .await?;
        }
    } else if oauth_state.provider_type == "embedding" {
        if let Some(provider) = get_embedding_provider_by_name(&state.db, &oauth_state.provider_name).await? {
            update_embedding_provider(
                &state.db,
                &provider.id,
                UpdateEmbeddingProvider {
                    oauth_access_token: Some(tokens.access_token.clone()),
                    oauth_refresh_token: tokens.refresh_token.clone(),
                    oauth_token_expires_at: expires_at,
                    ..Default::default()
                },
            )
            .await?;
        }
    }

    // Clean up OAuth state
    delete_provider_oauth_state(&state.db, &state_token).await?;

    info!(
        provider = %oauth_state.provider_name,
        provider_type = %oauth_state.provider_type,
        "OAuth authentication successful"
    );

    Ok(Json(json!({
        "success": true,
        "provider": oauth_state.provider_name,
        "provider_type": oauth_state.provider_type,
        "has_refresh_token": tokens.refresh_token.is_some(),
    })))
}

// ============================================================================
// Claude Code Handlers
// ============================================================================

/// Response for Claude Code status check.
#[derive(Debug, Serialize)]
pub struct ClaudeCodeStatusResponse {
    /// Whether Claude Code credentials are detected on this machine
    pub detected: bool,
    /// Info about the detected credentials (if any)
    pub info: Option<ClaudeCodeInfo>,
    /// Whether a claudecode provider exists in the database
    pub provider_exists: bool,
    /// The provider ID if it exists
    pub provider_id: Option<String>,
}

/// Check Claude Code credentials status.
/// Returns whether credentials are auto-detected and if a provider exists.
#[axum::debug_handler]
async fn claudecode_status(
    State(state): State<AppState>,
) -> Result<Json<ClaudeCodeStatusResponse>> {
    let claudecode_service = ClaudeCodeService::new();
    let detected = claudecode_service.is_available();
    let info = claudecode_service.get_info();

    // Check if provider exists in database
    let provider = get_llm_provider_by_name(&state.db, "claudecode").await?;

    Ok(Json(ClaudeCodeStatusResponse {
        detected,
        info,
        provider_exists: provider.is_some(),
        provider_id: provider.map(|p| p.id),
    }))
}

/// Import a Claude Code OAuth token manually.
/// Users can paste their token from `~/.claude/.credentials.json` or
/// obtain it via `claude code setup-token`.
#[axum::debug_handler]
async fn import_claudecode_token(
    State(state): State<AppState>,
    Json(req): Json<ImportClaudeCodeTokenRequest>,
) -> Result<Json<LlmProviderResponse>> {
    info!(
        subscription_type = ?req.subscription_type,
        "Importing Claude Code token"
    );

    // Create or update the claudecode provider with the imported token
    let provider = seed_claudecode_provider_async(
        &state.db,
        &req.access_token,
        req.subscription_type.as_deref(),
    )
    .await?;

    info!(
        provider_id = %provider.id,
        "Claude Code provider created/updated with imported token"
    );

    Ok(Json(build_llm_provider_response(provider)))
}

/// Auto-import Claude Code credentials from the local machine.
/// Reads from `~/.claude/.credentials.json` if available.
#[axum::debug_handler]
async fn auto_import_claudecode_token(
    State(state): State<AppState>,
) -> Result<Json<LlmProviderResponse>> {
    let claudecode_service = ClaudeCodeService::new();

    // Check if credentials exist
    if !claudecode_service.is_available() {
        return Err(Error::NotFound(
            "No Claude Code credentials found. Please run 'claude login' first.".to_string(),
        ));
    }

    // Read credentials
    let creds = claudecode_service.read_credentials().map_err(|e| {
        Error::Internal(format!("Failed to read Claude Code credentials: {}", e))
    })?;

    // Get access token
    let access_token = creds.access_token().ok_or_else(|| {
        Error::Validation("Claude Code token is expired. Please run 'claude login' again.".to_string())
    })?;

    info!(
        subscription_type = ?creds.subscription_type(),
        "Auto-importing Claude Code token from local credentials"
    );

    // Create or update the claudecode provider
    let provider = seed_claudecode_provider_async(
        &state.db,
        access_token,
        creds.subscription_type(),
    )
    .await?;

    info!(
        provider_id = %provider.id,
        "Claude Code provider auto-imported successfully"
    );

    Ok(Json(build_llm_provider_response(provider)))
}

/// Helper to build LlmProviderResponse from a database row.
fn build_llm_provider_response(provider: LlmProviderRow) -> LlmProviderResponse {
    let enabled = provider.is_enabled();
    let has_api_key = provider.api_key.is_some();
    let has_oauth_token = provider.oauth_access_token.is_some();
    let oauth_token_expired = provider.is_oauth_token_expired();
    let config_json = serde_json::from_str(&provider.config).unwrap_or(json!({}));

    LlmProviderResponse {
        id: provider.id,
        name: provider.name,
        enabled,
        priority: provider.priority,
        auth_type: provider.auth_type,
        has_api_key,
        has_oauth_token,
        oauth_token_expired,
        config: config_json,
        request_count: provider.request_count,
        token_count: provider.token_count,
        error_count: provider.error_count,
        last_error: provider.last_error,
        last_error_at: provider.last_error_at,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        last_used_at: provider.last_used_at,
    }
}
