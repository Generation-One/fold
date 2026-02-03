//! Application state for Fold.
//!
//! Contains the shared state that is passed to all handlers.

use std::sync::Arc;

use crate::db::DbPool;
use crate::services::{
    AuthService, ContentResolverService, EmbeddingService, FoldStorageService, GitHubService,
    GitLabService, GitLocalService, GitService, GitSyncService, GraphService, IndexerService,
    LinkerService, LlmService, MemoryService, MetaStorageService, ProjectService, ProviderRegistry,
    QdrantService,
};
use std::path::PathBuf;
use crate::{config, Result};

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
    /// File source provider registry.
    pub providers: Arc<ProviderRegistry>,
    /// Memory management service (agentic).
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
    /// Local git operations service.
    pub git_local: Arc<GitLocalService>,
    /// Git integration service for auto-commit and sync.
    pub git_service: Arc<GitService>,
    /// Content resolver for external memory storage (legacy).
    pub content_resolver: Arc<ContentResolverService>,
    /// Fold storage service for hash-based memory storage.
    pub fold_storage: Arc<FoldStorageService>,
}

impl AppState {
    /// Create a new application state, initializing all services.
    pub async fn new() -> Result<Self> {
        let config = config::config();

        // Initialize database
        let db = crate::db::init_pool(&config.database.path).await?;

        // Initialize database schema
        crate::db::initialize_schema(&db).await?;

        // Initialize core services
        let qdrant = Arc::new(QdrantService::new(&config.qdrant).await?);
        let embeddings = Arc::new(EmbeddingService::new(db.clone(), &config.embedding).await?);
        let llm = Arc::new(LlmService::new(db.clone(), &config.llm).await?);
        let github = Arc::new(GitHubService::new());
        let gitlab = Arc::new(GitLabService::new());
        let git_local = Arc::new(GitLocalService::new());
        let providers = Arc::new(ProviderRegistry::with_defaults());

        // Initialize filesystem storage services
        let meta_storage = Arc::new(MetaStorageService::new(PathBuf::from(&config.storage.fold_path)));
        let content_resolver = Arc::new(ContentResolverService::new(db.clone(), meta_storage.clone()));
        let fold_storage = Arc::new(FoldStorageService::new());

        // Initialize high-level services with agentic memory
        let memory = MemoryService::new(
            db.clone(),
            qdrant.clone(),
            embeddings.clone(),
            llm.clone(),
            fold_storage.clone(),
        );

        let project = ProjectService::new(
            db.clone(),
            qdrant.clone(),
            embeddings.clone(),
        );

        // Initialize git service for auto-commit and sync
        let git_service = Arc::new(GitService::new(
            db.clone(),
            memory.clone(),
            fold_storage.clone(),
            qdrant.clone(),
            embeddings.clone(),
        ));

        // Initialize indexer with git service for auto-commit
        let indexer = IndexerService::with_git_service(
            memory.clone(),
            llm.clone(),
            git_service.clone(),
        );

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
            git_local,
            git_service,
            providers,
            memory,
            project,
            indexer,
            git_sync,
            graph,
            linker,
            auth,
            content_resolver,
            fold_storage,
        })
    }
}
