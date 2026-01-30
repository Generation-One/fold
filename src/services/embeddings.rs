//! Embedding service for generating vector embeddings.
//!
//! Currently uses a placeholder implementation. Enable the `fastembed` feature
//! for local ONNX-based embeddings when running in Docker.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::EmbeddingConfig;
use crate::error::{Error, Result};

/// Service for generating text embeddings.
///
/// By default uses a hash-based placeholder. Enable fastembed feature for
/// real semantic embeddings.
#[derive(Clone)]
pub struct EmbeddingService {
    inner: Arc<EmbeddingServiceInner>,
}

struct EmbeddingServiceInner {
    model_name: String,
    dimension: usize,
    initialized: RwLock<bool>,
}

impl EmbeddingService {
    /// Create a new embedding service with the specified model.
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let model_name = config.model.clone();
        let dimension = Self::model_dimension(&model_name);

        warn!(
            model = %model_name,
            dimension,
            "Using placeholder embeddings - enable fastembed for semantic search"
        );

        Ok(Self {
            inner: Arc::new(EmbeddingServiceInner {
                model_name,
                dimension,
                initialized: RwLock::new(false),
            }),
        })
    }

    /// Get the embedding dimension for a model
    fn model_dimension(model_name: &str) -> usize {
        // Common sentence-transformer model dimensions
        if model_name.contains("MiniLM-L6") {
            384
        } else if model_name.contains("MiniLM-L12") {
            384
        } else if model_name.contains("mpnet") {
            768
        } else if model_name.contains("e5-small") {
            384
        } else if model_name.contains("e5-base") {
            768
        } else if model_name.contains("e5-large") {
            1024
        } else if model_name.contains("bge-small") {
            384
        } else if model_name.contains("bge-base") {
            768
        } else if model_name.contains("bge-large") {
            1024
        } else {
            // Default to MiniLM dimension
            384
        }
    }

    /// Get the embedding dimension
    pub fn dimension(&self) -> usize {
        self.inner.dimension
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.inner.model_name
    }

    /// Lazily initialize the model
    async fn ensure_initialized(&self) -> Result<()> {
        let mut initialized = self.inner.initialized.write().await;
        if !*initialized {
            info!(model = %self.inner.model_name, "Embedding service ready (placeholder mode)");
            *initialized = true;
        }
        Ok(())
    }

    /// Generate a deterministic embedding from text using hashing.
    /// This is NOT semantic - just a placeholder for development.
    fn hash_embed(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let dim = self.inner.dimension;
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

    /// Generate embeddings for multiple texts.
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        self.ensure_initialized().await?;

        debug!(count = texts.len(), "Generating placeholder embeddings");

        let embeddings: Vec<Vec<f32>> = texts
            .iter()
            .map(|text| self.hash_embed(text))
            .collect();

        Ok(embeddings)
    }

    /// Generate embedding for a single text.
    pub async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        self.ensure_initialized().await?;
        Ok(self.hash_embed(text))
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

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(batch_size) {
            let embeddings = self.embed(chunk.to_vec()).await?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_dimension() {
        assert_eq!(
            EmbeddingService::model_dimension("sentence-transformers/all-MiniLM-L6-v2"),
            384
        );
        assert_eq!(
            EmbeddingService::model_dimension("BAAI/bge-large-en-v1.5"),
            1024
        );
    }

    #[test]
    fn test_hash_embed_deterministic() {
        let config = EmbeddingConfig {
            model: "all-MiniLM-L6-v2".to_string(),
        };
        let service = EmbeddingService::new(&config).unwrap();

        let emb1 = service.hash_embed("test text");
        let emb2 = service.hash_embed("test text");

        assert_eq!(emb1, emb2);
        assert_eq!(emb1.len(), 384);
    }

    #[test]
    fn test_hash_embed_normalized() {
        let config = EmbeddingConfig {
            model: "all-MiniLM-L6-v2".to_string(),
        };
        let service = EmbeddingService::new(&config).unwrap();

        let emb = service.hash_embed("test text");
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Should be approximately 1.0 (unit vector)
        assert!((norm - 1.0).abs() < 0.001);
    }
}
