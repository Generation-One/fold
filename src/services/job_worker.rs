//! Background job worker for processing indexing tasks.
//!
//! Persistent SQLite-backed job queue with:
//! - Atomic job claiming (prevents duplicate processing)
//! - Priority-based scheduling
//! - Automatic retry with exponential backoff
//! - Stale job recovery
//! - Execution history tracking
//! - Heartbeat to prevent job timeouts

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::db::{self, DbPool, JobType, LogLevel};
use crate::error::{Error, Result};
use crate::services::{EmbeddingService, GitHubService, GitLocalService, GitSyncService, IndexerService, LlmService, MemoryService, MetadataSyncService};

/// Poll interval for checking new jobs (seconds)
const POLL_INTERVAL_SECS: u64 = 2;

/// Maximum jobs to process concurrently
const MAX_CONCURRENT_JOBS: usize = 5;

/// How often to send heartbeats (seconds)
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// How often to check for stale jobs (seconds)
const STALE_CHECK_INTERVAL_SECS: u64 = 60;

/// Default polling interval for repository sync (seconds) - 5 minutes
const REPO_POLL_INTERVAL_SECS: u64 = 300;

/// How often to check for provider availability and resume paused jobs (seconds)
const PROVIDER_CHECK_INTERVAL_SECS: u64 = 30;

/// Background job worker service.
#[derive(Clone)]
pub struct JobWorker {
    inner: Arc<JobWorkerInner>,
}

struct JobWorkerInner {
    db: DbPool,
    memory: MemoryService,
    git_sync: GitSyncService,
    github: Arc<GitHubService>,
    git_local: Arc<GitLocalService>,
    indexer: IndexerService,
    llm: Arc<LlmService>,
    embeddings: Arc<EmbeddingService>,
    running: RwLock<bool>,
    active_jobs: RwLock<usize>,
    worker_id: String,
    /// Tracks whether providers are currently available
    providers_available: RwLock<bool>,
}

impl JobWorker {
    /// Create a new job worker.
    pub fn new(
        db: DbPool,
        memory: MemoryService,
        git_sync: GitSyncService,
        github: Arc<GitHubService>,
        git_local: Arc<GitLocalService>,
        indexer: IndexerService,
        llm: Arc<LlmService>,
        embeddings: Arc<EmbeddingService>,
    ) -> Self {
        // Generate unique worker ID
        let worker_id = format!("worker-{}-{}", hostname(), nanoid::nanoid!(8));

        Self {
            inner: Arc::new(JobWorkerInner {
                db,
                memory,
                git_sync,
                github,
                git_local,
                indexer,
                llm,
                embeddings,
                running: RwLock::new(false),
                active_jobs: RwLock::new(0),
                worker_id,
                providers_available: RwLock::new(true),
            }),
        }
    }

    /// Get the worker ID.
    pub fn worker_id(&self) -> &str {
        &self.inner.worker_id
    }

    /// Start the job worker background loop.
    /// Returns a handle that can be used to stop the worker.
    pub async fn start(&self) -> JobWorkerHandle {
        let worker = self.clone();

        // Set running flag
        *worker.inner.running.write().await = true;

        // Check initial provider status
        let initial_available = self.check_providers_available().await;
        *self.inner.providers_available.write().await = initial_available;
        if !initial_available {
            warn!("LLM/embedding providers not available at startup - indexing jobs will be paused");
        }

        // Spawn main worker loop
        let worker_clone = worker.clone();
        let main_handle = tokio::spawn(async move {
            worker_clone.run_loop().await;
        });

        // Spawn stale job recovery loop
        let recovery_worker = self.clone();
        tokio::spawn(async move {
            recovery_worker.run_stale_recovery_loop().await;
        });

        // Spawn repository polling loop (for repos without webhooks)
        let polling_worker = self.clone();
        tokio::spawn(async move {
            polling_worker.run_repository_polling_loop().await;
        });

        // Spawn provider availability check loop
        let provider_worker = self.clone();
        tokio::spawn(async move {
            provider_worker.run_provider_check_loop().await;
        });

        info!(worker_id = %self.inner.worker_id, "Job worker started");

        JobWorkerHandle {
            worker: self.clone(),
            _handle: main_handle,
        }
    }

