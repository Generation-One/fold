//! Provider configuration models for LLM and embedding services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;

/// Provider authentication type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    ApiKey,
    OAuth,
}

impl AuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthType::ApiKey => "api_key",
            AuthType::OAuth => "oauth",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "api_key" => Some(AuthType::ApiKey),
            "oauth" => Some(AuthType::OAuth),
            _ => None,
        }
    }
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::ApiKey
    }
}

/// Supported LLM provider names
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderName {
    Gemini,
    OpenAI,
    Anthropic,
    OpenRouter,
}

impl LlmProviderName {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmProviderName::Gemini => "gemini",
            LlmProviderName::OpenAI => "openai",
            LlmProviderName::Anthropic => "anthropic",
            LlmProviderName::OpenRouter => "openrouter",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gemini" => Some(LlmProviderName::Gemini),
            "openai" => Some(LlmProviderName::OpenAI),
            "anthropic" => Some(LlmProviderName::Anthropic),
            "openrouter" => Some(LlmProviderName::OpenRouter),
            _ => None,
        }
    }
}

/// Supported embedding provider names
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingProviderName {
    Gemini,
    OpenAI,
}

impl EmbeddingProviderName {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmbeddingProviderName::Gemini => "gemini",
            EmbeddingProviderName::OpenAI => "openai",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gemini" => Some(EmbeddingProviderName::Gemini),
            "openai" => Some(EmbeddingProviderName::OpenAI),
            _ => None,
        }
    }
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct LlmProvider {
    pub id: String,
    pub name: String,
    pub enabled: i32, // SQLite boolean (0/1)
    pub priority: i32,
    pub auth_type: String,

    // API key auth (encrypted in DB)
    pub api_key: Option<String>,

    // OAuth auth (encrypted in DB)
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expires_at: Option<DateTime<Utc>>,
    pub oauth_scopes: Option<String>, // JSON array

    // Provider-specific config (JSON)
    pub config: String, // { model, endpoint, max_tokens, etc. }

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl LlmProvider {
    /// Create a new LLM provider
    pub fn new(name: String, auth_type: AuthType) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            name,
            enabled: 1,
            priority: 0,
            auth_type: auth_type.as_str().to_string(),
            api_key: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_token_expires_at: None,
            oauth_scopes: None,
            config: "{}".to_string(),
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    /// Get the typed auth type
    pub fn get_auth_type(&self) -> Option<AuthType> {
        AuthType::from_str(&self.auth_type)
    }

    /// Get the typed provider name
    pub fn get_provider_name(&self) -> Option<LlmProviderName> {
        LlmProviderName::from_str(&self.name)
    }

    /// Check if provider is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled == 1
    }

    /// Parse config from JSON string
    pub fn config_json(&self) -> Result<JsonValue, serde_json::Error> {
        serde_json::from_str(&self.config)
    }

    /// Get model from config
    pub fn model(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("model").and_then(|m| m.as_str()).map(String::from))
    }

    /// Get endpoint from config
    pub fn endpoint(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("endpoint").and_then(|e| e.as_str()).map(String::from))
    }

    /// Parse OAuth scopes from JSON string
    pub fn scopes_vec(&self) -> Vec<String> {
        self.oauth_scopes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Check if OAuth token has expired
    pub fn is_oauth_token_expired(&self) -> bool {
        self.oauth_token_expires_at
            .map(|exp| exp < Utc::now())
            .unwrap_or(false)
    }
}

/// Embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct EmbeddingProvider {
    pub id: String,
    pub name: String,
    pub enabled: i32, // SQLite boolean (0/1)
    pub priority: i32,
    pub auth_type: String,

    // API key auth (encrypted in DB)
    pub api_key: Option<String>,

    // OAuth auth (encrypted in DB)
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expires_at: Option<DateTime<Utc>>,
    pub oauth_scopes: Option<String>, // JSON array

    // Provider-specific config (JSON)
    pub config: String, // { model, dimension, endpoint, etc. }

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl EmbeddingProvider {
    /// Create a new embedding provider
    pub fn new(name: String, auth_type: AuthType) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            name,
            enabled: 1,
            priority: 0,
            auth_type: auth_type.as_str().to_string(),
            api_key: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_token_expires_at: None,
            oauth_scopes: None,
            config: "{}".to_string(),
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    /// Get the typed auth type
    pub fn get_auth_type(&self) -> Option<AuthType> {
        AuthType::from_str(&self.auth_type)
    }

    /// Get the typed provider name
    pub fn get_provider_name(&self) -> Option<EmbeddingProviderName> {
        EmbeddingProviderName::from_str(&self.name)
    }

    /// Check if provider is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled == 1
    }

    /// Parse config from JSON string
    pub fn config_json(&self) -> Result<JsonValue, serde_json::Error> {
        serde_json::from_str(&self.config)
    }

    /// Get model from config
    pub fn model(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("model").and_then(|m| m.as_str()).map(String::from))
    }

    /// Get dimension from config
    pub fn dimension(&self) -> Option<usize> {
        self.config_json().ok().and_then(|c| {
            c.get("dimension")
                .and_then(|d| d.as_u64())
                .map(|d| d as usize)
        })
    }

    /// Get endpoint from config
    pub fn endpoint(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("endpoint").and_then(|e| e.as_str()).map(String::from))
    }

    /// Parse OAuth scopes from JSON string
    pub fn scopes_vec(&self) -> Vec<String> {
        self.oauth_scopes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Check if OAuth token has expired
    pub fn is_oauth_token_expired(&self) -> bool {
        self.oauth_token_expires_at
            .map(|exp| exp < Utc::now())
            .unwrap_or(false)
    }
}

/// OAuth state for provider authentication flow
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct ProviderOAuthState {
    pub id: String,
    pub state: String,
    pub provider_type: String, // "llm" | "embedding"
    pub provider_name: String, // "gemini", "openai", etc.
    pub pkce_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl ProviderOAuthState {
    /// Create a new OAuth state
    pub fn new(
        state: String,
        provider_type: String,
        provider_name: String,
        redirect_uri: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            state,
            provider_type,
            provider_name,
            pkce_verifier: None,
            redirect_uri,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(10),
        }
    }

    /// Check if the state has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

/// DTO for creating/updating LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateLlmProviderDto {
    pub name: String,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub config: Option<JsonValue>,
}

/// DTO for creating/updating embedding providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateEmbeddingProviderDto {
    pub name: String,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub config: Option<JsonValue>,
}

/// DTO for provider test results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderTestResult {
    pub success: bool,
    pub message: String,
    pub details: Option<JsonValue>,
}
