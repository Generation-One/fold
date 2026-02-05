//! Embedding service with multi-provider fallback.
//!
//! Supports Gemini and OpenAI embedding APIs with automatic fallback
//! when rate limits are hit or providers fail. Falls back to hash-based
//! placeholders when no providers are configured.
//!
//! Providers are loaded from the database with fallback to environment variables
//! for initial seeding.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::config::EmbeddingConfig;
use crate::db::{
    list_enabled_embedding_providers, seed_embedding_providers_from_env,
    update_embedding_provider_last_used, DbPool, EmbeddingProviderRow,
};
use crate::error::{Error, Result};

/// Maximum retries per provider before fallback
const MAX_RETRIES: u32 = 2;

/// Delay between retries (doubles each time)
const RETRY_DELAY_MS: u64 = 500;

/// Maximum texts per batch for API calls
const MAX_BATCH_SIZE: usize = 100;

/// Runtime provider configuration (loaded from database)
#[derive(Debug, Clone)]
pub struct RuntimeEmbeddingProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub oauth_access_token: Option<String>,
    pub dimension: Option<usize>,
    pub priority: i32,
}

impl RuntimeEmbeddingProvider {
    /// Get the authentication token (prefers OAuth, falls back to API key)
    pub fn auth_token(&self) -> Option<&str> {
        self.oauth_access_token
            .as_deref()
            .or(self.api_key.as_deref())
    }

    /// Check if provider has valid credentials
    pub fn has_credentials(&self) -> bool {
        self.api_key.is_some() || self.oauth_access_token.is_some()
    }
}

impl From<EmbeddingProviderRow> for RuntimeEmbeddingProvider {
    fn from(row: EmbeddingProviderRow) -> Self {
        let config = row.config_json().unwrap_or(json!({}));

        Self {
            id: row.id,
            name: row.name.clone(),
            base_url: config
                .get("endpoint")
                .and_then(|e| e.as_str())
                .map(String::from)
                .unwrap_or_else(|| default_endpoint(&row.name)),
            model: config
                .get("model")
                .and_then(|m| m.as_str())
                .map(String::from)
                .unwrap_or_else(|| default_model(&row.name)),
            api_key: row.api_key,
            oauth_access_token: row.oauth_access_token,
            dimension: config
                .get("dimension")
                .and_then(|d| d.as_u64())
                .map(|d| d as usize),
            priority: row.priority,
        }
    }
}

