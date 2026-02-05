//! LLM and Embedding provider database queries.
//!
//! Handles provider configuration storage, retrieval, and OAuth state management.

use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// LLM Provider Types
// ============================================================================

/// LLM provider record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LlmProviderRow {
    pub id: String,
    pub name: String,
    pub enabled: i32,
    pub priority: i32,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expires_at: Option<String>,
    pub oauth_scopes: Option<String>,
    pub config: String,
    // Usage tracking
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

impl LlmProviderRow {
    pub fn is_enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn config_json(&self) -> Result<JsonValue> {
        serde_json::from_str(&self.config).map_err(|e| Error::Internal(e.to_string()))
    }

    pub fn model(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("model").and_then(|m| m.as_str()).map(String::from))
    }

    pub fn endpoint(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("endpoint").and_then(|e| e.as_str()).map(String::from))
    }

    pub fn is_oauth_token_expired(&self) -> bool {
        if let Some(ref expires) = self.oauth_token_expires_at {
            if let Ok(dt) = DateTime::parse_from_rfc3339(expires) {
                return dt < Utc::now();
            }
        }
        // If no expiry set, consider it not expired
        false
    }
}

/// Input for creating/updating an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLlmProvider {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub config: JsonValue,
}

/// Input for updating an LLM provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateLlmProvider {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub api_key: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expires_at: Option<DateTime<Utc>>,
    pub config: Option<JsonValue>,
}

// ============================================================================
// Embedding Provider Types
// ============================================================================

/// Embedding provider record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EmbeddingProviderRow {
    pub id: String,
    pub name: String,
    pub enabled: i32,
    pub priority: i32,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expires_at: Option<String>,
    pub oauth_scopes: Option<String>,
    pub config: String,
    // Usage tracking
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

impl EmbeddingProviderRow {
    pub fn is_enabled(&self) -> bool {
        self.enabled != 0
    }

    pub fn config_json(&self) -> Result<JsonValue> {
        serde_json::from_str(&self.config).map_err(|e| Error::Internal(e.to_string()))
    }

    pub fn model(&self) -> Option<String> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("model").and_then(|m| m.as_str()).map(String::from))
    }

    pub fn dimension(&self) -> Option<usize> {
        self.config_json()
            .ok()
            .and_then(|c| c.get("dimension").and_then(|d| d.as_u64()).map(|d| d as usize))
    }

    pub fn is_oauth_token_expired(&self) -> bool {
        if let Some(ref expires) = self.oauth_token_expires_at {
            if let Ok(dt) = DateTime::parse_from_rfc3339(expires) {
                return dt < Utc::now();
            }
        }
        false
    }
}

/// Input for creating/updating an embedding provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEmbeddingProvider {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub config: JsonValue,
}

/// Input for updating an embedding provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateEmbeddingProvider {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub api_key: Option<String>,
    pub oauth_access_token: Option<String>,
    pub oauth_refresh_token: Option<String>,
    pub oauth_token_expires_at: Option<DateTime<Utc>>,
    pub config: Option<JsonValue>,
}

// ============================================================================
// OAuth State Types
// ============================================================================

/// OAuth state for provider authentication flow.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProviderOAuthStateRow {
    pub id: String,
    pub state: String,
    pub provider_type: String,
    pub provider_name: String,
    pub pkce_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub created_at: String,
    pub expires_at: String,
}

impl ProviderOAuthStateRow {
    pub fn is_expired(&self) -> bool {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&self.expires_at) {
            dt < Utc::now()
        } else {
            true
        }
    }
}

/// Input for creating OAuth state.
#[derive(Debug, Clone)]
pub struct CreateProviderOAuthState {
    pub id: String,
    pub state: String,
    pub provider_type: String,
    pub provider_name: String,
    pub pkce_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub expires_at: DateTime<Utc>,
}

// ============================================================================
// LLM Provider Queries
// ============================================================================

/// Create a new LLM provider.
pub async fn create_llm_provider(pool: &DbPool, input: CreateLlmProvider) -> Result<LlmProviderRow> {
    let id = crate::models::new_id();
    let config_json = serde_json::to_string(&input.config)?;

    sqlx::query_as::<_, LlmProviderRow>(
        r#"
        INSERT INTO llm_providers (id, name, enabled, priority, auth_type, api_key, config)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&id)
    .bind(&input.name)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(input.priority)
    .bind(&input.auth_type)
    .bind(&input.api_key)
    .bind(&config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("LLM provider '{}' already exists", input.name))
        }
        _ => Error::Database(e),
    })
}

