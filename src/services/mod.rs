//! Service layer for Fold.
//!
//! Contains business logic and external service integrations:
//! - Embeddings (local via fastembed)
//! - LLM (multi-provider with fallback)
//! - Qdrant (vector storage)
//! - Memory (high-level operations)
//! - Project (project management)
//! - Indexer (codebase indexing)
//! - FileSource (abstraction for file providers)
//! - GitHub/GitLab (git provider APIs)
//! - GitSync (webhook processing)
//! - Graph (relationship queries)
//! - Linker (auto-linking)
//! - Auth (OIDC flows)

mod auth;
mod embeddings;
pub mod file_source;
mod git_sync;
mod github;
mod gitlab;
pub mod graph;
mod indexer;
mod job_worker;
mod linker;
mod llm;
mod memory;
mod project;
mod qdrant;

pub use auth::AuthService;
pub use embeddings::EmbeddingService;
pub use file_source::{
    ChangeEvent, FileContent, FileInfo, FileSourceProvider, GitHubFileSource,
    GoogleDriveFileSource, NotificationConfig, ProviderRegistry, SourceConfig, SourceInfo,
};
pub use git_sync::GitSyncService;
pub use github::GitHubService;
pub use gitlab::GitLabService;
pub use graph::{
    AffectedMemory, GraphEdge, GraphNode, GraphResult, GraphService, GraphStats, ImpactAnalysis,
    MemoryContext, RelatedMemory,
};
pub use indexer::IndexerService;
pub use job_worker::{JobWorker, JobWorkerHandle, JobWorkerStatus};
pub use linker::LinkerService;
pub use llm::LlmService;
pub use memory::MemoryService;
pub use project::ProjectService;
pub use qdrant::QdrantService;
