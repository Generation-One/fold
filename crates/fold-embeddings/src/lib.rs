//! Embedding service with multi-provider fallback.
//!
//! Supports Gemini, OpenAI, and Ollama embedding APIs with automatic fallback
//! when rate limits are hit or providers fail. Falls back to hash-based
//! placeholders when no providers are configured.
//!
//! # Example
//!
//! ```no_run
//! use fold_embeddings::{EmbeddingService, EmbeddingConfig, EmbeddingProviderConfig};
//!
//! # async fn example() -> Result<(), fold_embeddings::Error> {
//! let config = EmbeddingConfig {
//!     providers: vec![
//!         EmbeddingProviderConfig {
//!             name: "gemini".to_string(),
//!             base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
//!             model: "text-embedding-004".to_string(),
//!             api_key: "your-api-key".to_string(),
//!             priority: 1,
//!         },
//!     ],
//!     dimension: 768,
//! };
//!
//! let service = EmbeddingService::from_config(&config)?;
//! let embeddings = service.embed(vec!["hello world".to_string()]).await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Maximum retries per provider before fallback
const MAX_RETRIES: u32 = 2;

/// Delay between retries (doubles each time)
const RETRY_DELAY_MS: u64 = 500;

/// Maximum texts per batch for API calls
const MAX_BATCH_SIZE: usize = 100;

// ============================================================================
// Error types
// ============================================================================

/// Errors that can occur in the embedding service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Internal error (HTTP client, parsing, etc.)
    #[error("Internal error: {0}")]
    Internal(String),

    /// Provider API error
    #[error("Provider error: {0}")]
    Provider(String),

    /// No credentials configured
    #[error("No credentials configured")]
    NoCredentials,

    /// All providers failed
    #[error("All embedding providers failed")]
    AllProvidersFailed,
}

/// Result type for embedding operations.
pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Configuration types
// ============================================================================

/// Configuration for the embedding service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// List of embedding providers in priority order.
    pub providers: Vec<EmbeddingProviderConfig>,
    /// Default embedding dimension.
    pub dimension: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            dimension: 768,
        }
    }
}

/// Configuration for a single embedding provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingProviderConfig {
    /// Provider name (e.g., "gemini", "openai", "ollama").
    pub name: String,
    /// Base URL for the API.
    pub base_url: String,
    /// Model name to use.
    pub model: String,
    /// API key for authentication.
    pub api_key: String,
    /// Priority (lower = higher priority).
    pub priority: u8,
}

// ============================================================================
// Runtime provider
// ============================================================================

/// Runtime provider configuration with optional OAuth support.
#[derive(Debug, Clone)]
pub struct RuntimeEmbeddingProvider {
    /// Unique identifier (empty for config-based providers).
    pub id: String,
    /// Provider name (e.g., "gemini", "openai", "ollama").
    pub name: String,
    /// Base URL for the API.
    pub base_url: String,
    /// Model name to use.
    pub model: String,
    /// API key for authentication.
    pub api_key: Option<String>,
    /// OAuth access token (alternative to API key).
    pub oauth_access_token: Option<String>,
    /// Embedding dimension (if known).
    pub dimension: Option<usize>,
    /// Priority (lower = higher priority).
    pub priority: i32,
}

impl RuntimeEmbeddingProvider {
    /// Get the authentication token (prefers OAuth, falls back to API key).
    pub fn auth_token(&self) -> Option<&str> {
        self.oauth_access_token
            .as_deref()
            .or(self.api_key.as_deref())
    }

    /// Check if provider has valid credentials.
    pub fn has_credentials(&self) -> bool {
        self.api_key.is_some() || self.oauth_access_token.is_some()
    }
}

impl From<&EmbeddingProviderConfig> for RuntimeEmbeddingProvider {
    fn from(config: &EmbeddingProviderConfig) -> Self {
        Self {
            id: String::new(),
            name: config.name.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: if config.api_key.is_empty() {
                None
            } else {
                Some(config.api_key.clone())
            },
            oauth_access_token: None,
            dimension: Some(default_dimension(&config.model)),
            priority: config.priority as i32,
        }
    }
}

// ============================================================================
// Default values
// ============================================================================

