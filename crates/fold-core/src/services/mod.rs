//! Service layer for Fold.
//!
//! Contains business logic and external service integrations:
//! - Embeddings (local via fastembed)
//! - LLM (multi-provider with fallback)
//! - Qdrant (vector storage)
//! - Memory (agentic memory with LLM-powered evolution)
//! - FoldStorage (hash-based storage in fold/ directory)
//! - Project (project management)
//! - Indexer (codebase indexing)
//! - Index (SQLite index management with rebuild capability)
//! - Git (auto-commit and sync for fold/ directory)
//! - MetaStorage (filesystem-centric memory storage - legacy)
//! - ContentResolver (resolve memory content from external storage)
//! - FileSource (abstraction for file providers)
//! - GitHub/GitLab (git provider APIs)
//! - GitSync (webhook processing)
//! - Graph (relationship queries)
//! - Linker (auto-linking)
//! - Auth (OIDC flows)
//! - AttachmentStorage (content-addressed file storage)

mod attachment_storage;
mod auth;
mod claudecode;
mod content_resolver;
pub mod decay;
mod embeddings_bridge;
pub mod file_source;
pub mod fold_storage;
mod git;
mod git_local;
mod git_sync;
mod github;
mod gitlab;
pub mod graph;
mod index;
mod indexer;
mod job_worker;
mod linker;
mod llm;
pub mod markdown;
mod memory;
mod meta_storage;
mod metadata_sync;
mod permissions;
mod project;

pub use auth::AuthService;
pub use fold_chunker::{ChunkStrategy, ChunkerConfig, ChunkerService, CodeChunk};
pub use claudecode::{ClaudeCodeInfo, ClaudeCodeService};
pub use content_resolver::ContentResolverService;
pub use embeddings_bridge::EmbeddingService;
pub use fold_embeddings::{
    default_dimension, default_endpoint, default_model, EmbeddingCallbacks, EmbeddingConfig,
    EmbeddingProviderConfig, Error as EmbeddingError, NoOpCallbacks, RuntimeEmbeddingProvider,
};
pub use file_source::{FileSourceProvider, ProviderRegistry};
pub use fold_storage::{
    storage_error_to_error, storage_memory_to_memory, FoldStorageExt, FoldStorageService,
    MemoryData, MemoryFrontmatter, StorageMemory,
};
pub use git::GitService;
pub use git_local::GitLocalService;
pub use git_sync::GitSyncService;
pub use github::GitHubService;
pub use gitlab::GitLabService;
pub use graph::GraphService;
pub use indexer::IndexerService;
pub use job_worker::JobWorker;
pub use linker::LinkerService;
pub use llm::LlmService;
// Re-export fold-llm types for external use
pub use fold_llm::{
    default_endpoint as llm_default_endpoint, default_model as llm_default_model,
    Error as LlmError, GeneratedMetadata, LlmConfig, LlmProviderConfig, RuntimeLlmProvider,
};
pub use memory::MemoryService;
pub use meta_storage::MetaStorageService;
pub use metadata_sync::MetadataSyncService;
pub use permissions::{PermissionService, ProjectAccess};
pub use project::ProjectService;
pub use fold_qdrant::{QdrantService, SearchFilter, VectorSearchResult, CollectionInfo};
