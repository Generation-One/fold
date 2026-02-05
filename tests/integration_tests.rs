//! Integration tests for Fold server.
//!
//! Tests database operations and embedding service.

use fold::db;
use fold::Result;

// ============================================================================
// Database Integration Tests
// ============================================================================

/// Test basic database initialization and migration
#[tokio::test]
async fn test_database_init_and_migrate() -> Result<()> {
    let pool = db::init_pool(":memory:").await?;
    db::migrate(&pool).await?;

    // Verify tables exist by querying them
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
        .fetch_one(&pool)
        .await?;
    assert_eq!(result.0, 0);

    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories")
        .fetch_one(&pool)
        .await?;
    assert_eq!(result.0, 0);

    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM repositories")
        .fetch_one(&pool)
        .await?;
    assert_eq!(result.0, 0);

    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(result.0, 0);

    Ok(())
}

/// Test project CRUD operations
#[tokio::test]
async fn test_project_crud() -> Result<()> {
    let pool = db::init_pool(":memory:").await?;
    db::migrate(&pool).await?;

    // Create project
    let project = db::create_project(
        &pool,
        db::CreateProject {
            id: "test-id".to_string(),
            name: "Test Project".to_string(),
            slug: "test-project".to_string(),
            description: Some("A test project".to_string()),
        },
    )
    .await?;

    assert_eq!(project.name, "Test Project");
    assert_eq!(project.slug, "test-project");

    // Get project
    let fetched = db::get_project(&pool, &project.id).await?;
    assert_eq!(fetched.id, project.id);

    // Get by slug
    let by_slug = db::get_project_by_slug(&pool, "test-project").await?;
    assert!(by_slug.is_some());
    assert_eq!(by_slug.unwrap().id, project.id);

    // Update project
    let updated = db::update_project(
        &pool,
        &project.id,
        db::UpdateProject {
            name: Some("Updated Name".to_string()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(updated.name, "Updated Name");

    // List projects
    let projects = db::list_projects(&pool).await?;
    assert_eq!(projects.len(), 1);

    // Delete project
    db::delete_project(&pool, &project.id).await?;
    let projects = db::list_projects(&pool).await?;
    assert_eq!(projects.len(), 0);

    Ok(())
}

/// Test job lifecycle
#[tokio::test]
async fn test_job_lifecycle() -> Result<()> {
    let pool = db::init_pool(":memory:").await?;
    db::migrate(&pool).await?;

    // Create job (no FK references to avoid constraint issues)
    let job = db::create_job(
        &pool,
        db::CreateJob {
            id: "job-1".to_string(),
            job_type: db::JobType::IndexRepo,
            project_id: None,
            repository_id: None,
            total_items: Some(10),
        },
    )
    .await?;

    assert_eq!(job.status, "pending");
    assert!(!job.is_finished());

    // Start job
    let started = db::start_job(&pool, &job.id).await?;
    assert_eq!(started.status, "running");
    assert!(started.is_running());

    // Update progress
    db::update_job_progress(&pool, &job.id, 5, 1).await?;
    let updated = db::get_job(&pool, &job.id).await?;
    assert_eq!(updated.processed_items, 5);
    assert_eq!(updated.failed_items, 1);

    // Complete job
    let completed = db::complete_job(&pool, &job.id, None).await?;
    assert_eq!(completed.status, "completed");
    assert!(completed.is_finished());

    // List pending jobs (should be empty)
    let pending = db::list_pending_jobs(&pool, 10).await?;
    assert_eq!(pending.len(), 0);

    Ok(())
}

/// Test job failure
#[tokio::test]
async fn test_job_failure() -> Result<()> {
    let pool = db::init_pool(":memory:").await?;
    db::migrate(&pool).await?;

    // Create and start job
    let job = db::create_job(
        &pool,
        db::CreateJob {
            id: "job-fail".to_string(),
            job_type: db::JobType::ReindexRepo,
            project_id: None,
            repository_id: None,
            total_items: None,
        },
    )
    .await?;

    db::start_job(&pool, &job.id).await?;

    // Fail job
    let failed = db::fail_job(&pool, &job.id, "Test error").await?;
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.error, Some("Test error".to_string()));
    assert!(failed.is_finished());

    Ok(())
}

/// Test job log creation
#[tokio::test]
async fn test_job_logging() -> Result<()> {
    let pool = db::init_pool(":memory:").await?;
    db::migrate(&pool).await?;

    // Create job
    let job = db::create_job(
        &pool,
        db::CreateJob {
            id: "job-log".to_string(),
            job_type: db::JobType::IndexRepo,
            project_id: None,
            repository_id: None,
            total_items: None,
        },
    )
    .await?;

    // Create log entries
    db::create_job_log(
        &pool,
        db::CreateJobLog {
            job_id: job.id.clone(),
            level: db::LogLevel::Info,
            message: "Starting job".to_string(),
            metadata: None,
        },
    )
    .await?;

    db::create_job_log(
        &pool,
        db::CreateJobLog {
            job_id: job.id.clone(),
            level: db::LogLevel::Warn,
            message: "Warning message".to_string(),
            metadata: None,
        },
    )
    .await?;

    // List logs
    let logs = db::list_job_logs(&pool, &job.id).await?;
    assert_eq!(logs.len(), 2);

    Ok(())
}

/// Test repository CRUD
#[tokio::test]
async fn test_repository_crud() -> Result<()> {
    let pool = db::init_pool(":memory:").await?;
    db::migrate(&pool).await?;

    // Create project first
    db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-1".to_string(),
            name: "Test".to_string(),
            slug: "test".to_string(),
            description: None,
        },
    )
    .await?;

    // Create repository
    let repo = db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: db::GitProvider::GitHub,
            owner: "test-owner".to_string(),
            repo: "test-repo".to_string(),
            branch: "main".to_string(),
            access_token: "token123".to_string(),
            local_path: None,
        },
    )
    .await?;

    assert_eq!(repo.owner, "test-owner");
    assert_eq!(repo.repo, "test-repo");
    assert_eq!(repo.full_name(), "test-owner/test-repo");

    // Get repository
    let fetched = db::get_repository(&pool, &repo.id).await?;
    assert_eq!(fetched.id, repo.id);

    // List repositories
    let repos = db::list_project_repositories(&pool, "proj-1").await?;
    assert_eq!(repos.len(), 1);

    // Delete repository
    db::delete_repository(&pool, &repo.id).await?;
    let repos = db::list_project_repositories(&pool, "proj-1").await?;
    assert_eq!(repos.len(), 0);

    Ok(())
}