/// Get default endpoint for a provider.
pub fn default_endpoint(name: &str) -> String {
    match name {
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        "openai" => "https://api.openai.com/v1".to_string(),
        "ollama" => "http://localhost:11434".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

/// Get default model for a provider.
pub fn default_model(name: &str) -> String {
    match name {
        "gemini" => "text-embedding-004".to_string(),
        "openai" => "text-embedding-3-small".to_string(),
        "ollama" => "nomic-embed-text".to_string(),
        _ => "text-embedding-3-small".to_string(),
    }
}

/// Get default dimension for a model.
pub fn default_dimension(model: &str) -> usize {
    if model.contains("text-embedding-004") || model.contains("embedding-001") {
        768
    } else if model.contains("text-embedding-3-small") {
        1536
    } else if model.contains("text-embedding-3-large") {
        3072
    } else if model.contains("text-embedding-ada-002") {
        1536
    } else if model.contains("nomic-embed-text") {
        768
    } else if model.contains("all-minilm") {
        384
    } else if model.contains("all-mpnet") {
        768
    } else if model.contains("bge-large") || model.contains("mxbai-embed-large") {
        1024
    } else if model.contains("bge-base") || model.contains("bge-small") {
        768
    } else if model.contains("jina-embeddings-v2-base") {
        768
    } else if model.contains("jina-embeddings-v2-small") {
        512
    } else if model.contains("MiniLM-L6") {
        384
    } else if model.contains("mpnet") {
        768
    } else {
        384 // Default
    }
}

// ============================================================================
// API response types
// ============================================================================

/// Gemini embedding response.
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

/// Gemini batch embedding response.
#[derive(Debug, Deserialize)]
struct GeminiBatchResponse {
    embeddings: Option<Vec<GeminiEmbedding>>,
    error: Option<GeminiError>,
}

/// OpenAI embedding response.
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

/// Ollama embedding response (single).
#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embedding: Option<Vec<f32>>,
    error: Option<String>,
}

/// Ollama batch embedding response.
#[derive(Debug, Deserialize)]
struct OllamaBatchResponse {
    embeddings: Option<Vec<Vec<f32>>>,
    error: Option<String>,
}

// ============================================================================
// Callback trait for database integration
// ============================================================================

/// Callback trait for database operations.
///
/// Implement this trait to integrate with your database for provider management.
#[async_trait::async_trait]
pub trait EmbeddingCallbacks: Send + Sync {
    /// Called after a provider is successfully used.
    async fn on_provider_used(&self, provider_id: &str);
}

/// No-op implementation for when no callbacks are needed.
pub struct NoOpCallbacks;

#[async_trait::async_trait]
impl EmbeddingCallbacks for NoOpCallbacks {
    async fn on_provider_used(&self, _provider_id: &str) {}
}

// ============================================================================
// Embedding service
// ============================================================================

/// Service for generating text embeddings with multi-provider fallback.
///
/// Tries providers in priority order (Gemini -> OpenAI -> Ollama),
/// automatically falling back on rate limits or failures.
/// Uses hash-based placeholders when no providers are configured.
#[derive(Clone)]
pub struct EmbeddingService {
    inner: Arc<EmbeddingServiceInner>,
}

struct EmbeddingServiceInner {
    providers: RwLock<Vec<RuntimeEmbeddingProvider>>,
    dimension: RwLock<usize>,
    client: Client,
    initialized: RwLock<bool>,
    callbacks: Option<Arc<dyn EmbeddingCallbacks>>,
}

impl EmbeddingService {
    /// Create a new embedding service from configuration.
    pub fn from_config(config: &EmbeddingConfig) -> Result<Self> {
        Self::from_config_with_callbacks(config, None)
    }

    /// Create a new embedding service from configuration with callbacks.
    pub fn from_config_with_callbacks(
        config: &EmbeddingConfig,
        callbacks: Option<Arc<dyn EmbeddingCallbacks>>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        let providers: Vec<RuntimeEmbeddingProvider> = config
            .providers
            .iter()
            .map(RuntimeEmbeddingProvider::from)
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
                providers: RwLock::new(providers),
                dimension: RwLock::new(config.dimension),
                client,
                initialized: RwLock::new(false),
                callbacks,
            }),
        })
    }

    /// Create a new embedding service with runtime providers.
    ///
    /// Use this when loading providers from a database.
    pub fn from_providers(
        providers: Vec<RuntimeEmbeddingProvider>,
        dimension: usize,
        callbacks: Option<Arc<dyn EmbeddingCallbacks>>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        if providers.is_empty() {
            warn!(
                dimension = dimension,
                "No embedding providers configured - using hash-based placeholders"
            );
        } else {
            info!(
                providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
                dimension = dimension,
                "Embedding service initialized from providers"
            );
        }

        Ok(Self {
            inner: Arc::new(EmbeddingServiceInner {
                providers: RwLock::new(providers),
                dimension: RwLock::new(dimension),
                client,
                initialized: RwLock::new(false),
                callbacks,
            }),
        })
    }

    /// Update the providers at runtime.
    pub async fn set_providers(&self, providers: Vec<RuntimeEmbeddingProvider>) {
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
            "Updated embedding providers"
        );

        let mut guard = self.inner.providers.write().await;
        *guard = providers;
    }

    /// Get the embedding dimension.
    pub async fn dimension(&self) -> usize {
        *self.inner.dimension.read().await
    }

    /// Get provider names in priority order.
    pub async fn providers(&self) -> Vec<String> {
        let guard = self.inner.providers.read().await;
        guard.iter().map(|p| p.name.clone()).collect()
    }

    /// Check if real embedding providers are available.
    pub async fn has_providers(&self) -> bool {
        let guard = self.inner.providers.read().await;
        !guard.is_empty()
    }

    /// Lazily initialize the service.
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
            debug!(
                count = texts.len(),
                "Generating hash-based placeholder embeddings"
            );
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

            match self.try_provider_batch(provider, &texts).await {
                Ok(embeddings) => {
                    // Notify callback of successful use
                    if !provider.id.is_empty() {
                        if let Some(callbacks) = &self.inner.callbacks {
                            callbacks.on_provider_used(&provider.id).await;
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
        Err(last_error.unwrap_or(Error::AllProvidersFailed))
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

            match self.try_provider_single(provider, text).await {
                Ok(embedding) => {
                    if !provider.id.is_empty() {
                        if let Some(callbacks) = &self.inner.callbacks {
                            callbacks.on_provider_used(&provider.id).await;
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
        Err(last_error.unwrap_or(Error::AllProvidersFailed))
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

    /// Call provider's batch embedding API.
    async fn call_provider_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        match provider.name.as_str() {
            "gemini" => self.call_gemini_batch(provider, texts).await,
            "openai" => self.call_openai_batch(provider, texts).await,
            "ollama" => self.call_ollama_batch(provider, texts).await,
            _ => Err(Error::Internal(format!(
                "Unknown embedding provider: {}",
                provider.name
            ))),
        }
    }

    /// Call provider's single embedding API.
    async fn call_provider_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        match provider.name.as_str() {
            "gemini" => self.call_gemini_single(provider, text).await,
            "openai" => self.call_openai_single(provider, text).await,
            "ollama" => self.call_ollama_single(provider, text).await,
            _ => Err(Error::Internal(format!(
                "Unknown embedding provider: {}",
                provider.name
            ))),
        }
    }

    /// Call Gemini embedding API for single text.
    async fn call_gemini_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        let auth_token = provider.auth_token().ok_or(Error::NoCredentials)?;

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
            return Err(Error::Provider(format!(
                "Gemini error ({}): {}",
                error.code.unwrap_or(status.as_u16() as i32),
                error.message
            )));
        }

        resp.embedding
            .map(|e| e.values)
            .ok_or_else(|| Error::Internal("No embedding in Gemini response".to_string()))
    }

    /// Call Gemini batch embedding API.
    async fn call_gemini_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let auth_token = provider.auth_token().ok_or(Error::NoCredentials)?;

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
        let resp: GeminiBatchResponse = response.json().await.map_err(|e| {
            Error::Internal(format!("Failed to parse Gemini batch response: {}", e))
        })?;

        if let Some(error) = resp.error {
            return Err(Error::Provider(format!(
                "Gemini batch error ({}): {}",
                error.code.unwrap_or(status.as_u16() as i32),
                error.message
            )));
        }

        resp.embeddings
            .map(|embs| embs.into_iter().map(|e| e.values).collect())
            .ok_or_else(|| Error::Internal("No embeddings in Gemini batch response".to_string()))
    }

    /// Call OpenAI embedding API for single text.
    async fn call_openai_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        let auth_token = provider.auth_token().ok_or(Error::NoCredentials)?;

        let url = format!("{}/embeddings", provider.base_url);

        let body = json!({
            "model": provider.model,
            "input": text,
            "dimensions": 768
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
            return Err(Error::Provider(format!("OpenAI error: {}", error.message)));
        }

        resp.data
            .and_then(|d| d.into_iter().next())
            .map(|e| e.embedding)
            .ok_or_else(|| Error::Internal("No embedding in OpenAI response".to_string()))
    }

    /// Call OpenAI batch embedding API.
    async fn call_openai_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let auth_token = provider.auth_token().ok_or(Error::NoCredentials)?;

        let url = format!("{}/embeddings", provider.base_url);

        let body = json!({
            "model": provider.model,
            "input": texts,
            "dimensions": 768
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

        let resp: OpenAIEmbedResponse = response.json().await.map_err(|e| {
            Error::Internal(format!("Failed to parse OpenAI batch response: {}", e))
        })?;

        if let Some(error) = resp.error {
            return Err(Error::Provider(format!(
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

    /// Call Ollama embedding API for a single text.
    async fn call_ollama_single(
        &self,
        provider: &RuntimeEmbeddingProvider,
        text: &str,
    ) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", provider.base_url);

        let body = json!({
            "model": provider.model,
            "prompt": text
        });

        let response = self
            .inner
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Ollama request failed: {}", e)))?;

        let resp: OllamaEmbedResponse = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse Ollama response: {}", e)))?;

        if let Some(error) = resp.error {
            return Err(Error::Provider(format!("Ollama error: {}", error)));
        }

        resp.embedding
            .ok_or_else(|| Error::Internal("No embedding in Ollama response".to_string()))
    }

    /// Call Ollama batch embedding API (supports both batch and single requests).
    async fn call_ollama_batch(
        &self,
        provider: &RuntimeEmbeddingProvider,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embeddings", provider.base_url);

        // Use batch API with prompts array for better performance
        let body = json!({
            "model": provider.model,
            "prompts": texts
        });

        let response = self
            .inner
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Ollama batch request failed: {}", e)))?;

        // Parse response flexibly to handle both batch and single formats
        let resp: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse Ollama response: {}", e)))?;

        // Check for error field
        if let Some(error) = resp.get("error").and_then(|e| e.as_str()) {
            return Err(Error::Provider(format!("Ollama error: {}", error)));
        }

        // Try batch embeddings response first
        if let Some(embeddings) = resp.get("embeddings") {
            return serde_json::from_value::<Vec<Vec<f32>>>(embeddings.clone())
                .map_err(|e| Error::Internal(format!("Failed to parse embeddings: {}", e)));
        }

        // Fallback to single embedding format for compatibility
        if let Some(embedding) = resp.get("embedding") {
            if let Ok(emb) = serde_json::from_value::<Vec<f32>>(embedding.clone()) {
                return Ok(vec![emb]);
            }
        }

        Err(Error::Internal(
            "No embeddings in Ollama response".to_string(),
        ))
    }

    /// Check if an error is retryable (rate limit, temporary failure).
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
    pub fn hash_embed(&self, text: &str, dim: usize) -> Vec<f32> {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(default_dimension("text-embedding-004"), 768);
        assert_eq!(default_dimension("text-embedding-3-small"), 1536);
        assert_eq!(default_dimension("text-embedding-3-large"), 3072);
        assert_eq!(default_dimension("unknown-model"), 384);
    }

    #[test]
    fn test_default_endpoints() {
        assert_eq!(
            default_endpoint("gemini"),
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(default_endpoint("openai"), "https://api.openai.com/v1");
        assert_eq!(default_endpoint("ollama"), "http://localhost:11434");
    }

    #[test]
    fn test_default_models() {
        assert_eq!(default_model("gemini"), "text-embedding-004");
        assert_eq!(default_model("openai"), "text-embedding-3-small");
        assert_eq!(default_model("ollama"), "nomic-embed-text");
    }
}
