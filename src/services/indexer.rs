//! Indexer service for codebase indexing.
//!
//! Scans project directories, detects languages, and indexes source files
//! with LLM-generated summaries. Summaries are written to fold/a/b/hash.md
//! using hash-based storage for deduplication.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use sha2::{Sha256, Digest};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::error::{Error, Result};
use crate::models::{Memory, MemoryCreate, MemoryType, Project};

use super::{FoldStorageService, GitService, LlmService, MemoryService};

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

/// Service for indexing codebases into memories.
#[derive(Clone)]
pub struct IndexerService {
    memory_service: MemoryService,
    llm: Arc<LlmService>,
    fold_storage: Arc<FoldStorageService>,
    git_service: Option<Arc<GitService>>,
    file_hashes: Arc<tokio::sync::RwLock<HashMap<String, HashMap<String, String>>>>,
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
            file_hashes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
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
            file_hashes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
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
            file_hashes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Set the git service for auto-commit functionality.
    pub fn set_git_service(&mut self, git_service: Arc<GitService>) {
        self.git_service = Some(git_service);
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

    /// Generate a full SHA256 hash of file content for change detection and ID generation.
    /// Returns (full_hash, short_hash) where short_hash is first 16 chars for memory ID.
    fn file_hash(content: &str) -> (String, String) {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        let full_hash = hex::encode(&result);
        let short_hash = full_hash[..16].to_string();
        (full_hash, short_hash)
    }

    /// Generate a short hash (16 chars) for backwards compatibility.
    #[allow(dead_code)]
    fn file_hash_short(content: &str) -> String {
        let (_, short) = Self::file_hash(content);
        short
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
            "Found files to index"
        );

        for (i, file_path) in files.iter().enumerate() {
            match self
                .index_file(file_path, project, &root, author)
                .await
            {
                Ok(indexed) => {
                    if indexed {
                        stats.indexed_files += 1;
                    } else {
                        stats.skipped_files += 1;
                    }
                }
                Err(e) => {
                    warn!(
                        file = %file_path.display(),
                        error = %e,
                        "Error indexing file"
                    );
                    stats.errors += 1;
                }
            }

            if let Some(ref callback) = progress {
                callback(i + 1, files.len(), &file_path.display().to_string());
            }
        }

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
                        stats.indexed_files,
                        project.slug
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
            Error::Internal(format!("Failed to read directory {}: {}", current.display(), e))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            Error::Internal(format!("Failed to read entry: {}", e))
        })? {
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

        // Calculate content hash - use for change detection and memory ID
        let (full_hash, memory_id) = Self::file_hash(&content);

        // Check if file has changed using in-memory cache
        {
            let hashes = self.file_hashes.read().await;
            if let Some(project_hashes) = hashes.get(&project.slug) {
                if project_hashes.get(&rel_path) == Some(&full_hash) {
                    debug!(file = %rel_path, "Skipping unchanged file");
                    return Ok(false);
                }
            }
        }

        // Also check if memory with this hash already exists in fold/
        if let Some(ref root_path) = project.root_path {
            let fold_root = Path::new(root_path);
            if self.fold_storage.exists(fold_root, &memory_id).await {
                debug!(file = %rel_path, hash = %memory_id, "Memory already exists in fold/");
                // Update in-memory cache
                let mut hashes = self.file_hashes.write().await;
                let project_hashes = hashes.entry(project.slug.clone()).or_insert_with(HashMap::new);
                project_hashes.insert(rel_path, full_hash);
                return Ok(false);
            }
        }

        // Generate summary using LLM
        let (title, summary, keywords, tags, created_date) = if self.llm.is_available().await {
            match self.llm.summarize_code(&content, &rel_path, &language).await {
                Ok(cs) => (cs.title, cs.summary, cs.keywords, cs.tags, cs.created_date),
                Err(e) => {
                    warn!(error = %e, "LLM summarization failed, using defaults");
                    (
                        rel_path.split('/').last().unwrap_or(&rel_path).to_string(),
                        String::new(),
                        Vec::new(),
                        Vec::new(),
                        None,
                    )
                }
            }
        } else {
            (
                rel_path.split('/').last().unwrap_or(&rel_path).to_string(),
                String::new(),
                Vec::new(),
                Vec::new(),
                None,
            )
        };

        // Create summary content for storage (not the raw file content)
        let summary_content = if summary.is_empty() {
            content.chars().take(1000).collect()
        } else {
            summary
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "content_hash".to_string(),
            serde_json::Value::String(full_hash.clone()),
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
                Some(language)
            },
            metadata,
            ..Default::default()
        };

        // Add memory to database and vector store
        let memory = self.memory_service
            .add(&project.id, &project.slug, create, false)
            .await?;

        // Write summary to fold/ directory using hash-based path
        if let Some(ref root_path) = project.root_path {
            let fold_root = Path::new(root_path);
            match self.fold_storage.write_memory(fold_root, &memory, &summary_content).await {
                Ok(path) => {
                    debug!(file = %rel_path, path = %path.display(), "Wrote summary to fold/");
                }
                Err(e) => {
                    warn!(file = %rel_path, error = %e, "Failed to write to fold/, continuing anyway");
                }
            }
        }

        // Update in-memory hash cache
        {
            let mut hashes = self.file_hashes.write().await;
            let project_hashes = hashes.entry(project.slug.clone()).or_insert_with(HashMap::new);
            project_hashes.insert(rel_path, full_hash);
        }

        Ok(true)
    }

    /// Index a single file by path (for webhook-triggered updates).
    ///
    /// Uses content hash for memory ID and writes summary to fold/a/b/hash.md.
    pub async fn index_single_file(
        &self,
        project: &Project,
        file_path: &str,
        content: &str,
        author: Option<&str>,
    ) -> Result<Memory> {
        let language = Self::detect_language(file_path);

        // Calculate content hash for change detection and metadata
        let (full_hash, _memory_id) = Self::file_hash(content);

        // Generate summary
        let (title, summary, keywords, tags) = if self.llm.is_available().await {
            match self.llm.summarize_code(content, file_path, &language).await {
                Ok(cs) => (cs.title, cs.summary, cs.keywords, cs.tags),
                Err(_) => (
                    file_path.split('/').last().unwrap_or(file_path).to_string(),
                    String::new(),
                    Vec::new(),
                    Vec::new(),
                )
            }
        } else {
            (
                file_path.split('/').last().unwrap_or(file_path).to_string(),
                String::new(),
                Vec::new(),
                Vec::new(),
            )
        };

        let summary_content = if summary.is_empty() {
            content.chars().take(1000).collect()
        } else {
            summary
        };

        // Include hash and file stats in metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "content_hash".to_string(),
            serde_json::Value::String(full_hash),
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
                Some(language)
            },
            metadata,
            ..Default::default()
        };

        // Add memory to database and vector store
        let memory = self.memory_service
            .add(&project.id, &project.slug, create, false)
            .await?;

        // Write summary to fold/ directory using hash-based path
        if let Some(ref root_path) = project.root_path {
            let fold_root = Path::new(root_path);
            match self.fold_storage.write_memory(fold_root, &memory, &summary_content).await {
                Ok(path) => {
                    debug!(file = %file_path, path = %path.display(), "Wrote summary to fold/");
                }
                Err(e) => {
                    warn!(file = %file_path, error = %e, "Failed to write to fold/, continuing anyway");
                }
            }
        }

        Ok(memory)
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