// ============================================================================
// Embedding Service Tests
// ============================================================================

#[tokio::test]
async fn test_embedding_hash_fallback() -> Result<()> {
    use fold::config::EmbeddingConfig;
    use fold::services::EmbeddingService;

    // Create service with no providers (will use hash fallback)
    let config = EmbeddingConfig {
        providers: vec![],
        dimension: 384,
    };

    let service = EmbeddingService::new(&config)?;

    // Should use hash fallback
    assert!(!service.has_providers());
    assert_eq!(service.dimension(), 384);

    // Generate embeddings
    let embeddings = service
        .embed(vec!["test text".to_string()])
        .await?;

    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].len(), 384);

    // Embeddings should be deterministic
    let embeddings2 = service
        .embed(vec!["test text".to_string()])
        .await?;

    assert_eq!(embeddings[0], embeddings2[0]);

    Ok(())
}

#[tokio::test]
async fn test_embedding_normalization() -> Result<()> {
    use fold::config::EmbeddingConfig;
    use fold::services::EmbeddingService;

    let config = EmbeddingConfig {
        providers: vec![],
        dimension: 384,
    };

    let service = EmbeddingService::new(&config)?;

    let embeddings = service
        .embed(vec!["some random text".to_string()])
        .await?;

    // Check that embedding is normalized (unit vector)
    let norm: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.001);

    Ok(())
}

#[tokio::test]
async fn test_embedding_batch() -> Result<()> {
    use fold::config::EmbeddingConfig;
    use fold::services::EmbeddingService;

    let config = EmbeddingConfig {
        providers: vec![],
        dimension: 256,
    };

    let service = EmbeddingService::new(&config)?;

    // Test batch processing
    let texts = vec![
        "first text".to_string(),
        "second text".to_string(),
        "third text".to_string(),
    ];

    let embeddings = service.embed_batch(texts, 2).await?;

    assert_eq!(embeddings.len(), 3);
    for emb in &embeddings {
        assert_eq!(emb.len(), 256);
    }

    // Each text should produce different embeddings
    assert_ne!(embeddings[0], embeddings[1]);
    assert_ne!(embeddings[1], embeddings[2]);

    Ok(())
}

#[tokio::test]
async fn test_embedding_single() -> Result<()> {
    use fold::config::EmbeddingConfig;
    use fold::services::EmbeddingService;

    let config = EmbeddingConfig {
        providers: vec![],
        dimension: 512,
    };

    let service = EmbeddingService::new(&config)?;

    let embedding = service.embed_single("single text").await?;
    assert_eq!(embedding.len(), 512);

    // Verify normalization
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.001);

    Ok(())
}

#[tokio::test]
async fn test_empty_input() -> Result<()> {
    use fold::config::EmbeddingConfig;
    use fold::services::EmbeddingService;

    let config = EmbeddingConfig {
        providers: vec![],
        dimension: 384,
    };

    let service = EmbeddingService::new(&config)?;

    // Empty input should return empty result
    let embeddings = service.embed(vec![]).await?;
    assert!(embeddings.is_empty());

    Ok(())
}
