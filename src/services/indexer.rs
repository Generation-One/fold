//! Indexer service for codebase indexing.
//!
//! Scans project directories, detects languages, and indexes source files
//! with LLM-generated summaries. Summaries are written to fold/a/b/hash.md
//! using hash-based storage for deduplication.
//!
//! Also extracts semantic chunks (functions, classes, sections) using tree-sitter
//! AST parsing for code and heading-based splitting for markdown. Chunks are
//! stored in SQLite and Qdrant for fine-grained search.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chrono::Utc;
use futures::stream::{self, StreamExt};
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::db::{self, DbPool};
use crate::error::{Error, Result};
use crate::models::{ChunkCreate, Memory, MemoryCreate, MemorySource, MemoryType, Project};

use super::{
    ChunkerService, EmbeddingService, FoldStorageService, GitService, LinkerService, LlmService,
    MemoryService, QdrantService,
};

/// Maximum file size to index (100KB)
const MAX_FILE_SIZE: usize = 100_000;

/// Language detection by extension
const LANGUAGE_MAP: &[(&str, &str)] = &[
    (".py", "python"),
    (".js", "javascript"),
    (".ts", "typescript"),
    (".tsx", "typescript"),
    (".jsx", "javascript"),
    (".java", "java"),
    (".go", "go"),
    (".rs", "rust"),
    (".rb", "ruby"),
    (".php", "php"),
    (".swift", "swift"),
    (".kt", "kotlin"),
    (".c", "c"),
    (".cpp", "cpp"),
    (".h", "c"),
    (".hpp", "cpp"),
    (".cs", "csharp"),
    (".sql", "sql"),
    (".sh", "bash"),
    (".yml", "yaml"),
    (".yaml", "yaml"),
    (".json", "json"),
    (".md", "markdown"),
    (".html", "html"),
    (".css", "css"),
    (".scss", "scss"),
    (".vue", "vue"),
    (".svelte", "svelte"),
];

/// Default concurrency limit for indexing
const DEFAULT_CONCURRENCY: usize = 4;

/// Service for indexing codebases into memories.
#[derive(Clone)]
pub struct IndexerService {
    memory_service: MemoryService,
    llm: Arc<LlmService>,
    fold_storage: Arc<FoldStorageService>,
    git_service: Option<Arc<GitService>>,
    linker: Option<Arc<LinkerService>>,
    file_hashes: Arc<tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>>,
    /// Maximum number of files to index in parallel
    concurrency_limit: usize,
    /// Chunker service for extracting semantic code chunks
    chunker: Arc<ChunkerService>,
    /// Embedding service for vectorizing chunks
    embedding: Option<Arc<EmbeddingService>>,
    /// Qdrant service for storing chunk vectors
    qdrant: Option<Arc<QdrantService>>,
    /// Database pool for storing chunks
    db: Option<DbPool>,
}

/// Progress callback for indexing
pub type ProgressCallback = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Result of indexing operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexResult {
    pub total_files: usize,
    pub indexed_files: usize,
    pub skipped_files: usize,
    pub errors: usize,
    pub duration_seconds: f64,
}

impl IndexerService {
    /// Create a new indexer service.
    pub fn new(memory_service: MemoryService, llm: Arc<LlmService>) -> Self {
        Self {
            memory_service,
            llm,
            fold_storage: Arc::new(FoldStorageService::new()),
            git_service: None,
            linker: None,
            file_hashes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            concurrency_limit: DEFAULT_CONCURRENCY,
            chunker: Arc::new(ChunkerService::new()),
            embedding: None,
            qdrant: None,
            db: None,
        }
    }

    /// Create a new indexer service with a specific fold storage service.
    pub fn with_fold_storage(
        memory_service: MemoryService,
        llm: Arc<LlmService>,
        fold_storage: Arc<FoldStorageService>,
    ) -> Self {
        Self {
            memory_service,
            llm,
            fold_storage,
            git_service: None,
            linker: None,
            file_hashes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            concurrency_limit: DEFAULT_CONCURRENCY,
            chunker: Arc::new(ChunkerService::new()),
            embedding: None,
            qdrant: None,
            db: None,
        }
    }

