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
mod embeddings;
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
mod project;
mod qdrant;

pub use auth::AuthService;
pub use claudecode::{ClaudeCodeInfo, ClaudeCodeService};
pub use content_resolver::ContentResolverService;
pub use embeddings::EmbeddingService;
pub use file_source::{
    FileSourceProvider, ProviderRegistry,
};
pub use fold_storage::FoldStorageService;
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
pub use memory::MemoryService;
pub use meta_storage::MetaStorageService;
pub use metadata_sync::MetadataSyncService;
pub use project::ProjectService;
pub use qdrant::QdrantService;
