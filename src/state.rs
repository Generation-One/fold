//! Application state for Fold.
//!
//! Contains the shared state that is passed to all handlers.

use std::sync::Arc;

use crate::db::DbPool;
use crate::services::{
    AuthService, EmbeddingService, GitHubService, GitLabService, GitSyncService,
    GraphService, IndexerService, LinkerService, LlmService, MemoryService,
    ProjectService, QdrantService,
};
use crate::{config, Error, Result};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub db: DbPool,
    /// Qdrant vector database service.
    pub qdrant: Arc<QdrantService>,
    /// Embedding generation service.
    pub embeddings: Arc<EmbeddingService>,
    /// LLM completion service.
    pub llm: Arc<LlmService>,
    /// GitHub API service.
    pub github: Arc<GitHubService>,
    /// GitLab API service.
    pub gitlab: Arc<GitLabService>,
    /// Memory management service.
    pub memory: MemoryService,
    /// Project management service.
    pub project: ProjectService,
    /// Codebase indexer service.
    pub indexer: IndexerService,
    /// Git sync service for webhooks.
    pub git_sync: GitSyncService,
    /// Knowledge graph service.
    pub graph: GraphService,
    /// Auto-linking service.
    pub linker: LinkerService,
    /// Authentication service.
    pub auth: AuthService,
}

impl AppState {
    /// Create a new application state, initializing all services.
    pub async fn new() -> Result<Self> {
        let config = config::config();

        // Initialize database
        let db = crate::db::init_pool(&config.database.path).await?;

        // Run migrations
        crate::db::migrate(&db).await?;

        // Initialize core services
        let qdrant = Arc::new(QdrantService::new(&config.qdrant).await?);
        let embeddings = Arc::new(EmbeddingService::new(&config.embedding)?);
        let llm = Arc::new(LlmService::new(&config.llm));
        let github = Arc::new(GitHubService::new());
        let gitlab = Arc::new(GitLabService::new());

        // Initialize high-level services
        let memory = MemoryService::new(
            db.clone(),
            qdrant.clone(),
            embeddings.clone(),
            llm.clone(),
        );

        let project = ProjectService::new(
            db.clone(),
            qdrant.clone(),
            embeddings.clone(),
        );

        let indexer = IndexerService::new(memory.clone(), llm.clone());

        let git_sync = GitSyncService::new(
            db.clone(),
            github.clone(),
            gitlab.clone(),
            memory.clone(),
            llm.clone(),
            indexer.clone(),
        );

        let graph = GraphService::new(db.clone());

        let linker = LinkerService::new(
            db.clone(),
            memory.clone(),
            llm.clone(),
            qdrant.clone(),
            embeddings.clone(),
        );

        let auth = AuthService::new(db.clone(), config.auth.clone());

        Ok(Self {
            db,
            qdrant,
            embeddings,
            llm,
            github,
            gitlab,
            memory,
            project,
            indexer,
            git_sync,
            graph,
            linker,
            auth,
        })
    }
}