    /// Create a new indexer service with git integration for auto-commit.
    pub fn with_git_service(
        memory_service: MemoryService,
        llm: Arc<LlmService>,
        git_service: Arc<GitService>,
    ) -> Self {
        Self {
            memory_service,
            llm,
            fold_storage: Arc::new(FoldStorageService::new()),
            git_service: Some(git_service),
            linker: None,
            file_hashes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            concurrency_limit: DEFAULT_CONCURRENCY,
            chunker: Arc::new(ChunkerService::new()),
            embedding: None,
            qdrant: None,
            db: None,
        }
    }

    /// Set the git service for auto-commit functionality.
    pub fn set_git_service(&mut self, git_service: Arc<GitService>) {
        self.git_service = Some(git_service);
    }

    /// Set the linker service for auto-linking memories.
    pub fn set_linker(&mut self, linker: Arc<LinkerService>) {
        self.linker = Some(linker);
    }

    /// Set the concurrency limit for parallel file indexing.
    pub fn set_concurrency_limit(&mut self, limit: usize) {
        self.concurrency_limit = limit.max(1); // Ensure at least 1
    }

    /// Set services required for chunking (embedding, qdrant, and db).
    pub fn set_chunk_services(
        &mut self,
        embedding: Arc<EmbeddingService>,
        qdrant: Arc<QdrantService>,
        db: DbPool,
    ) {
        self.embedding = Some(embedding);
        self.qdrant = Some(qdrant);
        self.db = Some(db);
    }

    /// Check if chunking services are configured.
    pub fn chunking_enabled(&self) -> bool {
        self.embedding.is_some() && self.qdrant.is_some() && self.db.is_some()
    }