/// Get default endpoint for a provider
fn default_endpoint(name: &str) -> String {
    match name {
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        "openai" => "https://api.openai.com/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

/// Get default model for a provider
fn default_model(name: &str) -> String {
    match name {
        // text-embedding-001 was discontinued Nov 2025
        "gemini" => "text-embedding-001".to_string(),
        "openai" => "text-embedding-3-small".to_string(),
        _ => "text-embedding-3-small".to_string(),
    }
}

/// Get default dimension for a model
fn default_dimension(model: &str) -> usize {
    if model.contains("gemini-embedding-001") {
        // gemini-embedding-001 default is 3072, but can be scaled down via MRL
        3072
    } else if model.contains("text-embedding-001") || model.contains("embedding-001") {
        768
    } else if model.contains("text-embedding-3-small") {
        1536
    } else if model.contains("text-embedding-3-large") {
        3072
    } else if model.contains("text-embedding-ada-002") {
        1536
    } else if model.contains("MiniLM-L6") {
        384
    } else if model.contains("mpnet") {
        768
    } else {
        384 // Default
    }
}

/// Service for generating text embeddings with multi-provider fallback.
///
/// Tries providers in priority order (Gemini -> OpenAI),
/// automatically falling back on rate limits or failures.
/// Uses hash-based placeholders when no providers are configured.
#[derive(Clone)]
pub struct EmbeddingService {
    inner: Arc<EmbeddingServiceInner>,
}

struct EmbeddingServiceInner {
    db: Option<DbPool>,
    providers: RwLock<Vec<RuntimeEmbeddingProvider>>,
    dimension: RwLock<usize>,
    client: Client,
    initialized: RwLock<bool>,
}

/// Gemini embedding response
#[derive(Debug, Deserialize)]
struct GeminiEmbedResponse {
    embedding: Option<GeminiEmbedding>,
    error: Option<GeminiError>,
}

#[derive(Debug, Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct GeminiError {
    message: String,
    code: Option<i32>,
}

/// Gemini batch embedding response
#[derive(Debug, Deserialize)]
struct GeminiBatchResponse {
    embeddings: Option<Vec<GeminiEmbedding>>,
    error: Option<GeminiError>,
}

/// OpenAI embedding response
#[derive(Debug, Deserialize)]
struct OpenAIEmbedResponse {
    data: Option<Vec<OpenAIEmbedding>>,
    error: Option<OpenAIError>,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbedding {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct OpenAIError {
    message: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: Option<String>,
}

impl EmbeddingService {
    /// Create a new embedding service with database-backed providers.
    ///
    /// On first run, seeds providers from environment variables.
    pub async fn new(db: DbPool, config: &EmbeddingConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        // Try to load providers from database
        let db_providers = list_enabled_embedding_providers(&db).await?;

        let (providers, dimension) = if db_providers.is_empty() {
            // Seed from environment variables on first run
            if !config.providers.is_empty() {
                info!("Seeding embedding providers from environment variables");
                seed_embedding_providers_from_env(&db, &config.providers).await?;
                let seeded = list_enabled_embedding_providers(&db).await?;
                let providers: Vec<RuntimeEmbeddingProvider> =
                    seeded.into_iter().map(RuntimeEmbeddingProvider::from).collect();

                // Determine dimension from first provider
                let dim = providers
                    .first()
                    .and_then(|p| p.dimension.or_else(|| Some(default_dimension(&p.model))))
                    .unwrap_or(config.dimension);

                (providers, dim)
            } else {
                (Vec::new(), config.dimension)
            }
        } else {
            let providers: Vec<RuntimeEmbeddingProvider> =
                db_providers.into_iter().map(RuntimeEmbeddingProvider::from).collect();

            // Determine dimension from first provider
            let dim = providers
                .first()
                .and_then(|p| p.dimension.or_else(|| Some(default_dimension(&p.model))))
                .unwrap_or(config.dimension);

            (providers, dim)
        };

        if providers.is_empty() {
            warn!(
                dimension = dimension,
                "No embedding providers configured - using hash-based placeholders"
            );
        } else {
            info!(
                providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
                dimension = dimension,
                "Embedding service initialized from database"
            );
        }

        Ok(Self {
            inner: Arc::new(EmbeddingServiceInner {
                db: Some(db),
                providers: RwLock::new(providers),
                dimension: RwLock::new(dimension),
                client,
                initialized: RwLock::new(false),
            }),
        })
    }

    /// Create embedding service from config only (for backwards compatibility)
    pub fn from_config(config: &EmbeddingConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        let providers: Vec<RuntimeEmbeddingProvider> = config
            .providers
            .iter()
            .map(|p| RuntimeEmbeddingProvider {
                id: String::new(),
                name: p.name.clone(),
                base_url: p.base_url.clone(),
                model: p.model.clone(),
                api_key: Some(p.api_key.clone()),
                oauth_access_token: None,
                dimension: Some(default_dimension(&p.model)),
                priority: p.priority as i32,
            })
            .collect();

        if providers.is_empty() {
            warn!(
                dimension = config.dimension,
                "No embedding providers configured - using hash-based placeholders"
            );
        } else {
            info!(
                providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
                dimension = config.dimension,
                "Embedding service initialized from config"
            );
        }

        Ok(Self {
            inner: Arc::new(EmbeddingServiceInner {
                db: None,
                providers: RwLock::new(providers),
                dimension: RwLock::new(config.dimension),
                client,
                initialized: RwLock::new(false),
            }),
        })
    }

    /// Reload providers from the database.
    pub async fn refresh_providers(&self) -> Result<()> {
        let db = self
            .inner
            .db
            .as_ref()
            .ok_or_else(|| Error::Internal("No database connection".to_string()))?;

        let db_providers = list_enabled_embedding_providers(db).await?;
        let providers: Vec<RuntimeEmbeddingProvider> = db_providers
            .into_iter()
            .map(RuntimeEmbeddingProvider::from)
            .collect();

        // Update dimension from first provider
        if let Some(first) = providers.first() {
            let dim = first
                .dimension
                .or_else(|| Some(default_dimension(&first.model)))
                .unwrap_or(384);
            let mut dim_guard = self.inner.dimension.write().await;
            *dim_guard = dim;
        }

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "Refreshed embedding providers from database"
        );

        let mut guard = self.inner.providers.write().await;
        *guard = providers;

        Ok(())
    }

    /// Get the embedding dimension
    pub async fn dimension(&self) -> usize {
        *self.inner.dimension.read().await
    }

    /// Get provider names in priority order
    pub async fn providers(&self) -> Vec<String> {
        let guard = self.inner.providers.read().await;
        guard.iter().map(|p| p.name.clone()).collect()
    }

    /// Check if real embedding providers are available
    pub async fn has_providers(&self) -> bool {
        let guard = self.inner.providers.read().await;
        !guard.is_empty()
    }

    /// Lazily initialize the service
    async fn ensure_initialized(&self) -> Result<()> {
        let mut initialized = self.inner.initialized.write().await;
        if !*initialized {
            let providers = self.inner.providers.read().await;
            if providers.is_empty() {
                info!("Embedding service ready (placeholder mode)");
            } else {
                info!(
                    providers = ?providers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
                    "Embedding service ready"
                );
            }
            *initialized = true;
        }
        Ok(())
    }

    /// Generate embeddings for multiple texts.
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        self.ensure_initialized().await?;

        let providers = {
            let guard = self.inner.providers.read().await;
            guard.clone()
        };

        // Use hash fallback if no providers
        if providers.is_empty() {
            debug!(count = texts.len(), "Generating hash-based placeholder embeddings");
            let dim = *self.inner.dimension.read().await;
            return Ok(texts.iter().map(|t| self.hash_embed(t, dim)).collect());
        }

        debug!(count = texts.len(), "Generating API embeddings");

        // Try each provider with fallback
        let mut last_error = None;

        for provider in &providers {
            if !provider.has_credentials() {
                debug!(provider = %provider.name, "Skipping provider without credentials");
                continue;
            }

            match self.try_provider_batch(&provider, &texts).await {
                Ok(embeddings) => {
                    // Update last used timestamp
                    if !provider.id.is_empty() {
                        if let Some(db) = &self.inner.db {
                            let _ = update_embedding_provider_last_used(db, &provider.id).await;
                        }
                    }
                    return Ok(embeddings);
                }
                Err(e) => {
                    warn!(
                        provider = %provider.name,
                        error = %e,
                        "Embedding provider failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        // When providers are configured but ALL fail, return error (don't use hash fallback)
        // This ensures indexing stops and waits for providers to come back online
        Err(last_error.unwrap_or_else(|| {
            Error::Internal("All embedding providers failed".to_string())
        }))
    }

    /// Generate embedding for a single text.
    pub async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        self.ensure_initialized().await?;

        let providers = {
            let guard = self.inner.providers.read().await;
            guard.clone()
        };

        if providers.is_empty() {
            let dim = *self.inner.dimension.read().await;
            return Ok(self.hash_embed(text, dim));
        }

        let mut last_error = None;

        for provider in &providers {
            if !provider.has_credentials() {
                continue;
            }

            match self.try_provider_single(&provider, text).await {
                Ok(embedding) => {
                    if !provider.id.is_empty() {
                        if let Some(db) = &self.inner.db {
                            let _ = update_embedding_provider_last_used(db, &provider.id).await;
                        }
                    }
                    return Ok(embedding);
                }
                Err(e) => {
                    warn!(
                        provider = %provider.name,
                        error = %e,
                        "Embedding provider failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        // When providers are configured but ALL fail, return error
        Err(last_error.unwrap_or_else(|| {
            Error::Internal("All embedding providers failed".to_string())
        }))
    }

    /// Generate embeddings in batches for large inputs.
    pub async fn embed_batch(
        &self,
        texts: Vec<String>,
        batch_size: usize,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let batch_size = batch_size.min(MAX_BATCH_SIZE);
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(batch_size) {
            let embeddings = self.embed(chunk.to_vec()).await?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    /// Try a provider for batch embedding with retries.
    async fn try_provider_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let mut delay = Duration::from_millis(RETRY_DELAY_MS);

        for attempt in 0..MAX_RETRIES {
            match self.call_provider_batch(provider, texts).await {
                Ok(embeddings) => return Ok(embeddings),
                Err(e) => {
                    if Self::is_retryable(&e) && attempt < MAX_RETRIES - 1 {
                        debug!(
                            provider = %provider.name,
                            attempt,
                            delay_ms = delay.as_millis(),
                            "Retrying after error"
                        );
                        sleep(delay).await;
                        delay *= 2;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(Error::Internal("Max retries exceeded".to_string()))
    }

    /// Try a provider for single embedding with retries.
    async fn try_provider_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        let mut delay = Duration::from_millis(RETRY_DELAY_MS);

        for attempt in 0..MAX_RETRIES {
            match self.call_provider_single(provider, text).await {
                Ok(embedding) => return Ok(embedding),
                Err(e) => {
                    if Self::is_retryable(&e) && attempt < MAX_RETRIES - 1 {
                        debug!(
                            provider = %provider.name,
                            attempt,
                            delay_ms = delay.as_millis(),
                            "Retrying after error"
                        );
                        sleep(delay).await;
                        delay *= 2;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(Error::Internal("Max retries exceeded".to_string()))
    }

    /// Call provider's batch embedding API
    async fn call_provider_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        match provider.name.as_str() {
            "gemini" => self.call_gemini_batch(provider, texts).await,
            "openai" => self.call_openai_batch(provider, texts).await,
            _ => Err(Error::Internal(format!(
                "Unknown embedding provider: {}",
                provider.name
            ))),
        }
    }

    /// Call provider's single embedding API
    async fn call_provider_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        match provider.name.as_str() {
            "gemini" => self.call_gemini_single(provider, text).await,
            "openai" => self.call_openai_single(provider, text).await,
            _ => Err(Error::Internal(format!(
                "Unknown embedding provider: {}",
                provider.name
            ))),
        }
    }

    /// Call Gemini embedding API for single text
    async fn call_gemini_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        let auth_token = provider
            .auth_token()
            .ok_or_else(|| Error::Internal("No credentials".to_string()))?;

        let url = format!(
            "{}/models/{}:embedContent?key={}",
            provider.base_url, provider.model, auth_token
        );

        let body = json!({
            "model": format!("models/{}", provider.model),
            "content": {
                "parts": [{"text": text}]
            }
        });

        let response = self
            .inner
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Gemini request failed: {}", e)))?;

        let status = response.status();
        let resp: GeminiEmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse Gemini response: {}", e)))?;

        if let Some(error) = resp.error {
            return Err(Error::Internal(format!(
                "Gemini error ({}): {}",
                error.code.unwrap_or(status.as_u16() as i32),
                error.message
            )));
        }

        resp.embedding
            .map(|e| e.values)
            .ok_or_else(|| Error::Internal("No embedding in Gemini response".to_string()))
    }

    /// Call Gemini batch embedding API
    async fn call_gemini_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let auth_token = provider
            .auth_token()
            .ok_or_else(|| Error::Internal("No credentials".to_string()))?;

        let url = format!(
            "{}/models/{}:batchEmbedContents?key={}",
            provider.base_url, provider.model, auth_token
        );

        let requests: Vec<_> = texts
            .iter()
            .map(|text| {
                json!({
                    "model": format!("models/{}", provider.model),
                    "content": {
                        "parts": [{"text": text}]
                    }
                })
            })
            .collect();

        let body = json!({ "requests": requests });

        let response = self
            .inner
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Gemini batch request failed: {}", e)))?;

        let status = response.status();
        let resp: GeminiBatchResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse Gemini batch response: {}", e)))?;

        if let Some(error) = resp.error {
            return Err(Error::Internal(format!(
                "Gemini batch error ({}): {}",
                error.code.unwrap_or(status.as_u16() as i32),
                error.message
            )));
        }

        resp.embeddings
            .map(|embs| embs.into_iter().map(|e| e.values).collect())
            .ok_or_else(|| Error::Internal("No embeddings in Gemini batch response".to_string()))
    }

    /// Call OpenAI embedding API for single text
    async fn call_openai_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        let auth_token = provider
            .auth_token()
            .ok_or_else(|| Error::Internal("No credentials".to_string()))?;

        let url = format!("{}/embeddings", provider.base_url);

        let body = json!({
            "model": provider.model,
            "input": text
        });

        let response = self
            .inner
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("OpenAI request failed: {}", e)))?;

        let resp: OpenAIEmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse OpenAI response: {}", e)))?;

        if let Some(error) = resp.error {
            return Err(Error::Internal(format!("OpenAI error: {}", error.message)));
        }

        resp.data
            .and_then(|d| d.into_iter().next())
            .map(|e| e.embedding)
            .ok_or_else(|| Error::Internal("No embedding in OpenAI response".to_string()))
    }

    /// Call OpenAI batch embedding API
    async fn call_openai_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let auth_token = provider
            .auth_token()
            .ok_or_else(|| Error::Internal("No credentials".to_string()))?;

        let url = format!("{}/embeddings", provider.base_url);

        let body = json!({
            "model": provider.model,
            "input": texts
        });

        let response = self
            .inner
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("OpenAI batch request failed: {}", e)))?;

        let resp: OpenAIEmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse OpenAI batch response: {}", e)))?;

        if let Some(error) = resp.error {
            return Err(Error::Internal(format!(
                "OpenAI batch error: {}",
                error.message
            )));
        }

        let mut data = resp
            .data
            .ok_or_else(|| Error::Internal("No embeddings in OpenAI batch response".to_string()))?;

        // Sort by index to ensure correct order
        data.sort_by_key(|e| e.index);

        Ok(data.into_iter().map(|e| e.embedding).collect())
    }

    /// Check if an error is retryable (rate limit, temporary failure)
    fn is_retryable(error: &Error) -> bool {
        let msg = error.to_string().to_lowercase();
        msg.contains("rate")
            || msg.contains("limit")
            || msg.contains("429")
            || msg.contains("503")
            || msg.contains("timeout")
            || msg.contains("temporarily")
    }

    /// Generate a deterministic embedding from text using hashing.
    /// This is NOT semantic - just a fallback for development/testing.
    fn hash_embed(&self, text: &str, dim: usize) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut embedding = vec![0.0f32; dim];

        // Use multiple hash seeds to fill the embedding
        for i in 0..dim {
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            (i as u64).hash(&mut hasher);
            let hash = hasher.finish();

            // Convert to float in [-1, 1] range
            embedding[i] = ((hash as f64 / u64::MAX as f64) * 2.0 - 1.0) as f32;
        }

        // Normalize to unit length
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        embedding
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmbeddingConfig;

    fn test_config() -> EmbeddingConfig {
        EmbeddingConfig {
            providers: vec![],
            dimension: 384,
        }
    }

    #[test]
    fn test_hash_embed_deterministic() {
        let service = EmbeddingService::from_config(&test_config()).unwrap();

        let emb1 = service.hash_embed("test text", 384);
        let emb2 = service.hash_embed("test text", 384);

        assert_eq!(emb1, emb2);
        assert_eq!(emb1.len(), 384);
    }

    #[test]
    fn test_hash_embed_normalized() {
        let service = EmbeddingService::from_config(&test_config()).unwrap();

        let emb = service.hash_embed("test text", 384);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Should be approximately 1.0 (unit vector)
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_no_providers_uses_fallback() {
        let config = test_config();
        let service = EmbeddingService::from_config(&config).unwrap();

        assert!(!service.has_providers().await);
        assert_eq!(service.dimension().await, 384);
    }

    #[tokio::test]
    async fn test_embed_empty_returns_empty() {
        let service = EmbeddingService::from_config(&test_config()).unwrap();
        let result = service.embed(vec![]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_embed_uses_hash_fallback() {
        let service = EmbeddingService::from_config(&test_config()).unwrap();
        let texts = vec!["hello".to_string(), "world".to_string()];

        let result = service.embed(texts).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 384);
        assert_eq!(result[1].len(), 384);
    }

    #[test]
    fn test_default_dimensions() {
        assert_eq!(default_dimension("text-embedding-001"), 768);
        assert_eq!(default_dimension("text-embedding-3-small"), 1536);
        assert_eq!(default_dimension("text-embedding-3-large"), 3072);
        assert_eq!(default_dimension("unknown-model"), 384);
    }
}
