//! Service layer for Fold.
//!
//! Contains business logic and external service integrations:
//! - Embeddings (local via fastembed)
//! - LLM (multi-provider with fallback)
//! - Qdrant (vector storage)
//! - Memory (high-level operations)
//! - Project (project management)
//! - Indexer (codebase indexing)
//! - GitHub/GitLab (git provider APIs)
//! - GitSync (webhook processing)
//! - Graph (relationship queries)
//! - Linker (auto-linking)
//! - Auth (OIDC flows)

mod auth;
mod embeddings;
mod git_sync;
mod github;
mod gitlab;
mod graph;
mod indexer;
mod linker;
mod llm;
mod memory;
mod project;
mod qdrant;

pub use auth::AuthService;
pub use embeddings::EmbeddingService;
pub use git_sync::GitSyncService;
pub use github::GitHubService;
pub use gitlab::GitLabService;
pub use graph::GraphService;
pub use indexer::IndexerService;
pub use linker::LinkerService;
pub use llm::LlmService;
pub use memory::MemoryService;
pub use project::ProjectService;
pub use qdrant::QdrantService;