    /// Detect programming language from file extension.
    pub fn detect_language(path: &str) -> String {
        let path = Path::new(path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_default();

        LANGUAGE_MAP
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, lang)| lang.to_string())
            .unwrap_or_default()
    }

    /// Check if a path matches any of the glob patterns.
    fn matches_patterns(path: &str, patterns: &[String]) -> bool {
        for pattern in patterns {
            if glob::Pattern::new(pattern)
                .map(|p| p.matches(path))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    /// Generate a full SHA256 hash of file content for change detection.
    /// Returns the full 64-char hex hash.
    fn content_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result)
    }

    /// Generate a stable memory ID from project slug and file path.
    /// This ensures the same file always maps to the same memory ID,
    /// allowing proper updates when file content changes.
    /// The ID is consistent across machines (uses slug + normalised relative path).
    pub fn path_hash(project_slug: &str, file_path: &str) -> String {
        let mut hasher = Sha256::new();
        // Normalise path separators to forward slashes for cross-platform consistency
        let normalised_path = file_path.replace('\\', "/");
        hasher.update(format!("{}/{}", project_slug, normalised_path).as_bytes());
        let result = hasher.finalize();
        let full_hash = hex::encode(&result);
        // Format first 32 chars as UUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)
        format!(
            "{}-{}-{}-{}-{}",
            &full_hash[0..8],
            &full_hash[8..12],
            &full_hash[12..16],
            &full_hash[16..20],
            &full_hash[20..32]
        )
    }

    /// Index a project's codebase.
    pub async fn index_project(
        &self,
        project: &Project,
        author: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> Result<IndexResult> {
        let root_path = project
            .root_path
            .as_ref()
            .ok_or_else(|| Error::Validation("Project has no root_path configured".to_string()))?;

        let root = PathBuf::from(root_path);
        if !root.exists() {
            return Err(Error::NotFound(format!(
                "Project root path does not exist: {}",
                root_path
            )));
        }

        let start_time = Utc::now();
        let mut stats = IndexResult {
            total_files: 0,
            indexed_files: 0,
            skipped_files: 0,
            errors: 0,
            duration_seconds: 0.0,
        };

        // Find all matching files
        let index_patterns = project.index_patterns_vec();
        let ignore_patterns = project.ignore_patterns_vec();

        let files = self
            .find_files(&root, &index_patterns, &ignore_patterns)
            .await?;

        stats.total_files = files.len();
        info!(
            project = %project.slug,
            files = files.len(),
            concurrency = self.concurrency_limit,
            "Found files to index (parallel)"
        );

        // Use atomic counters for thread-safe stats
        let indexed_count = Arc::new(AtomicUsize::new(0));
        let skipped_count = Arc::new(AtomicUsize::new(0));
        let error_count = Arc::new(AtomicUsize::new(0));
        let processed_count = Arc::new(AtomicUsize::new(0));

        let total_files = files.len();
        let progress = Arc::new(progress);

        // Process files in parallel with concurrency limit
        stream::iter(files.into_iter().enumerate())
            .for_each_concurrent(self.concurrency_limit, |(_, file_path)| {
                let indexed_count = Arc::clone(&indexed_count);
                let skipped_count = Arc::clone(&skipped_count);
                let error_count = Arc::clone(&error_count);
                let processed_count = Arc::clone(&processed_count);
                let progress = Arc::clone(&progress);
                let project = project.clone();
                let root = root.clone();
                let author = author.map(String::from);

                async move {
                    match self
                        .index_file(&file_path, &project, &root, author.as_deref())
                        .await
                    {
                        Ok(indexed) => {
                            if indexed {
                                indexed_count.fetch_add(1, Ordering::Relaxed);
                            } else {
                                skipped_count.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(e) => {
                            warn!(
                                file = %file_path.display(),
                                error = %e,
                                "Error indexing file"
                            );
                            error_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }

                    let completed = processed_count.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(ref callback) = *progress {
                        callback(completed, total_files, &file_path.display().to_string());
                    }
                }
            })
            .await;

        // Extract final counts
        stats.indexed_files = indexed_count.load(Ordering::Relaxed);
        stats.skipped_files = skipped_count.load(Ordering::Relaxed);
        stats.errors = error_count.load(Ordering::Relaxed);

        let duration = (Utc::now() - start_time).num_milliseconds() as f64 / 1000.0;
        stats.duration_seconds = duration;

        info!(
            project = %project.slug,
            indexed = stats.indexed_files,
            total = stats.total_files,
            errors = stats.errors,
            duration_s = duration,
            "Indexing completed"
        );

        // Auto-commit fold/ changes if git service is available and files were indexed
        if stats.indexed_files > 0 {
            if let Some(ref git_service) = self.git_service {
                if project.auto_commit_enabled() {
                    let commit_message = format!(
                        "fold: Index {} files from {}",
                        stats.indexed_files, project.slug
                    );
                    match git_service.auto_commit_fold(&root, &commit_message).await {
                        Ok(result) => {
                            if result.committed {
                                info!(
                                    project = %project.slug,
                                    sha = ?result.sha,
                                    "Auto-committed fold/ changes"
                                );
                            }
                        }
                        Err(e) => {
                            warn!(
                                project = %project.slug,
                                error = %e,
                                "Failed to auto-commit fold/ changes"
                            );
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    /// Find all files matching the patterns.
    async fn find_files(
        &self,
        root: &Path,
        include_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.walk_dir(root, root, include_patterns, exclude_patterns, &mut files)
            .await?;
        Ok(files)
    }

    /// Recursively walk directory.
    async fn walk_dir(
        &self,
        root: &Path,
        current: &Path,
        include: &[String],
        exclude: &[String],
        files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let mut entries = fs::read_dir(current).await.map_err(|e| {
            Error::Internal(format!(
                "Failed to read directory {}: {}",
                current.display(),
                e
            ))
        })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read entry: {}", e)))?
        {
            let path = entry.path();
            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            // Check exclusions first
            if Self::matches_patterns(&rel_path, exclude) {
                continue;
            }

            if path.is_dir() {
                Box::pin(self.walk_dir(root, &path, include, exclude, files)).await?;
            } else if path.is_file() && Self::matches_patterns(&rel_path, include) {
                files.push(path);
            }
        }

        Ok(())
    }

    /// Index a single file.
    ///
    /// Uses content hash for:
    /// 1. Change detection - skip unchanged files
    /// 2. Memory ID - derive from first 16 chars of hash
    /// 3. Fold storage path - fold/a/b/hash.md
    async fn index_file(
        &self,
        file_path: &Path,
        project: &Project,
        root: &Path,
        author: Option<&str>,
    ) -> Result<bool> {
        let content = fs::read_to_string(file_path).await.map_err(|e| {
            Error::Internal(format!("Failed to read {}: {}", file_path.display(), e))
        })?;

        // Skip empty files
        if content.trim().is_empty() {
            return Ok(false);
        }

        // Skip large files
        if content.len() > MAX_FILE_SIZE {
            debug!(file = %file_path.display(), "Skipping large file");
            return Ok(false);
        }

        let rel_path = file_path
            .strip_prefix(root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .replace('\\', "/");

        let language = Self::detect_language(&rel_path);

        // Calculate content hash for change detection
        let content_hash_value = Self::content_hash(&content);

        // Generate stable memory ID from path (consistent across machines, survives content changes)
        let memory_id = Self::path_hash(&project.slug, &rel_path);

        // Check if file has changed using in-memory cache
        {
            let hashes = self.file_hashes.read().await;
            if let Some(project_hashes) = hashes.get(&project.slug) {
                if project_hashes.get(&rel_path) == Some(&content_hash_value) {
                    debug!(file = %rel_path, "Skipping unchanged file");
                    return Ok(false);
                }
            }
        }

        // Generate summary using LLM - fail if LLM is unavailable (no dumb fallbacks)
        if !self.llm.is_available().await {
            return Err(Error::Llm(
                "LLM service is unavailable - cannot index without summarization".to_string(),
            ));
        }

        let code_summary = self
            .llm
            .summarize_code(&content, &rel_path, &language)
            .await?;

        let title = code_summary.title;
        let summary_content = code_summary.summary;
        let keywords = code_summary.keywords;
        let tags = code_summary.tags;
        let created_date = code_summary.created_date;

        // Ensure we got a real summary, not empty
        if summary_content.trim().is_empty() {
            return Err(Error::Llm(
                "LLM returned empty summary - cannot index without proper summarization"
                    .to_string(),
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "content_hash".to_string(),
            serde_json::Value::String(content_hash_value.clone()),
        );
        metadata.insert(
            "file_size".to_string(),
            serde_json::Value::Number(content.len().into()),
        );
        metadata.insert(
            "line_count".to_string(),
            serde_json::Value::Number((content.lines().count()).into()),
        );
        if let Some(ref date) = created_date {
            metadata.insert(
                "original_date".to_string(),
                serde_json::Value::String(date.clone()),
            );
        }

        let create = MemoryCreate {
            id: Some(memory_id),
            memory_type: MemoryType::Codebase,
            content: summary_content.clone(),
            author: author.map(String::from),
            title: Some(title),
            keywords,
            tags,
            context: Some(format!("Source file: {}", rel_path)),
            file_path: Some(rel_path.clone()),
            language: if language.is_empty() {
                None
            } else {
                Some(language.clone())
            },
            source: Some(MemorySource::File),
            metadata,
            ..Default::default()
        };

        // Add memory to database and vector store
        let memory = self
            .memory_service
            .add(&project.id, &project.slug, create, false)
            .await?;

        // Auto-link to related memories for holographic context
        // NOTE: memory.add() already calls process_memory_evolution() which handles evolution-based linking.
        // The linker service provides ADDITIONAL semantic similarity-based linking.
        if let Some(ref linker) = self.linker {
            info!(memory_id = %memory.id, "Starting auto-link for memory");
            match linker
                .auto_link(&project.id, &project.slug, &memory.id, 0.3)
                .await
            {
                Ok(result) => {
                    info!(
                        memory_id = %memory.id,
                        links_created = result.links_created,
                        suggestions = result.suggestions.len(),
                        "Auto-link completed"
                    );
                }
                Err(e) => {
                    warn!(memory_id = %memory.id, error = %e, "Auto-linking failed");
                }
            }
        } else {
            debug!(memory_id = %memory.id, "No linker configured, skipping auto-link");
        }

        // NOTE: The fold file write is handled by memory.add() via FoldStorageService.
        // We removed the duplicate write here that was erasing the related_to links.
        // The memory.add() call above already writes to fold/ with evolution-based links.

        // Extract and store semantic chunks for fine-grained search
        if self.chunking_enabled() {
            match self
                .process_chunks(&memory.id, &project.id, &project.slug, &content, &language)
                .await
            {
                Ok(chunk_count) => {
                    if chunk_count > 0 {
                        debug!(
                            memory_id = %memory.id,
                            chunks = chunk_count,
                            "Extracted and stored chunks"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        memory_id = %memory.id,
                        error = %e,
                        "Failed to process chunks (non-fatal)"
                    );
                }
            }
        }

        // Update in-memory hash cache
        {
            let mut hashes = self.file_hashes.write().await;
            let project_hashes = hashes
                .entry(project.slug.clone())
                .or_insert_with(HashMap::new);
            project_hashes.insert(rel_path, content_hash_value);
        }

        Ok(true)
    }

    /// Index a single file by path (for webhook-triggered updates).
    ///
    /// Uses path-based hash for memory ID (stable across content changes) and writes summary to fold/.
    pub async fn index_single_file(
        &self,
        project: &Project,
        file_path: &str,
        content: &str,
        author: Option<&str>,
    ) -> Result<Memory> {
        let language = Self::detect_language(file_path);

        // Calculate content hash for change detection and metadata
        let content_hash_value = Self::content_hash(content);

        // Generate stable memory ID from path
        let memory_id = Self::path_hash(&project.slug, file_path);

        // Generate summary using LLM - fail if LLM is unavailable (no dumb fallbacks)
        if !self.llm.is_available().await {
            return Err(Error::Llm(
                "LLM service is unavailable - cannot index without summarization".to_string(),
            ));
        }

        let code_summary = self
            .llm
            .summarize_code(content, file_path, &language)
            .await?;

        let title = code_summary.title;
        let summary_content = code_summary.summary;
        let keywords = code_summary.keywords;
        let tags = code_summary.tags;

        // Ensure we got a real summary, not empty
        if summary_content.trim().is_empty() {
            return Err(Error::Llm(
                "LLM returned empty summary - cannot index without proper summarization"
                    .to_string(),
            ));
        }

        // Include hash and file stats in metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "content_hash".to_string(),
            serde_json::Value::String(content_hash_value),
        );
        metadata.insert(
            "file_size".to_string(),
            serde_json::Value::Number(content.len().into()),
        );
        metadata.insert(
            "line_count".to_string(),
            serde_json::Value::Number((content.lines().count()).into()),
        );

        let create = MemoryCreate {
            id: Some(memory_id),
            memory_type: MemoryType::Codebase,
            content: summary_content.clone(),
            author: author.map(String::from),
            title: Some(title),
            keywords,
            tags,
            context: Some(format!("Source file: {}", file_path)),
            file_path: Some(file_path.to_string()),
            language: if language.is_empty() {
                None
            } else {
                Some(language.clone())
            },
            source: Some(MemorySource::Agent),
            metadata,
            ..Default::default()
        };

        // Add memory to database and vector store
        let memory = self
            .memory_service
            .add(&project.id, &project.slug, create, false)
            .await?;

        // Auto-link to related memories for holographic context
        if let Some(ref linker) = self.linker {
            info!(memory_id = %memory.id, "Starting auto-link for memory");
            match linker
                .auto_link(&project.id, &project.slug, &memory.id, 0.3)
                .await
            {
                Ok(result) => {
                    info!(
                        memory_id = %memory.id,
                        links_created = result.links_created,
                        suggestions = result.suggestions.len(),
                        "Auto-link completed"
                    );
                }
                Err(e) => {
                    warn!(memory_id = %memory.id, error = %e, "Auto-linking failed");
                }
            }
        } else {
            debug!(memory_id = %memory.id, "No linker configured, skipping auto-link");
        }

        // NOTE: The fold file write is handled by memory.add() via FoldStorageService.
        // We removed the duplicate write here that was erasing the related_to links.

        // Extract and store semantic chunks for fine-grained search
        if self.chunking_enabled() {
            match self
                .process_chunks(&memory.id, &project.id, &project.slug, content, &language)
                .await
            {
                Ok(chunk_count) => {
                    if chunk_count > 0 {
                        debug!(
                            memory_id = %memory.id,
                            chunks = chunk_count,
                            "Extracted and stored chunks"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        memory_id = %memory.id,
                        error = %e,
                        "Failed to process chunks (non-fatal)"
                    );
                }
            }
        }

        Ok(memory)
    }

    /// Extract semantic chunks from content and store in DB + Qdrant.
    ///
    /// Returns the number of chunks stored.
    async fn process_chunks(
        &self,
        memory_id: &str,
        project_id: &str,
        project_slug: &str,
        content: &str,
        language: &str,
    ) -> Result<usize> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| Error::Internal("Database not configured for chunking".to_string()))?;
        let embedding = self.embedding.as_ref().ok_or_else(|| {
            Error::Internal("Embedding service not configured for chunking".to_string())
        })?;
        let qdrant = self.qdrant.as_ref().ok_or_else(|| {
            Error::Internal("Qdrant service not configured for chunking".to_string())
        })?;

        // Extract chunks using the chunker service
        let code_chunks = self.chunker.chunk(content, language);

        if code_chunks.is_empty() {
            debug!(memory_id = %memory_id, "No chunks extracted");
            return Ok(0);
        }

        debug!(
            memory_id = %memory_id,
            chunk_count = code_chunks.len(),
            language = %language,
            "Extracted chunks from content"
        );

        // Delete any existing chunks for this memory (handles updates)
        let deleted = db::delete_chunks_for_memory(db, memory_id).await?;
        if deleted > 0 {
            debug!(memory_id = %memory_id, deleted = deleted, "Deleted existing chunks");

            // Also delete from Qdrant - filter by parent_memory_id
            // We'll use a prefix convention: chunk IDs start with "chunk-{memory_id}-"
            // For now, we'll skip Qdrant deletion as it's handled by the upsert
        }

        // Convert code chunks to ChunkCreate structs
        let creates: Vec<ChunkCreate> = code_chunks
            .iter()
            .map(|c| ChunkCreate {
                memory_id: memory_id.to_string(),
                project_id: project_id.to_string(),
                content: c.content.clone(),
                start_line: c.start_line as i32,
                end_line: c.end_line as i32,
                start_byte: c.start_byte as i32,
                end_byte: c.end_byte as i32,
                node_type: c.node_type.clone(),
                node_name: c.node_name.clone(),
                language: language.to_string(),
            })
            .collect();

        // Insert chunks into SQLite
        let chunks = db::insert_chunks(db, creates).await?;
        let chunk_count = chunks.len();

        // Prepare for batch embedding
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

        // Generate embeddings for all chunks
        let embeddings = match embedding.embed(texts).await {
            Ok(embs) => embs,
            Err(e) => {
                warn!(
                    memory_id = %memory_id,
                    error = %e,
                    "Failed to generate chunk embeddings"
                );
                return Ok(chunk_count); // Still return chunk count - DB storage succeeded
            }
        };

        // Prepare Qdrant points
        let points: Vec<(String, Vec<f32>, HashMap<String, serde_json::Value>)> = chunks
            .iter()
            .zip(embeddings.into_iter())
            .map(|(chunk, vector)| {
                let mut payload = HashMap::new();
                payload.insert(
                    "type".to_string(),
                    serde_json::Value::String("chunk".to_string()),
                );
                payload.insert(
                    "parent_memory_id".to_string(),
                    serde_json::Value::String(chunk.memory_id.clone()),
                );
                payload.insert(
                    "project_id".to_string(),
                    serde_json::Value::String(chunk.project_id.clone()),
                );
                payload.insert(
                    "node_type".to_string(),
                    serde_json::Value::String(chunk.node_type.clone()),
                );
                if let Some(ref name) = chunk.node_name {
                    payload.insert(
                        "node_name".to_string(),
                        serde_json::Value::String(name.clone()),
                    );
                }
                payload.insert(
                    "start_line".to_string(),
                    serde_json::Value::Number(chunk.start_line.into()),
                );
                payload.insert(
                    "end_line".to_string(),
                    serde_json::Value::Number(chunk.end_line.into()),
                );
                payload.insert(
                    "language".to_string(),
                    serde_json::Value::String(chunk.language.clone()),
                );

                (chunk.id.clone(), vector, payload)
            })
            .collect();

        // Store in Qdrant
        if let Err(e) = qdrant.upsert_batch(project_slug, points).await {
            warn!(
                memory_id = %memory_id,
                error = %e,
                "Failed to store chunk vectors in Qdrant"
            );
            // Don't fail - DB storage succeeded
        } else {
            info!(
                memory_id = %memory_id,
                chunks = chunk_count,
                "Stored chunk vectors in Qdrant"
            );
        }

        Ok(chunk_count)
    }

    /// Search indexed codebase.
    pub async fn search_code(
        &self,
        project_id: &str,
        project_slug: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<crate::models::MemorySearchResult>> {
        self.memory_service
            .search(project_id, project_slug, query, limit)
            .await
    }

    /// Clear file hash cache for a project.
    pub async fn clear_cache(&self, project_slug: &str) {
        let mut hashes = self.file_hashes.write().await;
        hashes.remove(project_slug);
    }
}