/// Get an LLM provider by ID.
pub async fn get_llm_provider(pool: &DbPool, id: &str) -> Result<Option<LlmProviderRow>> {
    sqlx::query_as::<_, LlmProviderRow>("SELECT * FROM llm_providers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get an LLM provider by name.
pub async fn get_llm_provider_by_name(pool: &DbPool, name: &str) -> Result<Option<LlmProviderRow>> {
    sqlx::query_as::<_, LlmProviderRow>("SELECT * FROM llm_providers WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// List all LLM providers.
pub async fn list_llm_providers(pool: &DbPool) -> Result<Vec<LlmProviderRow>> {
    sqlx::query_as::<_, LlmProviderRow>(
        "SELECT * FROM llm_providers ORDER BY priority ASC, name ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List enabled LLM providers ordered by priority.
pub async fn list_enabled_llm_providers(pool: &DbPool) -> Result<Vec<LlmProviderRow>> {
    sqlx::query_as::<_, LlmProviderRow>(
        r#"
        SELECT * FROM llm_providers
        WHERE enabled = 1
        ORDER BY priority ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Update an LLM provider.
pub async fn update_llm_provider(
    pool: &DbPool,
    id: &str,
    input: UpdateLlmProvider,
) -> Result<LlmProviderRow> {
    let mut updates = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(enabled) = input.enabled {
        updates.push("enabled = ?");
        bindings.push(if enabled { "1".to_string() } else { "0".to_string() });
    }
    if let Some(priority) = input.priority {
        updates.push("priority = ?");
        bindings.push(priority.to_string());
    }
    if let Some(api_key) = input.api_key {
        updates.push("api_key = ?");
        bindings.push(api_key);
    }
    if let Some(access_token) = input.oauth_access_token {
        updates.push("oauth_access_token = ?");
        bindings.push(access_token);
    }
    if let Some(refresh_token) = input.oauth_refresh_token {
        updates.push("oauth_refresh_token = ?");
        bindings.push(refresh_token);
    }
    if let Some(expires_at) = input.oauth_token_expires_at {
        updates.push("oauth_token_expires_at = ?");
        bindings.push(expires_at.to_rfc3339());
    }
    if let Some(config) = input.config {
        updates.push("config = ?");
        bindings.push(serde_json::to_string(&config)?);
    }

    if updates.is_empty() {
        return get_llm_provider(pool, id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("LLM provider not found: {}", id)));
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE llm_providers SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, LlmProviderRow>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("LLM provider not found: {}", id)))
}

/// Update LLM provider's last_used_at timestamp.
pub async fn update_llm_provider_last_used(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("UPDATE llm_providers SET last_used_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete an LLM provider.
pub async fn delete_llm_provider(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM llm_providers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("LLM provider not found: {}", id)));
    }

    Ok(())
}

/// Upsert an LLM provider (insert or update by name).
pub async fn upsert_llm_provider(pool: &DbPool, input: CreateLlmProvider) -> Result<LlmProviderRow> {
    let id = crate::models::new_id();
    let config_json = serde_json::to_string(&input.config)?;

    sqlx::query_as::<_, LlmProviderRow>(
        r#"
        INSERT INTO llm_providers (id, name, enabled, priority, auth_type, api_key, config)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(name) DO UPDATE SET
            enabled = excluded.enabled,
            priority = excluded.priority,
            auth_type = excluded.auth_type,
            api_key = COALESCE(excluded.api_key, llm_providers.api_key),
            config = excluded.config,
            updated_at = datetime('now')
        RETURNING *
        "#,
    )
    .bind(&id)
    .bind(&input.name)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(input.priority)
    .bind(&input.auth_type)
    .bind(&input.api_key)
    .bind(&config_json)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// Embedding Provider Queries
// ============================================================================

/// Create a new embedding provider.
pub async fn create_embedding_provider(
    pool: &DbPool,
    input: CreateEmbeddingProvider,
) -> Result<EmbeddingProviderRow> {
    let id = crate::models::new_id();
    let config_json = serde_json::to_string(&input.config)?;

    sqlx::query_as::<_, EmbeddingProviderRow>(
        r#"
        INSERT INTO embedding_providers (id, name, enabled, priority, auth_type, api_key, config)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&id)
    .bind(&input.name)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(input.priority)
    .bind(&input.auth_type)
    .bind(&input.api_key)
    .bind(&config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("Embedding provider '{}' already exists", input.name))
        }
        _ => Error::Database(e),
    })
}

/// Get an embedding provider by ID.
pub async fn get_embedding_provider(pool: &DbPool, id: &str) -> Result<Option<EmbeddingProviderRow>> {
    sqlx::query_as::<_, EmbeddingProviderRow>("SELECT * FROM embedding_providers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get an embedding provider by name.
pub async fn get_embedding_provider_by_name(
    pool: &DbPool,
    name: &str,
) -> Result<Option<EmbeddingProviderRow>> {
    sqlx::query_as::<_, EmbeddingProviderRow>("SELECT * FROM embedding_providers WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// List all embedding providers.
pub async fn list_embedding_providers(pool: &DbPool) -> Result<Vec<EmbeddingProviderRow>> {
    sqlx::query_as::<_, EmbeddingProviderRow>(
        "SELECT * FROM embedding_providers ORDER BY priority ASC, name ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List enabled embedding providers ordered by priority.
pub async fn list_enabled_embedding_providers(pool: &DbPool) -> Result<Vec<EmbeddingProviderRow>> {
    sqlx::query_as::<_, EmbeddingProviderRow>(
        r#"
        SELECT * FROM embedding_providers
        WHERE enabled = 1
        ORDER BY priority ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Update an embedding provider.
pub async fn update_embedding_provider(
    pool: &DbPool,
    id: &str,
    input: UpdateEmbeddingProvider,
) -> Result<EmbeddingProviderRow> {
    let mut updates = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(enabled) = input.enabled {
        updates.push("enabled = ?");
        bindings.push(if enabled { "1".to_string() } else { "0".to_string() });
    }
    if let Some(priority) = input.priority {
        updates.push("priority = ?");
        bindings.push(priority.to_string());
    }
    if let Some(api_key) = input.api_key {
        updates.push("api_key = ?");
        bindings.push(api_key);
    }
    if let Some(access_token) = input.oauth_access_token {
        updates.push("oauth_access_token = ?");
        bindings.push(access_token);
    }
    if let Some(refresh_token) = input.oauth_refresh_token {
        updates.push("oauth_refresh_token = ?");
        bindings.push(refresh_token);
    }
    if let Some(expires_at) = input.oauth_token_expires_at {
        updates.push("oauth_token_expires_at = ?");
        bindings.push(expires_at.to_rfc3339());
    }
    if let Some(config) = input.config {
        updates.push("config = ?");
        bindings.push(serde_json::to_string(&config)?);
    }

    if updates.is_empty() {
        return get_embedding_provider(pool, id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding provider not found: {}", id)));
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE embedding_providers SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, EmbeddingProviderRow>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Embedding provider not found: {}", id)))
}

/// Update embedding provider's last_used_at timestamp.
pub async fn update_embedding_provider_last_used(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("UPDATE embedding_providers SET last_used_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete an embedding provider.
pub async fn delete_embedding_provider(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM embedding_providers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!(
            "Embedding provider not found: {}",
            id
        )));
    }

    Ok(())
}

/// Upsert an embedding provider (insert or update by name).
pub async fn upsert_embedding_provider(
    pool: &DbPool,
    input: CreateEmbeddingProvider,
) -> Result<EmbeddingProviderRow> {
    let id = crate::models::new_id();
    let config_json = serde_json::to_string(&input.config)?;

    sqlx::query_as::<_, EmbeddingProviderRow>(
        r#"
        INSERT INTO embedding_providers (id, name, enabled, priority, auth_type, api_key, config)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(name) DO UPDATE SET
            enabled = excluded.enabled,
            priority = excluded.priority,
            auth_type = excluded.auth_type,
            api_key = COALESCE(excluded.api_key, embedding_providers.api_key),
            config = excluded.config,
            updated_at = datetime('now')
        RETURNING *
        "#,
    )
    .bind(&id)
    .bind(&input.name)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(input.priority)
    .bind(&input.auth_type)
    .bind(&input.api_key)
    .bind(&config_json)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// OAuth State Queries
// ============================================================================

/// Create a new OAuth state for provider authentication.
pub async fn create_provider_oauth_state(
    pool: &DbPool,
    input: CreateProviderOAuthState,
) -> Result<ProviderOAuthStateRow> {
    sqlx::query_as::<_, ProviderOAuthStateRow>(
        r#"
        INSERT INTO provider_oauth_states (id, state, provider_type, provider_name, pkce_verifier, redirect_uri, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.state)
    .bind(&input.provider_type)
    .bind(&input.provider_name)
    .bind(&input.pkce_verifier)
    .bind(&input.redirect_uri)
    .bind(input.expires_at.to_rfc3339())
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get OAuth state by state token.
pub async fn get_provider_oauth_state(
    pool: &DbPool,
    state: &str,
) -> Result<Option<ProviderOAuthStateRow>> {
    sqlx::query_as::<_, ProviderOAuthStateRow>(
        "SELECT * FROM provider_oauth_states WHERE state = ?",
    )
    .bind(state)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Get valid (non-expired) OAuth state by state token.
pub async fn get_valid_provider_oauth_state(
    pool: &DbPool,
    state: &str,
) -> Result<Option<ProviderOAuthStateRow>> {
    sqlx::query_as::<_, ProviderOAuthStateRow>(
        r#"
        SELECT * FROM provider_oauth_states
        WHERE state = ? AND expires_at > datetime('now')
        "#,
    )
    .bind(state)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Delete OAuth state by state token.
pub async fn delete_provider_oauth_state(pool: &DbPool, state: &str) -> Result<()> {
    sqlx::query("DELETE FROM provider_oauth_states WHERE state = ?")
        .bind(state)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cleanup expired OAuth states.
pub async fn cleanup_expired_provider_oauth_states(pool: &DbPool) -> Result<u64> {
    let result =
        sqlx::query("DELETE FROM provider_oauth_states WHERE expires_at < datetime('now')")
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Seed/Bootstrap Functions
// ============================================================================

/// Seed LLM providers from environment variables.
/// This populates the database with providers from the current env config.
pub async fn seed_llm_providers_from_env(
    pool: &DbPool,
    providers: &[crate::config::LlmProvider],
) -> Result<Vec<LlmProviderRow>> {
    let mut results = Vec::new();

    for provider in providers {
        let config = serde_json::json!({
            "model": provider.model,
            "endpoint": provider.base_url,
        });

        let row = upsert_llm_provider(
            pool,
            CreateLlmProvider {
                name: provider.name.clone(),
                enabled: true,
                priority: provider.priority as i32,
                auth_type: "api_key".to_string(),
                api_key: Some(provider.api_key.clone()),
                config,
            },
        )
        .await?;

        results.push(row);
    }

    Ok(results)
}

/// Seed embedding providers from environment variables.
pub async fn seed_embedding_providers_from_env(
    pool: &DbPool,
    providers: &[crate::config::EmbeddingProvider],
) -> Result<Vec<EmbeddingProviderRow>> {
    let mut results = Vec::new();

    for provider in providers {
        let config = serde_json::json!({
            "model": provider.model,
            "endpoint": provider.base_url,
        });

        let row = upsert_embedding_provider(
            pool,
            CreateEmbeddingProvider {
                name: provider.name.clone(),
                enabled: true,
                priority: provider.priority as i32,
                auth_type: "api_key".to_string(),
                api_key: Some(provider.api_key.clone()),
                config,
            },
        )
        .await?;

        results.push(row);
    }

    Ok(results)
}

/// Seed Claude Code provider from ~/.claude credentials.
/// This reads the OAuth token from Claude Code's local credentials file.
pub async fn seed_claudecode_provider_async(
    pool: &DbPool,
    access_token: &str,
    subscription_type: Option<&str>,
) -> Result<LlmProviderRow> {
    let config = serde_json::json!({
        "model": "claude-3-5-haiku-20241022",
        "endpoint": "https://api.anthropic.com/v1",
        "subscription_type": subscription_type,
    });

    // Upsert the provider record
    upsert_llm_provider(
        pool,
        CreateLlmProvider {
            name: "claudecode".to_string(),
            enabled: true,
            priority: 5, // Lower priority than explicit API key providers
            auth_type: "oauth".to_string(),
            api_key: None,
            config,
        },
    )
    .await?;

    // Update the OAuth token and return the updated row
    sqlx::query_as::<_, LlmProviderRow>(
        r#"
        UPDATE llm_providers
        SET oauth_access_token = ?,
            updated_at = datetime('now')
        WHERE name = 'claudecode'
        RETURNING *
        "#,
    )
    .bind(access_token)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// Usage Tracking Functions
// ============================================================================

/// Record successful LLM provider usage.
pub async fn record_llm_provider_usage(
    pool: &DbPool,
    id: &str,
    tokens_used: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE llm_providers
        SET request_count = request_count + 1,
            token_count = token_count + ?,
            last_used_at = datetime('now'),
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(tokens_used)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record LLM provider error.
pub async fn record_llm_provider_error(
    pool: &DbPool,
    id: &str,
    error_message: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE llm_providers
        SET error_count = error_count + 1,
            last_error = ?,
            last_error_at = datetime('now'),
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(error_message)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record successful embedding provider usage.
pub async fn record_embedding_provider_usage(
    pool: &DbPool,
    id: &str,
    tokens_used: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE embedding_providers
        SET request_count = request_count + 1,
            token_count = token_count + ?,
            last_used_at = datetime('now'),
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(tokens_used)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record embedding provider error.
pub async fn record_embedding_provider_error(
    pool: &DbPool,
    id: &str,
    error_message: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE embedding_providers
        SET error_count = error_count + 1,
            last_error = ?,
            last_error_at = datetime('now'),
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(error_message)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reset usage stats for an LLM provider.
pub async fn reset_llm_provider_stats(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE llm_providers
        SET request_count = 0,
            token_count = 0,
            error_count = 0,
            last_error = NULL,
            last_error_at = NULL,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reset usage stats for an embedding provider.
pub async fn reset_embedding_provider_stats(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE embedding_providers
        SET request_count = 0,
            token_count = 0,
            error_count = 0,
            last_error = NULL,
            last_error_at = NULL,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
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
    async fn test_create_and_get_llm_provider() {
        let pool = setup_test_db().await;

        let input = CreateLlmProvider {
            name: "openai".to_string(),
            enabled: true,
            priority: 1,
            auth_type: "api_key".to_string(),
            api_key: Some("sk-test-key".to_string()),
            config: serde_json::json!({
                "model": "gpt-4o-mini",
                "endpoint": "https://api.openai.com/v1"
            }),
        };

        let provider = create_llm_provider(&pool, input).await.unwrap();
        assert_eq!(provider.name, "openai");
        assert!(provider.is_enabled());

        let fetched = get_llm_provider(&pool, &provider.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, provider.id);
        assert_eq!(fetched.model(), Some("gpt-4o-mini".to_string()));
    }

    #[tokio::test]
    async fn test_list_enabled_providers() {
        let pool = setup_test_db().await;

        // Create enabled provider
        create_llm_provider(
            &pool,
            CreateLlmProvider {
                name: "gemini".to_string(),
                enabled: true,
                priority: 1,
                auth_type: "api_key".to_string(),
                api_key: None,
                config: serde_json::json!({}),
            },
        )
        .await
        .unwrap();

        // Create disabled provider
        create_llm_provider(
            &pool,
            CreateLlmProvider {
                name: "openai".to_string(),
                enabled: false,
                priority: 2,
                auth_type: "api_key".to_string(),
                api_key: None,
                config: serde_json::json!({}),
            },
        )
        .await
        .unwrap();

        let enabled = list_enabled_llm_providers(&pool).await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "gemini");
    }

    #[tokio::test]
    async fn test_upsert_provider() {
        let pool = setup_test_db().await;

        let input = CreateLlmProvider {
            name: "anthropic".to_string(),
            enabled: true,
            priority: 2,
            auth_type: "api_key".to_string(),
            api_key: Some("key-1".to_string()),
            config: serde_json::json!({"model": "claude-3-5-haiku"}),
        };

        // First insert
        let p1 = upsert_llm_provider(&pool, input.clone()).await.unwrap();
        assert_eq!(p1.api_key, Some("key-1".to_string()));

        // Upsert with new key
        let input2 = CreateLlmProvider {
            name: "anthropic".to_string(),
            enabled: true,
            priority: 1, // Changed priority
            auth_type: "api_key".to_string(),
            api_key: Some("key-2".to_string()),
            config: serde_json::json!({"model": "claude-3-5-sonnet"}),
        };

        let p2 = upsert_llm_provider(&pool, input2).await.unwrap();
        assert_eq!(p2.id, p1.id); // Same ID
        assert_eq!(p2.priority, 1); // Updated priority
        assert_eq!(p2.api_key, Some("key-2".to_string())); // Updated key
    }

    #[tokio::test]
    async fn test_oauth_state_lifecycle() {
        let pool = setup_test_db().await;

        let state = CreateProviderOAuthState {
            id: "state-1".to_string(),
            state: "random-state-token".to_string(),
            provider_type: "llm".to_string(),
            provider_name: "anthropic".to_string(),
            pkce_verifier: Some("verifier-123".to_string()),
            redirect_uri: Some("http://localhost:8765/callback".to_string()),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
        };

        let created = create_provider_oauth_state(&pool, state).await.unwrap();
        assert_eq!(created.provider_name, "anthropic");

        let fetched = get_valid_provider_oauth_state(&pool, "random-state-token")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.pkce_verifier, Some("verifier-123".to_string()));

        delete_provider_oauth_state(&pool, "random-state-token")
            .await
            .unwrap();

        let deleted = get_provider_oauth_state(&pool, "random-state-token")
            .await
            .unwrap();
        assert!(deleted.is_none());
    }
}