    /// Run the main processing loop.
    async fn run_loop(&self) {
        loop {
            // Check if we should stop
            if !*self.inner.running.read().await {
                info!(worker_id = %self.inner.worker_id, "Job worker stopping");
                break;
            }

            // Check active job count
            let active = *self.inner.active_jobs.read().await;
            if active >= MAX_CONCURRENT_JOBS {
                debug!(active, max = MAX_CONCURRENT_JOBS, "At max concurrent jobs, waiting");
                sleep(Duration::from_secs(1)).await;
                continue;
            }

            // Try to claim and process a job
            match self.claim_and_process().await {
                Ok(claimed) => {
                    if !claimed {
                        // No jobs available, wait before polling again
                        sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Error claiming job");
                    sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                }
            }
        }
    }

    /// Run stale job recovery loop.
    async fn run_stale_recovery_loop(&self) {
        loop {
            if !*self.inner.running.read().await {
                break;
            }

            // Recover stale jobs (locked for more than 5 minutes)
            match db::recover_stale_jobs(&self.inner.db, Some(300)).await {
                Ok(recovered) => {
                    if recovered > 0 {
                        info!(count = recovered, "Recovered stale jobs");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Error recovering stale jobs");
                }
            }

            sleep(Duration::from_secs(STALE_CHECK_INTERVAL_SECS)).await;
        }
    }

    /// Check if LLM and embedding providers are available.
    async fn check_providers_available(&self) -> bool {
        let llm_available = self.inner.llm.is_available().await;
        let embeddings_available = self.inner.embeddings.has_providers().await;

        llm_available && embeddings_available
    }

    /// Run provider availability check loop.
    /// Resumes paused jobs when providers become available.
    async fn run_provider_check_loop(&self) {
        // Wait a bit before starting
        sleep(Duration::from_secs(5)).await;

        loop {
            if !*self.inner.running.read().await {
                break;
            }

            let was_available = *self.inner.providers_available.read().await;
            let now_available = self.check_providers_available().await;

            // Update the state
            *self.inner.providers_available.write().await = now_available;

            // If providers just became available, resume paused jobs
            if !was_available && now_available {
                info!("LLM/embedding providers are now available - resuming paused jobs");
                match db::resume_paused_jobs(&self.inner.db).await {
                    Ok(count) => {
                        if count > 0 {
                            info!(count, "Resumed paused jobs");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to resume paused jobs");
                    }
                }
            } else if was_available && !now_available {
                warn!("LLM/embedding providers are no longer available - new indexing jobs will be paused");
            }

            // Log paused job count periodically
            if !now_available {
                match db::count_paused_jobs(&self.inner.db).await {
                    Ok(count) if count > 0 => {
                        debug!(count, "Jobs waiting for providers");
                    }
                    _ => {}
                }
            }

            sleep(Duration::from_secs(PROVIDER_CHECK_INTERVAL_SECS)).await;
        }
    }

    /// Run repository polling loop to check for new commits.
    ///
    /// Checks all repositories with `notification_type = 'polling'` every 5 minutes
    /// (or their custom interval) and creates sync jobs for any with new commits.
    async fn run_repository_polling_loop(&self) {
        // Wait a bit before starting to let the server fully initialize
        sleep(Duration::from_secs(10)).await;

        loop {
            if !*self.inner.running.read().await {
                break;
            }

            // Get all repositories with polling enabled
            match db::list_polling_repositories(&self.inner.db).await {
                Ok(repos) => {
                    for repo in repos {
                        // Check if we should poll this repo based on its interval
                        let interval_secs = repo.sync_cursor
                            .as_deref()
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(REPO_POLL_INTERVAL_SECS);

                        let should_poll = repo.last_sync
                            .as_ref()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|last| {
                                let elapsed = chrono::Utc::now().signed_duration_since(last);
                                elapsed.num_seconds() as u64 >= interval_secs
                            })
                            .unwrap_or(true); // No last_sync means never polled

                        if !should_poll {
                            continue;
                        }

                        // Check for new commits
                        debug!(
                            repo = %repo.full_name(),
                            "Polling repository for new commits"
                        );

                        match self.poll_repository(&repo).await {
                            Ok(new_commits) => {
                                if new_commits > 0 {
                                    info!(
                                        repo = %repo.full_name(),
                                        new_commits,
                                        "Found new commits during polling"
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(
                                    repo = %repo.full_name(),
                                    error = %e,
                                    "Error polling repository"
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Error listing polling repositories");
                }
            }

            // Wait before next poll cycle
            sleep(Duration::from_secs(REPO_POLL_INTERVAL_SECS)).await;
        }
    }

    /// Poll a single repository for new commits.
    async fn poll_repository(&self, repo: &db::Repository) -> Result<usize> {
        // Fetch commits from GitHub since last sync
        let commits = self.inner.github
            .get_commits(
                &repo.owner,
                &repo.repo,
                Some(&repo.branch),
                repo.last_commit_sha.as_deref(),
                100,
                &repo.access_token,
            )
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch commits: {}", e)))?;

        let new_commit_count = commits.len();

        // Update last_sync time regardless of whether we found commits
        db::update_repository(
            &self.inner.db,
            &repo.id,
            db::UpdateRepository {
                last_sync: Some(chrono::Utc::now().to_rfc3339()),
                ..Default::default()
            },
        )
        .await?;

        if new_commit_count == 0 {
            return Ok(0);
        }

        // Update last_commit_sha to the newest commit
        if let Some(newest) = commits.first() {
            db::update_repository(
                &self.inner.db,
                &repo.id,
                db::UpdateRepository {
                    last_commit_sha: Some(newest.sha.clone()),
                    ..Default::default()
                },
            )
            .await?;
        }

        // Create a sync job to process the commits
        let job_id = crate::models::new_id();
        let payload = serde_json::json!({
            "commits": commits.iter().map(|c| &c.sha).collect::<Vec<_>>(),
        });

        db::create_job(
            &self.inner.db,
            db::CreateJob::new(job_id, db::JobType::SyncMetadata)
                .with_project(repo.project_id.clone())
                .with_repository(repo.id.clone())
                .with_payload(payload),
        )
        .await?;

        Ok(new_commit_count)
    }

    /// Atomically claim a job and process it.
    async fn claim_and_process(&self) -> Result<bool> {
        // Atomically claim the next available job
        let job = match db::claim_job(&self.inner.db, &self.inner.worker_id).await? {
            Some(job) => job,
            None => return Ok(false),
        };

        // Increment active job count
        *self.inner.active_jobs.write().await += 1;

        // Spawn job processing (don't block the loop)
        let worker = self.clone();
        let job_id = job.id.clone();
        let job_type = job.job_type.clone();
        let retry_count = job.retry_count.unwrap_or(0);

        tokio::spawn(async move {
            // Record execution attempt
            let execution = db::create_job_execution(
                &worker.inner.db,
                &job_id,
                retry_count + 1,
                &worker.inner.worker_id,
            ).await.ok();

            let start_time = Instant::now();

            // Spawn heartbeat task
            let heartbeat_worker = worker.clone();
            let heartbeat_job_id = job_id.clone();
            let heartbeat_handle = tokio::spawn(async move {
                heartbeat_worker.heartbeat_loop(&heartbeat_job_id).await;
            });

            // Process the job
            let result = worker.process_job(&job_id, &job_type).await;
            let duration_ms = start_time.elapsed().as_millis() as i64;

            // Stop heartbeat
            heartbeat_handle.abort();

            // Record execution result
            if let Some(exec) = execution {
                let (status, error) = match &result {
                    Ok(()) => ("success", None),
                    Err(ref e) if e.to_string().starts_with("PAUSED:") => ("paused", None),
                    Err(e) => ("failed", Some(e.to_string())),
                };
                let _ = db::complete_job_execution(
                    &worker.inner.db,
                    exec.id,
                    status,
                    error.as_deref(),
                    duration_ms,
                ).await;
            }

            // Handle result
            match result {
                Ok(()) => {
                    info!(job_id = %job_id, duration_ms, "Job completed successfully");
                }
                Err(ref e) if e.to_string().starts_with("PAUSED:") => {
                    // Job was paused, don't retry - it will resume when providers are available
                    info!(job_id = %job_id, duration_ms, "Job paused waiting for providers");
                }
                Err(e) => {
                    error!(job_id = %job_id, error = %e, "Job processing failed, scheduling retry");
                    // Attempt retry (this handles max retries automatically)
                    let _ = db::retry_job(&worker.inner.db, &job_id, &e.to_string()).await;
                }
            }

            // Decrement active job count
            *worker.inner.active_jobs.write().await -= 1;
        });

        Ok(true)
    }

    /// Send periodic heartbeats for a running job.
    async fn heartbeat_loop(&self, job_id: &str) {
        loop {
            sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;

            if !*self.inner.running.read().await {
                break;
            }

            match db::heartbeat_job(&self.inner.db, job_id, &self.inner.worker_id).await {
                Ok(true) => {
                    debug!(job_id, "Heartbeat sent");
                }
                Ok(false) => {
                    // Job no longer owned by us, stop heartbeat
                    warn!(job_id, "Job no longer owned by this worker, stopping heartbeat");
                    break;
                }
                Err(e) => {
                    warn!(job_id, error = %e, "Failed to send heartbeat");
                }
            }
        }
    }

    /// Check if a job type requires LLM/embedding providers.
    fn job_requires_providers(job_type: &JobType) -> bool {
        matches!(
            job_type,
            JobType::IndexRepo
                | JobType::ReindexRepo
                | JobType::IndexHistory
                | JobType::GenerateSummary
        )
    }

    /// Process a single job.
    /// Returns Ok(()) on success, Err on failure (will trigger retry).
    /// Special case: Returns Err with "PAUSE:" prefix to pause instead of retry.
    async fn process_job(&self, job_id: &str, job_type: &str) -> Result<()> {
        info!(job_id, job_type, worker_id = %self.inner.worker_id, "Processing job");

        // Parse job type
        let parsed_type = JobType::from_str(job_type);

        // Check if this job requires providers and if they're available
        if let Some(ref jt) = parsed_type {
            if Self::job_requires_providers(jt) {
                let providers_ok = *self.inner.providers_available.read().await;
                if !providers_ok {
                    // Double-check by actually testing providers
                    let actually_available = self.check_providers_available().await;
                    *self.inner.providers_available.write().await = actually_available;

                    if !actually_available {
                        warn!(
                            job_id,
                            job_type,
                            "Providers unavailable - pausing job"
                        );
                        self.log_job(
                            job_id,
                            LogLevel::Warn,
                            "Paused: LLM/embedding providers not available",
                        ).await?;

                        // Pause the job instead of failing
                        db::pause_job(
                            &self.inner.db,
                            job_id,
                            "LLM/embedding providers not available",
                        ).await?;

                        // Return a special error that the caller can detect
                        return Err(Error::Internal("PAUSED:providers_unavailable".to_string()));
                    }
                }
            }
        }

        // Process based on type
        let result = match parsed_type {
            Some(JobType::IndexRepo) => self.process_index_repo(job_id).await,
            Some(JobType::ReindexRepo) => self.process_reindex_repo(job_id).await,
            Some(JobType::IndexHistory) => self.process_index_history(job_id).await,
            Some(JobType::SyncMetadata) => self.process_sync_metadata(job_id).await,
            Some(JobType::ProcessWebhook) => self.process_webhook(job_id).await,
            Some(JobType::GenerateSummary) => self.process_generate_summary(job_id).await,
            Some(JobType::Custom) => self.process_custom(job_id).await,
            None => {
                warn!(job_id, job_type, "Unknown job type");
                Err(Error::Internal(format!("Unknown job type: {}", job_type)))
            }
        };

        // Mark complete on success (retry handled by caller on error)
        if result.is_ok() {
            db::complete_job(&self.inner.db, job_id, None).await?;
        }

        result
    }

    /// Process webhook job.
    async fn process_webhook(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        // Get payload from job
        let payload: serde_json::Value = job.payload
            .as_ref()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_default();

        info!(job_id, payload = ?payload, "Processing webhook job");

        // Route webhook to appropriate handler based on event type
        let event_type = payload.get("event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match event_type {
            "push" => {
                // Process push event - index changed files
                if let Err(e) = self.inner.git_sync.process_push_webhook(&payload).await {
                    warn!(error = %e, job_id, "Failed to process push webhook");
                    return Err(Error::Internal(format!("Push webhook processing failed: {}", e)));
                }
                self.log_job(job_id, LogLevel::Info, "Processed push webhook").await?;
            }
            "pull_request" | "merge_request" => {
                // Process PR/MR event
                if let Err(e) = self.inner.git_sync.process_pr_webhook(&payload).await {
                    warn!(error = %e, job_id, "Failed to process PR webhook");
                    return Err(Error::Internal(format!("PR webhook processing failed: {}", e)));
                }
                self.log_job(job_id, LogLevel::Info, "Processed PR webhook").await?;
            }
            other => {
                // Unknown event type - log and continue
                self.log_job(
                    job_id,
                    LogLevel::Warn,
                    &format!("Unknown webhook event type: {}", other),
                ).await?;
            }
        }

        Ok(())
    }

    /// Process summary generation job.
    async fn process_generate_summary(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let payload: serde_json::Value = job.payload
            .as_ref()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_default();

        info!(job_id, "Generating summary");

        // Extract content to summarize from payload
        let content = payload.get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if content.is_empty() {
            self.log_job(job_id, LogLevel::Warn, "No content provided for summary").await?;
            return Ok(());
        }

        // Get summary type for context
        let summary_type = payload.get("summary_type")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        // Build prompt based on summary type
        let prompt = match summary_type {
            "commit" => format!(
                "Generate a concise summary (2-3 sentences) of this git commit. Focus on what changed and why:\n\n{}",
                content
            ),
            "pr" | "pull_request" => format!(
                "Generate a concise summary (2-3 sentences) of this pull request. Focus on the main changes and their purpose:\n\n{}",
                content
            ),
            "code" => format!(
                "Generate a concise summary (2-3 sentences) of this code. Focus on its purpose and key functionality:\n\n{}",
                content
            ),
            _ => format!(
                "Generate a concise summary (2-3 sentences) of the following:\n\n{}",
                content
            ),
        };

        // Generate summary using LLM (max 500 tokens for concise output)
        match self.inner.llm.complete(&prompt, 500).await {
            Ok(summary) => {
                self.log_job(
                    job_id,
                    LogLevel::Info,
                    &format!("Generated {} char summary for {}", summary.len(), summary_type),
                ).await?;

                // Store summary in job metadata for retrieval
                let metadata = serde_json::json!({ "summary": summary });
                db::update_job_metadata(&self.inner.db, job_id, &metadata).await?;
            }
            Err(e) => {
                self.log_job(
                    job_id,
                    LogLevel::Error,
                    &format!("Failed to generate summary: {}", e),
                ).await?;
                return Err(Error::Internal(format!("LLM summary generation failed: {}", e)));
            }
        }

        Ok(())
    }

    /// Process custom job (payload-driven).
    async fn process_custom(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let payload: serde_json::Value = job.payload
            .as_ref()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_default();

        info!(job_id, payload = ?payload, "Processing custom job");

        // Custom jobs are entirely payload-driven
        // The payload should contain all necessary information

        self.log_job(job_id, LogLevel::Info, "Custom job processed").await?;
        Ok(())
    }

    /// Process index_repo job - index files from a push event.
    /// Uses local clone for file reading.
    async fn process_index_repo(&self, job_id: &str) -> Result<()> {
        use std::path::PathBuf;

        let job = db::get_job(&self.inner.db, job_id).await?;

        let repo_id = job.repository_id.as_ref()
            .ok_or_else(|| Error::Internal("Job missing repository_id".to_string()))?;

        // Get repository details
        let mut repo = db::get_repository(&self.inner.db, repo_id).await?;

        // Get project
        let project = db::get_project(&self.inner.db, &repo.project_id).await?;

        info!(
            job_id,
            repo = %repo.full_name(),
            project = %project.slug,
            "Indexing repository files from push event"
        );

        // Ensure we have a local clone
        let local_path = match repo.local_path.clone() {
            Some(path) => PathBuf::from(path),
            None => {
                info!(job_id, repo = %repo.full_name(), "Cloning repository locally");

                let cloned_path = self.inner.git_local.clone_repo(
                    &project.slug,
                    &repo.owner,
                    &repo.repo,
                    &repo.branch,
                    &repo.access_token,
                    &repo.provider,
                ).await?;

                let path_str = cloned_path.to_string_lossy().to_string();
                db::update_repository(&self.inner.db, &repo.id, db::UpdateRepository {
                    local_path: Some(path_str.clone()),
                    ..Default::default()
                }).await?;

                repo.local_path = Some(path_str);
                cloned_path
            }
        };

        // Pull latest changes
        if let Err(e) = self.inner.git_local.pull_repo(
            &local_path,
            &repo.branch,
            &repo.access_token,
            &repo.provider,
        ).await {
            warn!(job_id, error = %e, "Failed to pull latest changes, using existing files");
        }

        // Extract files from job payload (set by webhook handler)
        let payload: serde_json::Value = job.payload
            .as_ref()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_default();

        // Get files to index from payload
        let files: Vec<String> = payload.get("files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect())
            .unwrap_or_default();

        let total = files.len();
        let mut indexed = 0i32;
        let mut failed = 0i32;

        for (i, file_path) in files.iter().enumerate() {
            match self.index_file_from_local(&local_path, &project, file_path).await {
                Ok(()) => {
                    indexed += 1;
                    debug!(job_id, file = %file_path, "Indexed file");
                }
                Err(e) => {
                    failed += 1;
                    warn!(job_id, file = %file_path, error = %e, "Failed to index file");
                }
            }

            // Update progress
            db::update_job_progress(&self.inner.db, job_id, indexed, failed).await?;

            // Log progress periodically
            if (i + 1) % 10 == 0 || i + 1 == total {
                debug!(job_id, processed = i + 1, total, indexed, failed, "Index progress");
            }
        }

        // Log completion
        self.log_job(
            job_id,
            LogLevel::Info,
            &format!("Indexed {} files from {}", total, repo.full_name()),
        )
        .await?;

        Ok(())
    }

    /// Index a single file from local clone.
    async fn index_file_from_local(
        &self,
        local_path: &std::path::Path,
        project: &db::Project,
        file_path: &str,
    ) -> Result<()> {
        use crate::services::IndexerService;

        // Skip non-code files
        let lang = IndexerService::detect_language(file_path);
        if lang.is_empty() {
            return Ok(());
        }

        let full_path = local_path.join(file_path);

        // Read file content from local clone
        let content = tokio::fs::read_to_string(&full_path).await.map_err(|e| {
            Error::Internal(format!("Failed to read {}: {}", full_path.display(), e))
        })?;

        // Skip empty files
        if content.trim().is_empty() {
            return Ok(());
        }

        // Skip large files (100KB)
        if content.len() > 100_000 {
            debug!(file = %file_path, size = content.len(), "Skipping large file");
            return Ok(());
        }

        // Create memory for the file
        let title = file_path.split('/').last().unwrap_or(file_path).to_string();

        self.inner.memory.add(
            &project.id,
            &project.slug,
            crate::models::MemoryCreate {
                memory_type: crate::models::MemoryType::Codebase,
                content,
                author: Some("system".to_string()),
                source: Some(crate::models::MemorySource::File),
                title: Some(title),
                keywords: vec![],
                tags: vec![lang.clone(), "code".to_string()],
                context: None,
                file_path: Some(file_path.to_string()),
                language: Some(lang),
                status: None,
                assignee: None,
                metadata: std::collections::HashMap::new(),
            },
            true, // auto_metadata
        ).await?;

        debug!(file = %file_path, "Indexed file from local clone");
        Ok(())
    }

    /// Process reindex_repo job - full repository reindex.
    /// Always uses local clone for efficient full-repo indexing.
    async fn process_reindex_repo(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let repo_id = job.repository_id.as_ref()
            .ok_or_else(|| Error::Internal("Job missing repository_id".to_string()))?;

        let mut repo = db::get_repository(&self.inner.db, repo_id).await?;
        let project = db::get_project(&self.inner.db, &repo.project_id).await?;

        info!(
            job_id,
            repo = %repo.full_name(),
            project = %project.slug,
            local_path = ?repo.local_path,
            "Full repository reindex"
        );

        // Clone repo locally if we don't have a local path yet
        let local_path = match repo.local_path.clone() {
            Some(path) => path,
            None => {
                info!(job_id, repo = %repo.full_name(), "Cloning repository locally");

                let cloned_path = self.inner.git_local.clone_repo(
                    &project.slug,
                    &repo.owner,
                    &repo.repo,
                    &repo.branch,
                    &repo.access_token,
                    &repo.provider,
                ).await?;

                let path_str = cloned_path.to_string_lossy().to_string();

                // Update repository record with local path
                db::update_repository(&self.inner.db, &repo.id, db::UpdateRepository {
                    local_path: Some(path_str.clone()),
                    ..Default::default()
                }).await?;

                repo.local_path = Some(path_str.clone());
                path_str
            }
        };

        self.reindex_from_local(job_id, &repo, &project, &local_path).await
    }

    /// Reindex using local clone - indexes all files in the repository.
    async fn reindex_from_local(
        &self,
        job_id: &str,
        repo: &db::Repository,
        project: &db::Project,
        local_path: &str,
    ) -> Result<()> {
        use std::path::PathBuf;

        info!(
            job_id,
            repo = %repo.full_name(),
            path = %local_path,
            "Reindexing from local clone"
        );

        // Pull latest changes first
        let path = PathBuf::from(local_path);
        if let Err(e) = self.inner.git_local.pull_repo(
            &path,
            &repo.branch,
            &repo.access_token,
            &repo.provider,
        ).await {
            warn!(
                job_id,
                repo = %repo.full_name(),
                error = %e,
                "Failed to pull latest changes, indexing existing files"
            );
        }

        // Update the HEAD SHA after pulling
        if let Ok(sha) = self.inner.git_local.get_head_sha(&path).await {
            db::update_repository_indexed(&self.inner.db, &repo.id, &sha).await?;
        }

        // Create a temporary project with the local path set
        // The indexer uses project.root_path for local indexing
        let mut indexed_project = crate::models::Project::new(project.name.clone());
        indexed_project.id = project.id.clone();
        indexed_project.slug = project.slug.clone();
        indexed_project.root_path = Some(local_path.to_string());

        // Use the indexer service for local file indexing
        match self.inner.indexer.index_project(
            &indexed_project,
            Some("system"),
            None, // No progress callback
        ).await {
            Ok(result) => {
                self.log_job(
                    job_id,
                    LogLevel::Info,
                    &format!(
                        "Reindexed {} from local clone: {} files indexed, {} skipped, {} errors",
                        repo.full_name(),
                        result.indexed_files,
                        result.skipped_files,
                        result.errors
                    ),
                ).await?;

                db::update_job_progress(
                    &self.inner.db,
                    job_id,
                    result.indexed_files as i32,
                    result.errors as i32,
                ).await?;
            }
            Err(e) => {
                self.log_job(
                    job_id,
                    LogLevel::Error,
                    &format!("Local reindex failed: {}", e),
                ).await?;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Process index_history job - index commit history.
    async fn process_index_history(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let repo_id = job.repository_id.as_ref()
            .ok_or_else(|| Error::Internal("Job missing repository_id".to_string()))?;

        let repo = db::get_repository(&self.inner.db, repo_id).await?;
        let project = db::get_project(&self.inner.db, &repo.project_id).await?;

        info!(
            job_id,
            repo = %repo.full_name(),
            project = %project.slug,
            "Indexing commit history"
        );

        let token = &repo.access_token;

        // Get recent commits
        let commits = self.inner.github
            .get_commits(&repo.owner, &repo.repo, Some(&repo.branch), None, 100, token)
            .await
            .map_err(|e| Error::Internal(format!("Failed to get commits: {}", e)))?;

        let total = commits.len();

        for (i, commit) in commits.iter().enumerate() {
            // Store commit in database
            // GitHubCommit has: sha, commit (GitHubCommitDetails), author (Option<GitHubUser>)
            let _ = db::create_git_commit(
                &self.inner.db,
                db::CreateGitCommit {
                    id: nanoid::nanoid!(),
                    repository_id: repo_id.clone(),
                    sha: commit.sha.clone(),
                    message: commit.commit.message.clone(),
                    author_name: Some(commit.commit.author.name.clone()),
                    author_email: Some(commit.commit.author.email.clone()),
                    files_changed: None,
                    insertions: commit.stats.as_ref().map(|s| s.additions),
                    deletions: commit.stats.as_ref().map(|s| s.deletions),
                    committed_at: commit.commit.author.date.clone(),
                },
            )
            .await;

            db::update_job_progress(&self.inner.db, job_id, i as i32 + 1, 0).await?;
        }

        self.log_job(
            job_id,
            LogLevel::Info,
            &format!("Indexed {} commits from {}", total, repo.full_name()),
        )
        .await?;

        Ok(())
    }

    /// Process sync_metadata job - sync repository metadata back to the repo.
    ///
    /// This generates Markdown files in `.fold/` directory and pushes them
    /// to the repository as commits from `fold-meta-bot`.
    async fn process_sync_metadata(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let repo_id = job.repository_id.as_ref()
            .ok_or_else(|| Error::Internal("Job missing repository_id".to_string()))?;

        let repo = db::get_repository(&self.inner.db, repo_id).await?;

        info!(
            job_id,
            repo = %repo.full_name(),
            "Syncing repository metadata"
        );

        self.log_job(
            job_id,
            LogLevel::Info,
            &format!("Starting metadata sync for {}", repo.full_name()),
        )
        .await?;

        // Create metadata sync service
        let work_dir = std::env::temp_dir().join("fold-metadata-sync");
        let metadata_sync = MetadataSyncService::new(self.inner.db.clone(), work_dir);

        // Sync metadata to repository
        match metadata_sync.sync_repository(&repo).await {
            Ok(result) => {
                if let Some(ref commit_sha) = result.commit_sha {
                    self.log_job(
                        job_id,
                        LogLevel::Info,
                        &format!(
                            "Synced metadata for {}: {} files created, {} updated (commit: {})",
                            repo.full_name(),
                            result.files_created,
                            result.files_updated,
                            &commit_sha[..8]
                        ),
                    )
                    .await?;
                } else {
                    self.log_job(
                        job_id,
                        LogLevel::Info,
                        &format!("No changes to sync for {}", repo.full_name()),
                    )
                    .await?;
                }
            }
            Err(e) => {
                self.log_job(
                    job_id,
                    LogLevel::Error,
                    &format!("Failed to sync metadata for {}: {}", repo.full_name(), e),
                )
                .await?;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Helper to log job messages.
    async fn log_job(&self, job_id: &str, level: LogLevel, message: &str) -> Result<()> {
        db::create_job_log(
            &self.inner.db,
            db::CreateJobLog {
                job_id: job_id.to_string(),
                level,
                message: message.to_string(),
                metadata: None,
            },
        )
        .await?;
        Ok(())
    }

    /// Stop the job worker.
    pub async fn stop(&self) {
        info!("Stopping job worker");
        *self.inner.running.write().await = false;
    }

    /// Get current job worker status.
    pub async fn status(&self) -> JobWorkerStatus {
        JobWorkerStatus {
            running: *self.inner.running.read().await,
            active_jobs: *self.inner.active_jobs.read().await,
            worker_id: self.inner.worker_id.clone(),
        }
    }
}

/// Handle for the running job worker.
pub struct JobWorkerHandle {
    worker: JobWorker,
    _handle: tokio::task::JoinHandle<()>,
}

impl JobWorkerHandle {
    /// Stop the job worker.
    pub async fn stop(self) {
        self.worker.stop().await;
    }
}

/// Job worker status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JobWorkerStatus {
    pub running: bool,
    pub active_jobs: usize,
    pub worker_id: String,
}

/// Get hostname for worker ID.
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
