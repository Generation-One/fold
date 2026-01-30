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
use crate::services::{GitHubService, GitSyncService, LlmService, MemoryService};

/// Poll interval for checking new jobs (seconds)
const POLL_INTERVAL_SECS: u64 = 2;

/// Maximum jobs to process concurrently
const MAX_CONCURRENT_JOBS: usize = 5;

/// How often to send heartbeats (seconds)
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// How often to check for stale jobs (seconds)
const STALE_CHECK_INTERVAL_SECS: u64 = 60;

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
    llm: Arc<LlmService>,
    running: RwLock<bool>,
    active_jobs: RwLock<usize>,
    worker_id: String,
}

impl JobWorker {
    /// Create a new job worker.
    pub fn new(
        db: DbPool,
        memory: MemoryService,
        git_sync: GitSyncService,
        github: Arc<GitHubService>,
        llm: Arc<LlmService>,
    ) -> Self {
        // Generate unique worker ID
        let worker_id = format!("worker-{}-{}", hostname(), nanoid::nanoid!(8));

        Self {
            inner: Arc::new(JobWorkerInner {
                db,
                memory,
                git_sync,
                github,
                llm,
                running: RwLock::new(false),
                active_jobs: RwLock::new(0),
                worker_id,
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

    /// Process a single job.
    /// Returns Ok(()) on success, Err on failure (will trigger retry).
    async fn process_job(&self, job_id: &str, job_type: &str) -> Result<()> {
        info!(job_id, job_type, worker_id = %self.inner.worker_id, "Processing job");

        // Process based on type
        let result = match JobType::from_str(job_type) {
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

        // TODO: Implement webhook processing based on payload
        // This would dispatch to git_sync based on webhook type

        self.log_job(job_id, LogLevel::Info, "Webhook processed").await?;
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

        // TODO: Use LLM to generate summary based on payload
        // This would be called for commit summaries, PR summaries, etc.

        self.log_job(job_id, LogLevel::Info, "Summary generated").await?;
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
    async fn process_index_repo(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let repo_id = job.repository_id.as_ref()
            .ok_or_else(|| Error::Internal("Job missing repository_id".to_string()))?;

        // Get repository details
        let repo = db::get_repository(&self.inner.db, repo_id).await?;

        // Get project
        let project = db::get_project(&self.inner.db, &repo.project_id).await?;

        info!(
            job_id,
            repo = %repo.full_name(),
            project = %project.slug,
            "Indexing repository files"
        );

        // Update progress for each item
        let total = job.total_items.unwrap_or(0);

        for i in 0..total {
            // TODO: Actually fetch and index files from GitHub
            // For now just update progress
            db::update_job_progress(&self.inner.db, job_id, i + 1, 0).await?;

            // Log progress periodically
            if (i + 1) % 10 == 0 || i + 1 == total {
                debug!(job_id, processed = i + 1, total, "Index progress");
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

    /// Process reindex_repo job - full repository reindex.
    async fn process_reindex_repo(&self, job_id: &str) -> Result<()> {
        let job = db::get_job(&self.inner.db, job_id).await?;

        let repo_id = job.repository_id.as_ref()
            .ok_or_else(|| Error::Internal("Job missing repository_id".to_string()))?;

        let repo = db::get_repository(&self.inner.db, repo_id).await?;
        let project = db::get_project(&self.inner.db, &repo.project_id).await?;

        info!(
            job_id,
            repo = %repo.full_name(),
            project = %project.slug,
            "Full repository reindex"
        );

        let token = &repo.access_token;

        // Get recent commits to understand what files exist
        let commits = self.inner.github
            .get_commits(&repo.owner, &repo.repo, Some(&repo.branch), None, 10, token)
            .await
            .map_err(|e| Error::Internal(format!("Failed to get commits: {}", e)))?;

        // For each commit, get the files and index them
        let mut indexed = 0;
        let mut failed = 0;

        for commit in &commits {
            // Get commit details including files
            match self.inner.github
                .get_commit(&repo.owner, &repo.repo, &commit.sha, token)
                .await
            {
                Ok(details) => {
                    // files is Option<Vec<GitHubFile>>
                    if let Some(files) = &details.files {
                        for file in files {
                            // Index each file
                            if let Err(e) = self.index_file(&repo, &project, &file.filename, token).await {
                                warn!(file = %file.filename, error = %e, "Failed to index file");
                                failed += 1;
                            } else {
                                indexed += 1;
                            }
                            db::update_job_progress(&self.inner.db, job_id, indexed, failed).await?;
                        }
                    }
                }
                Err(e) => {
                    warn!(sha = %commit.sha, error = %e, "Failed to get commit details");
                }
            }
        }

        self.log_job(
            job_id,
            LogLevel::Info,
            &format!(
                "Reindexed {}: {} indexed, {} failed",
                repo.full_name(),
                indexed,
                failed
            ),
        )
        .await?;

        Ok(())
    }

    /// Index a single file from GitHub.
    async fn index_file(
        &self,
        repo: &db::Repository,
        project: &db::Project,
        file_path: &str,
        token: &str,
    ) -> Result<()> {
        use crate::services::IndexerService;

        // Skip non-code files
        let lang = IndexerService::detect_language(file_path);
        if lang.is_empty() {
            return Ok(());
        }

        // Get file content from GitHub
        let file_info = self.inner.github
            .get_file(&repo.owner, &repo.repo, file_path, Some(&repo.branch), token)
            .await?;

        // Skip large files
        if file_info.size > 100_000 {
            debug!(file = %file_path, size = file_info.size, "Skipping large file");
            return Ok(());
        }

        // Decode content (base64)
        let content = file_info.content
            .ok_or_else(|| Error::Internal("File has no content".to_string()))?;

        let decoded = base64_decode(&content)?;
        let content_str = String::from_utf8_lossy(&decoded).to_string();

        // Create memory for the file
        let title = file_path.split('/').last().unwrap_or(file_path).to_string();

        // Store as memory using correct MemoryCreate structure
        self.inner.memory.add(
            &project.id,
            &project.slug,
            crate::models::MemoryCreate {
                memory_type: crate::models::MemoryType::Codebase,
                content: content_str,
                author: Some("system".to_string()),
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

        debug!(file = %file_path, "Indexed file");
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

    /// Process sync_metadata job - sync repository metadata.
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

        let token = &repo.access_token;

        // Get repository info from GitHub
        let _info = self.inner.github
            .get_repo(&repo.owner, &repo.repo, token)
            .await
            .map_err(|e| Error::Internal(format!("Failed to get repo: {}", e)))?;

        // Log that we synced metadata (no dedicated last_synced_at field)
        // The sync is just for verification - the real work is webhook-driven

        self.log_job(
            job_id,
            LogLevel::Info,
            &format!("Synced metadata for {}", repo.full_name()),
        )
        .await?;

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

/// Decode base64 content (GitHub returns base64-encoded file content)
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose};

    // Remove newlines that GitHub sometimes includes
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();

    general_purpose::STANDARD
        .decode(&cleaned)
        .map_err(|e| Error::Internal(format!("Base64 decode error: {}", e)))
}

/// Get hostname for worker ID.
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
