//! Bridge module for fold-embeddings integration with fold-core.
//!
//! Wraps the standalone fold-embeddings crate and provides database integration
//! for provider management, OAuth tokens, and usage tracking.

use std::sync::Arc;

use fold_embeddings::{
    default_dimension, default_endpoint, default_model, EmbeddingCallbacks,
    EmbeddingConfig as BaseEmbeddingConfig, EmbeddingProviderConfig,
    EmbeddingService as BaseEmbeddingService, RuntimeEmbeddingProvider,
};
use serde_json::json;
use tracing::{info, warn};

use crate::config::EmbeddingConfig;
use crate::db::{
    list_enabled_embedding_providers, seed_embedding_providers_from_env,
    update_embedding_provider_last_used, DbPool, EmbeddingProviderRow,
};
use crate::error::{Error, Result};

/// Database-backed callbacks for the embedding service.
struct DbCallbacks {
    db: DbPool,
}

#[async_trait::async_trait]
impl EmbeddingCallbacks for DbCallbacks {
    async fn on_provider_used(&self, provider_id: &str) {
        let _ = update_embedding_provider_last_used(&self.db, provider_id).await;
    }
}

/// Convert a database provider row to a runtime provider.
fn row_to_runtime_provider(row: EmbeddingProviderRow) -> RuntimeEmbeddingProvider {
    let config = row.config_json().unwrap_or(json!({}));

    RuntimeEmbeddingProvider {
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

/// Service for generating text embeddings with multi-provider fallback.
///
/// This is a wrapper around `fold_embeddings::EmbeddingService` that adds
/// database integration for provider management.
#[derive(Clone)]
pub struct EmbeddingService {
    inner: BaseEmbeddingService,
    db: Option<DbPool>,
}

impl EmbeddingService {
    /// Create a new embedding service with database-backed providers.
    ///
    /// On first run, seeds providers from environment variables.
    pub async fn new(db: DbPool, config: &EmbeddingConfig) -> Result<Self> {
        // Try to load providers from database
        let db_providers = list_enabled_embedding_providers(&db).await?;

        let (providers, dimension) = if db_providers.is_empty() {
            // Seed from environment variables on first run
            if !config.providers.is_empty() {
                info!("Seeding embedding providers from environment variables");
                seed_embedding_providers_from_env(&db, &config.providers).await?;
                let seeded = list_enabled_embedding_providers(&db).await?;
                let providers: Vec<RuntimeEmbeddingProvider> =
                    seeded.into_iter().map(row_to_runtime_provider).collect();

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
            let providers: Vec<RuntimeEmbeddingProvider> = db_providers
                .into_iter()
                .map(row_to_runtime_provider)
                .collect();

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

        let callbacks: Option<Arc<dyn EmbeddingCallbacks>> =
            Some(Arc::new(DbCallbacks { db: db.clone() }));

        let inner = BaseEmbeddingService::from_providers(providers, dimension, callbacks)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self {
            inner,
            db: Some(db),
        })
    }

    /// Create embedding service from config only (for backwards compatibility).
    pub fn from_config(config: &EmbeddingConfig) -> Result<Self> {
        let base_config = BaseEmbeddingConfig {
            providers: config
                .providers
                .iter()
                .map(|p| EmbeddingProviderConfig {
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    model: p.model.clone(),
                    api_key: p.api_key.clone(),
                    priority: p.priority,
                })
                .collect(),
            dimension: config.dimension,
        };

        let inner = BaseEmbeddingService::from_config(&base_config)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self { inner, db: None })
    }

    /// Reload providers from the database.
    pub async fn refresh_providers(&self) -> Result<()> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| Error::Internal("No database connection".to_string()))?;

        let db_providers = list_enabled_embedding_providers(db).await?;
        let providers: Vec<RuntimeEmbeddingProvider> = db_providers
            .into_iter()
            .map(row_to_runtime_provider)
            .collect();

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "Refreshed embedding providers from database"
        );

        self.inner.set_providers(providers).await;

        Ok(())
    }

    /// Get the embedding dimension.
    pub async fn dimension(&self) -> usize {
        self.inner.dimension().await
    }

    /// Get provider names in priority order.
    pub async fn providers(&self) -> Vec<String> {
        self.inner.providers().await
    }

    /// Check if real embedding providers are available.
    pub async fn has_providers(&self) -> bool {
        self.inner.has_providers().await
    }

    /// Generate embeddings for multiple texts.
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        self.inner
            .embed(texts)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Generate embedding for a single text.
    pub async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        self.inner
            .embed_single(text)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Generate embeddings in batches for large inputs.
    pub async fn embed_batch(
        &self,
        texts: Vec<String>,
        batch_size: usize,
    ) -> Result<Vec<Vec<f32>>> {
        self.inner
            .embed_batch(texts, batch_size)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
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

    #[tokio::test]
    async fn test_from_config_no_providers() {
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
}
