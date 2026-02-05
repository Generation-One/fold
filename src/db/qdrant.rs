//! Qdrant vector database client placeholder.
//!
//! The actual Qdrant client implementation lives in `crate::services::vector`.
//! This module exists for organizational consistency and provides a re-export
//! path from the db module.
//!
//! # Usage
//!
//! For vector operations, use the services module directly:
//!
//! ```ignore
//! use fold::services::vector::{VectorStore, QdrantStore};
//!
//! let store = QdrantStore::new(&config).await?;
//! store.upsert_memory(&memory, &embedding).await?;
//! let results = store.search(&query_embedding, 10).await?;
//! ```
//!
//! # Why separate from SQLite?
//!
//! Qdrant is a specialized vector database that handles:
//! - High-dimensional vector storage and similarity search
//! - Approximate nearest neighbor (ANN) algorithms
//! - Filtering and metadata queries on vectors
//!
//! SQLite handles:
//! - Relational data and complex queries
//! - ACID transactions
//! - Full-text search (via FTS5)
//!
//! Keeping them separate allows independent scaling and optimization.

/// Placeholder for future Qdrant-related types that may be shared
/// between the db and services layers.
pub mod types {
    use serde::{Deserialize, Serialize};

    /// Vector embedding dimensions.
    /// fastembed's default model produces 384-dimensional vectors.
    pub const DEFAULT_EMBEDDING_DIM: usize = 384;

    /// Similarity metric for vector search.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum SimilarityMetric {
        Cosine,
        Euclidean,
        DotProduct,
    }

    impl Default for SimilarityMetric {
        fn default() -> Self {
            Self::Cosine
        }
    }

    /// Search result from vector store.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VectorSearchResult {
        /// Memory ID
        pub id: String,
        /// Similarity score (0.0 to 1.0 for cosine)
        pub score: f32,
        /// Optional payload/metadata
        pub payload: Option<serde_json::Value>,
    }

    /// Configuration for vector store operations.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VectorStoreConfig {
        /// Qdrant server URL
        pub url: String,
        /// Collection name prefix
        pub collection_prefix: String,
        /// Embedding dimension
        pub embedding_dim: usize,
        /// Similarity metric
        pub similarity_metric: SimilarityMetric,
        /// Enable on-disk storage (vs in-memory)
        pub on_disk: bool,
    }

    impl Default for VectorStoreConfig {
        fn default() -> Self {
            Self {
                url: "http://localhost:6333".to_string(),
                collection_prefix: "fold".to_string(),
                embedding_dim: DEFAULT_EMBEDDING_DIM,
                similarity_metric: SimilarityMetric::Cosine,
                on_disk: true,
            }
        }
    }

    /// Filter for vector search.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct VectorFilter {
        /// Filter by project ID
        pub project_id: Option<String>,
        /// Filter by memory type(s)
        pub memory_types: Option<Vec<String>>,
        /// Filter by author
        pub author: Option<String>,
        /// Filter by tags (any match)
        pub tags: Option<Vec<String>>,
        /// Minimum score threshold
        pub min_score: Option<f32>,
    }
}

// Re-export types at module level for convenience
