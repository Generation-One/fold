//! Service layer for Fold.
//!
//! Contains business logic and external service integrations:
//! - Embeddings (local via fastembed)
//! - LLM (multi-provider with fallback)
//! - Qdrant (vector storage)
//! - Memory (high-level operations)
//! - Project (project management)
//! - Indexer (codebase indexing)
//! - Index (SQLite index management with rebuild capability)
//! - MetaStorage (filesystem-centric memory storage)
//! - FileSource (abstraction for file providers)
//! - GitHub/GitLab (git provider APIs)
//! - GitSync (webhook processing)
//! - Graph (relationship queries)
//! - Linker (auto-linking)
//! - Auth (OIDC flows)
//! - AttachmentStorage (content-addressed file storage)

mod attachment_storage;
mod auth;
pub mod decay;
mod embeddings;
pub mod file_source;
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
mod project;
mod qdrant;

pub use attachment_storage::AttachmentStorageService;
pub use auth::AuthService;
pub use decay::{
    calculate_strength, blend_scores, rerank_by_combined_score, DecayConfig, ScoredResult,
    DEFAULT_HALF_LIFE_DAYS, DEFAULT_STRENGTH_WEIGHT,
};
pub use embeddings::EmbeddingService;
pub use file_source::{
    ChangeEvent, FileContent, FileInfo, FileSourceProvider, GitHubFileSource,
    GoogleDriveFileSource, LocalFileSource, NotificationConfig, ProviderRegistry, SourceConfig,
    SourceInfo,
};
pub use git_sync::GitSyncService;
pub use github::GitHubService;
pub use gitlab::GitLabService;
pub use graph::{
    AffectedMemory, GraphEdge, GraphNode, GraphResult, GraphService, GraphStats, ImpactAnalysis,
    MemoryContext, RelatedMemory,
};
pub use index::{IndexHealth, IndexService, RebuildStats};
pub use indexer::IndexerService;
pub use job_worker::{JobWorker, JobWorkerHandle, JobWorkerStatus};
pub use linker::LinkerService;
pub use llm::LlmService;
pub use markdown::{FrontmatterAttachment, FrontmatterLink, MarkdownService, MemoryFrontmatter};
pub use memory::MemoryService;
pub use meta_storage::MetaStorageService;
pub use project::ProjectService;
pub use qdrant::QdrantService;
