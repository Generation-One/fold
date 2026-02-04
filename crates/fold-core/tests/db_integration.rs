//! Database integration tests for Fold server.
//!
//! Tests the database CRUD operations using SQLite in-memory database.
//! Each test runs migrations and operates in isolation.

use fold_core::db;
use fold_core::Error;
use fold_core::Result;

// ============================================================================
// Test Setup Helper
// ============================================================================

/// Set up a fresh in-memory database with migrations applied.
async fn setup_test_db() -> db::DbPool {
    let pool = db::init_pool(":memory:")
        .await
        .expect("Failed to init pool");
    db::migrate(&pool).await.expect("Failed to run migrations");
    pool
}

/// Set up database with a test project pre-created.
async fn setup_test_db_with_project() -> db::DbPool {
    let pool = setup_test_db().await;
    db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-1".to_string(),
            slug: "test-project".to_string(),
            name: "Test Project".to_string(),
            description: Some("A test project".to_string()),
        },
    )
    .await
    .expect("Failed to create test project");
    pool
}

/// Set up database with project and user for session/workspace tests.
async fn setup_test_db_with_user() -> db::DbPool {
    let pool = setup_test_db_with_project().await;

    db::create_user(
        &pool,
        db::CreateUser {
            id: "user-1".to_string(),
            provider: "google".to_string(),
            subject: "sub-123".to_string(),
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            avatar_url: None,
            role: db::UserRole::Member,
        },
    )
    .await
    .expect("Failed to create test user");

    db::create_api_token(
        &pool,
        db::CreateApiToken {
            id: "token-1".to_string(),
            user_id: "user-1".to_string(),
            name: "Test Token".to_string(),
            token_hash: "hash123".to_string(),
            token_prefix: "fold_".to_string(),
            project_ids: vec!["proj-1".to_string()],
            expires_at: None,
        },
    )
    .await
    .expect("Failed to create test token");

    pool
}

// ============================================================================
// Project Tests
// ============================================================================

#[tokio::test]
async fn test_project_create() -> Result<()> {
    let pool = setup_test_db().await;

    let project = db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-new".to_string(),
            slug: "new-project".to_string(),
            name: "New Project".to_string(),
            description: Some("Description".to_string()),
        },
    )
    .await?;

    assert_eq!(project.id, "proj-new");
    assert_eq!(project.slug, "new-project");
    assert_eq!(project.name, "New Project");
    assert_eq!(project.description, Some("Description".to_string()));
    assert!(!project.created_at.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_project_get_by_id() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let project = db::get_project(&pool, "proj-1").await?;
    assert_eq!(project.id, "proj-1");
    assert_eq!(project.slug, "test-project");

    Ok(())
}

#[tokio::test]
async fn test_project_get_by_slug() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let project = db::get_project_by_slug(&pool, "test-project").await?;
    assert!(project.is_some());
    assert_eq!(project.unwrap().id, "proj-1");

    // Non-existent slug
    let missing = db::get_project_by_slug(&pool, "nonexistent").await?;
    assert!(missing.is_none());

    Ok(())
}

#[tokio::test]
async fn test_project_get_by_id_or_slug() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Get by ID
    let by_id = db::get_project_by_id_or_slug(&pool, "proj-1").await?;
    assert_eq!(by_id.slug, "test-project");

    // Get by slug
    let by_slug = db::get_project_by_id_or_slug(&pool, "test-project").await?;
    assert_eq!(by_slug.id, "proj-1");

    Ok(())
}

#[tokio::test]
async fn test_project_update() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let updated = db::update_project(
        &pool,
        "proj-1",
        db::UpdateProject {
            name: Some("Updated Name".to_string()),
            description: Some("Updated description".to_string()),
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.description, Some("Updated description".to_string()));
    assert_eq!(updated.slug, "test-project"); // Unchanged

    Ok(())
}

#[tokio::test]
async fn test_project_delete() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Delete project
    db::delete_project(&pool, "proj-1").await?;

    // Verify it's gone
    let result = db::get_project(&pool, "proj-1").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

#[tokio::test]
async fn test_project_list() -> Result<()> {
    let pool = setup_test_db().await;

    // Create multiple projects
    for i in 1..=3 {
        db::create_project(
            &pool,
            db::CreateProject {
                id: format!("proj-{}", i),
                slug: format!("project-{}", i),
                name: format!("Project {}", i),
                description: None,
            },
        )
        .await?;
    }

    let projects = db::list_projects(&pool).await?;
    assert_eq!(projects.len(), 3);

    // Test pagination
    let paginated = db::list_projects_paginated(&pool, 2, 0).await?;
    assert_eq!(paginated.len(), 2);

    let second_page = db::list_projects_paginated(&pool, 2, 2).await?;
    assert_eq!(second_page.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_project_duplicate_slug_error() -> Result<()> {
    let pool = setup_test_db().await;

    db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-1".to_string(),
            slug: "unique-slug".to_string(),
            name: "Project 1".to_string(),
            description: None,
        },
    )
    .await?;

    // Try to create another with same slug
    let result = db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-2".to_string(),
            slug: "unique-slug".to_string(),
            name: "Project 2".to_string(),
            description: None,
        },
    )
    .await;

    assert!(matches!(result, Err(Error::AlreadyExists(_))));

    Ok(())
}

#[tokio::test]
async fn test_project_not_found() -> Result<()> {
    let pool = setup_test_db().await;

    let result = db::get_project(&pool, "nonexistent").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    let result = db::update_project(
        &pool,
        "nonexistent",
        db::UpdateProject {
            name: Some("Test".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    let result = db::delete_project(&pool, "nonexistent").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

// ============================================================================
// Memory Tests
// ============================================================================

#[tokio::test]
async fn test_memory_create_and_get() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let memory = db::create_memory(
        &pool,
        db::CreateMemory {
            id: "mem-1".to_string(),
            project_id: "proj-1".to_string(),
            repository_id: None,
            memory_type: db::MemoryType::General,
            title: Some("Test Memory".to_string()),
            content: Some("This is test content".to_string()),
            content_hash: Some("hash123".to_string()),
            content_storage: "filesystem".to_string(),
            file_path: None,
            language: None,
            git_branch: None,
            git_commit_sha: None,
            author: Some("tester".to_string()),
            keywords: Some(vec!["test".to_string(), "memory".to_string()]),
            tags: Some(vec!["important".to_string()]),
        },
    )
    .await?;

    assert_eq!(memory.id, "mem-1");
    assert_eq!(memory.memory_type, "general");
    assert_eq!(memory.title, Some("Test Memory".to_string()));
    assert_eq!(memory.keywords_vec(), vec!["test", "memory"]);
    assert_eq!(memory.tags_vec(), vec!["important"]);

    // Fetch and verify
    let fetched = db::get_memory(&pool, "mem-1").await?;
    assert_eq!(fetched.content, Some("This is test content".to_string()));
    assert_eq!(fetched.author, Some("tester".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_memory_update() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_memory(
        &pool,
        db::CreateMemory {
            id: "mem-1".to_string(),
            project_id: "proj-1".to_string(),
            repository_id: None,
            memory_type: db::MemoryType::Decision,
            title: Some("Original Title".to_string()),
            content: Some("Original content".to_string()),
            content_hash: None,
            content_storage: "filesystem".to_string(),
            file_path: None,
            language: None,
            git_branch: None,
            git_commit_sha: None,
            author: None,
            keywords: None,
            tags: None,
        },
    )
    .await?;

    let updated = db::update_memory(
        &pool,
        "mem-1",
        db::UpdateMemory {
            title: Some("Updated Title".to_string()),
            content: Some("Updated content".to_string()),
            keywords: Some(vec!["new".to_string(), "keywords".to_string()]),
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(updated.title, Some("Updated Title".to_string()));
    assert_eq!(updated.content, Some("Updated content".to_string()));
    assert_eq!(updated.keywords_vec(), vec!["new", "keywords"]);

    Ok(())
}

#[tokio::test]
async fn test_memory_delete() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_memory(
        &pool,
        db::CreateMemory {
            id: "mem-1".to_string(),
            project_id: "proj-1".to_string(),
            repository_id: None,
            memory_type: db::MemoryType::General,
            title: None,
            content: Some("Content".to_string()),
            content_hash: None,
            content_storage: "filesystem".to_string(),
            file_path: None,
            language: None,
            git_branch: None,
            git_commit_sha: None,
            author: None,
            keywords: None,
            tags: None,
        },
    )
    .await?;

    db::delete_memory(&pool, "mem-1").await?;

    let result = db::get_memory(&pool, "mem-1").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

#[tokio::test]
async fn test_memory_list_with_filters() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories of different types
    for (i, mem_type) in [
        db::MemoryType::Decision,
        db::MemoryType::Task,
        db::MemoryType::Decision,
        db::MemoryType::Spec,
    ]
    .iter()
    .enumerate()
    {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: *mem_type,
                title: Some(format!("Memory {}", i)),
                content: Some(format!("Content {}", i)),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: if i % 2 == 0 {
                    Some("alice".to_string())
                } else {
                    Some("bob".to_string())
                },
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    // Filter by type
    let decisions =
        db::list_project_memories_by_type(&pool, "proj-1", db::MemoryType::Decision, 10, 0).await?;
    assert_eq!(decisions.len(), 2);

    // Filter by project
    let all = db::list_project_memories(&pool, "proj-1", 10, 0).await?;
    assert_eq!(all.len(), 4);

    // Filter using MemoryFilter
    let filtered = db::list_memories(
        &pool,
        db::MemoryFilter {
            project_id: Some("proj-1".to_string()),
            author: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(filtered.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_memory_upsert() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let input = db::CreateMemory {
        id: "mem-1".to_string(),
        project_id: "proj-1".to_string(),
        repository_id: None,
        memory_type: db::MemoryType::Codebase,
        title: Some("File".to_string()),
        content: Some("Original content".to_string()),
        content_hash: Some("hash1".to_string()),
        content_storage: "filesystem".to_string(),
        file_path: Some("src/main.rs".to_string()),
        language: Some("rust".to_string()),
        git_branch: None,
        git_commit_sha: None,
        author: None,
        keywords: None,
        tags: None,
    };

    // First upsert creates
    let created = db::upsert_memory(&pool, input.clone()).await?;
    assert_eq!(created.content, Some("Original content".to_string()));

    // Second upsert updates
    let mut updated_input = input;
    updated_input.content = Some("Updated content".to_string());
    updated_input.content_hash = Some("hash2".to_string());

    let updated = db::upsert_memory(&pool, updated_input).await?;
    assert_eq!(updated.content, Some("Updated content".to_string()));
    assert_eq!(updated.content_hash, Some("hash2".to_string()));

    // Verify only one memory exists
    let count = db::count_project_memories(&pool, "proj-1").await?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn test_memory_not_found_errors() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let result = db::get_memory(&pool, "nonexistent").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    let result = db::update_memory(
        &pool,
        "nonexistent",
        db::UpdateMemory {
            content: Some("test".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    let result = db::delete_memory(&pool, "nonexistent").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

// ============================================================================
// Repository Tests
// ============================================================================

#[tokio::test]
async fn test_repository_create_and_get() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let repo = db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: db::GitProvider::GitHub,
            owner: "testorg".to_string(),
            repo: "testrepo".to_string(),
            branch: "main".to_string(),
            access_token: "token123".to_string(),
            local_path: None,
        },
    )
    .await?;

    assert_eq!(repo.id, "repo-1");
    assert_eq!(repo.full_name(), "testorg/testrepo");
    assert_eq!(repo.url(), "https://github.com/testorg/testrepo");

    let fetched = db::get_repository(&pool, "repo-1").await?;
    assert_eq!(fetched.branch, "main");

    Ok(())
}

#[tokio::test]
async fn test_repository_update() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: db::GitProvider::GitHub,
            owner: "testorg".to_string(),
            repo: "testrepo".to_string(),
            branch: "main".to_string(),
            access_token: "old-token".to_string(),
            local_path: None,
        },
    )
    .await?;

    let updated = db::update_repository(
        &pool,
        "repo-1",
        db::UpdateRepository {
            branch: Some("develop".to_string()),
            access_token: Some("new-token".to_string()),
            webhook_id: Some("wh-123".to_string()),
            webhook_secret: Some("secret".to_string()),
            notification_type: None,
            last_indexed_at: None,
            last_commit_sha: None,
            last_sync: None,
            sync_cursor: None,
            local_path: None,
        },
    )
    .await?;

    assert_eq!(updated.branch, "develop");
    assert_eq!(updated.access_token, "new-token");
    assert_eq!(updated.webhook_id, Some("wh-123".to_string()));
    assert_eq!(updated.webhook_secret, Some("secret".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_repository_delete() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: db::GitProvider::GitHub,
            owner: "testorg".to_string(),
            repo: "testrepo".to_string(),
            branch: "main".to_string(),
            access_token: "token".to_string(),
            local_path: None,
        },
    )
    .await?;

    db::delete_repository(&pool, "repo-1").await?;

    let result = db::get_repository(&pool, "repo-1").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

#[tokio::test]
async fn test_repository_list_by_project() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create another project
    db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-2".to_string(),
            slug: "project-2".to_string(),
            name: "Project 2".to_string(),
            description: None,
        },
    )
    .await?;

    // Create repos for both projects
    db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-1".to_string(),
            project_id: "proj-1".to_string(),
            provider: db::GitProvider::GitHub,
            owner: "org".to_string(),
            repo: "repo1".to_string(),
            branch: "main".to_string(),
            access_token: "token".to_string(),
            local_path: None,
        },
    )
    .await?;

    db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-2".to_string(),
            project_id: "proj-1".to_string(),
            provider: db::GitProvider::GitHub,
            owner: "org".to_string(),
            repo: "repo2".to_string(),
            branch: "main".to_string(),
            access_token: "token".to_string(),
            local_path: None,
        },
    )
    .await?;

    db::create_repository(
        &pool,
        db::CreateRepository {
            id: "repo-3".to_string(),
            project_id: "proj-2".to_string(),
            provider: db::GitProvider::GitLab,
            owner: "org".to_string(),
            repo: "repo3".to_string(),
            branch: "main".to_string(),
            access_token: "token".to_string(),
            local_path: None,
        },
    )
    .await?;

    let proj1_repos = db::list_project_repositories(&pool, "proj-1").await?;
    assert_eq!(proj1_repos.len(), 2);

    let proj2_repos = db::list_project_repositories(&pool, "proj-2").await?;
    assert_eq!(proj2_repos.len(), 1);
    assert_eq!(proj2_repos[0].provider, "gitlab");

    Ok(())
}

#[tokio::test]
async fn test_repository_not_found() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let result = db::get_repository(&pool, "nonexistent").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    let result = db::delete_repository(&pool, "nonexistent").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

// ============================================================================
// AI Session Tests
// ============================================================================

#[tokio::test]
async fn test_session_create_and_get() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    let session = db::create_ai_session(
        &pool,
        db::CreateAiSession {
            id: "sess-1".to_string(),
            project_id: "proj-1".to_string(),
            task: "Implement authentication".to_string(),
            local_root: Some("/home/user/project".to_string()),
            repository_id: None,
            agent_type: Some("claude-code".to_string()),
        },
    )
    .await?;

    assert_eq!(session.id, "sess-1");
    assert!(session.is_active());
    assert!(!session.is_ended());
    assert_eq!(session.task, "Implement authentication");

    let fetched = db::get_ai_session(&pool, "sess-1").await?;
    assert_eq!(fetched.agent_type, Some("claude-code".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_session_update_and_end() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_ai_session(
        &pool,
        db::CreateAiSession {
            id: "sess-1".to_string(),
            project_id: "proj-1".to_string(),
            task: "Test task".to_string(),
            local_root: None,
            repository_id: None,
            agent_type: None,
        },
    )
    .await?;

    // Update session
    let updated = db::update_ai_session(
        &pool,
        "sess-1",
        db::UpdateAiSession {
            status: Some(db::SessionStatus::Paused),
            summary: Some("Paused for review".to_string()),
            next_steps: Some(vec!["Fix tests".to_string(), "Deploy".to_string()]),
        },
    )
    .await?;

    assert_eq!(updated.status, "paused");
    assert_eq!(updated.summary, Some("Paused for review".to_string()));
    assert_eq!(updated.next_steps_vec(), vec!["Fix tests", "Deploy"]);

    // End session
    let ended = db::end_ai_session(&pool, "sess-1", Some("Completed successfully")).await?;
    assert_eq!(ended.status, "completed");
    assert!(ended.is_ended());
    assert!(ended.ended_at.is_some());

    Ok(())
}

#[tokio::test]
async fn test_session_notes() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_ai_session(
        &pool,
        db::CreateAiSession {
            id: "sess-1".to_string(),
            project_id: "proj-1".to_string(),
            task: "Test task".to_string(),
            local_root: None,
            repository_id: None,
            agent_type: None,
        },
    )
    .await?;

    // Create notes
    db::create_session_note(
        &pool,
        db::CreateSessionNote {
            id: "note-1".to_string(),
            session_id: "sess-1".to_string(),
            note_type: db::NoteType::Decision,
            content: "Decided to use REST API".to_string(),
        },
    )
    .await?;

    db::create_session_note(
        &pool,
        db::CreateSessionNote {
            id: "note-2".to_string(),
            session_id: "sess-1".to_string(),
            note_type: db::NoteType::Progress,
            content: "Completed initial setup".to_string(),
        },
    )
    .await?;

    db::create_session_note(
        &pool,
        db::CreateSessionNote {
            id: "note-3".to_string(),
            session_id: "sess-1".to_string(),
            note_type: db::NoteType::Blocker,
            content: "Need API keys".to_string(),
        },
    )
    .await?;

    // List all notes
    let notes = db::list_session_notes(&pool, "sess-1").await?;
    assert_eq!(notes.len(), 3);

    // Filter by type
    let decisions = db::list_session_notes_by_type(&pool, "sess-1", db::NoteType::Decision).await?;
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].content, "Decided to use REST API");

    // Get specific note
    let note = db::get_session_note(&pool, "note-2").await?;
    assert_eq!(note.note_type, "progress");

    Ok(())
}

#[tokio::test]
async fn test_session_list_by_status() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create multiple sessions
    for i in 1..=3 {
        db::create_ai_session(
            &pool,
            db::CreateAiSession {
                id: format!("sess-{}", i),
                project_id: "proj-1".to_string(),
                task: format!("Task {}", i),
                local_root: None,
                repository_id: None,
                agent_type: None,
            },
        )
        .await?;
    }

    // End one session
    db::end_ai_session(&pool, "sess-2", None).await?;

    // List active sessions
    let active = db::list_active_ai_sessions(&pool, "proj-1").await?;
    assert_eq!(active.len(), 2);

    // List by status
    let completed = db::list_ai_sessions_by_status(&pool, db::SessionStatus::Completed).await?;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].id, "sess-2");

    Ok(())
}

#[tokio::test]
async fn test_session_delete() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    db::create_ai_session(
        &pool,
        db::CreateAiSession {
            id: "sess-1".to_string(),
            project_id: "proj-1".to_string(),
            task: "Test".to_string(),
            local_root: None,
            repository_id: None,
            agent_type: None,
        },
    )
    .await?;

    db::delete_ai_session(&pool, "sess-1").await?;

    let result = db::get_ai_session(&pool, "sess-1").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

// ============================================================================
// Link Tests
// ============================================================================

#[tokio::test]
async fn test_link_create_and_get() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create two memories to link
    for i in 1..=2 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: Some(format!("Memory {}", i)),
                content: Some(format!("Content {}", i)),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    let link = db::create_link(
        &pool,
        db::CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::References,
            created_by: db::LinkCreator::User,
            confidence: Some(0.9),
            context: Some("Referenced in discussion".to_string()),
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    assert_eq!(link.id, "link-1");
    assert_eq!(link.link_type, "references");
    assert_eq!(link.created_by, "user");
    assert_eq!(link.confidence, Some(0.9));

    let fetched = db::get_link(&pool, "link-1").await?;
    assert_eq!(
        fetched.context,
        Some("Referenced in discussion".to_string())
    );

    Ok(())
}

#[tokio::test]
async fn test_link_get_by_endpoints() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories
    for i in 1..=2 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::DependsOn,
            created_by: db::LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    let found =
        db::get_link_by_endpoints(&pool, "mem-1", "mem-2", &db::LinkType::DependsOn).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "link-1");

    let not_found =
        db::get_link_by_endpoints(&pool, "mem-1", "mem-2", &db::LinkType::References).await?;
    assert!(not_found.is_none());

    Ok(())
}

#[tokio::test]
async fn test_link_list_directions() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories
    for i in 1..=3 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    // mem-1 -> mem-2, mem-3 -> mem-2
    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::References,
            created_by: db::LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-2".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-3".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::Implements,
            created_by: db::LinkCreator::Ai,
            confidence: Some(0.8),
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    // Outgoing from mem-1
    let outgoing = db::list_outgoing_links(&pool, "mem-1").await?;
    assert_eq!(outgoing.len(), 1);

    // Incoming to mem-2
    let incoming = db::list_incoming_links(&pool, "mem-2").await?;
    assert_eq!(incoming.len(), 2);

    // All links for mem-2
    let all = db::list_memory_links(&pool, "mem-2").await?;
    assert_eq!(all.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_link_delete() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories
    for i in 1..=2 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::Related,
            created_by: db::LinkCreator::User,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    // Delete by ID
    db::delete_link(&pool, "link-1").await?;

    let result = db::get_link(&pool, "link-1").await;
    assert!(matches!(result, Err(Error::NotFound(_))));

    Ok(())
}

#[tokio::test]
async fn test_link_graph_traversal() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create chain: mem-1 -> mem-2 -> mem-3 -> mem-4
    for i in 1..=4 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    for i in 1..=3 {
        db::create_link(
            &pool,
            db::CreateLink {
                id: format!("link-{}", i),
                project_id: "proj-1".to_string(),
                source_id: format!("mem-{}", i),
                target_id: format!("mem-{}", i + 1),
                link_type: db::LinkType::References,
                created_by: db::LinkCreator::System,
                confidence: None,
                context: None,
                change_type: None,
                additions: None,
                deletions: None,
            },
        )
        .await?;
    }

    // Traverse from mem-1 with depth 2
    let nodes = db::traverse_graph(&pool, "mem-1", 2, None).await?;
    assert_eq!(nodes.len(), 2); // mem-2 at depth 1, mem-3 at depth 2

    // Traverse with depth 3 should get all
    let all_nodes = db::traverse_graph(&pool, "mem-1", 3, None).await?;
    assert_eq!(all_nodes.len(), 3);

    // Find path from mem-1 to mem-4
    let path = db::find_path(&pool, "mem-1", "mem-4", 5).await?;
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path, vec!["mem-1", "mem-2", "mem-3", "mem-4"]);

    Ok(())
}

#[tokio::test]
async fn test_link_duplicate_error() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories
    for i in 1..=2 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::References,
            created_by: db::LinkCreator::User,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    // Try to create duplicate link (same endpoints and type)
    let result = db::create_link(
        &pool,
        db::CreateLink {
            id: "link-2".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::References,
            created_by: db::LinkCreator::User,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await;

    assert!(matches!(result, Err(Error::AlreadyExists(_))));

    Ok(())
}

// ============================================================================
// Workspace Tests
// ============================================================================

#[tokio::test]
async fn test_workspace_create_and_get() -> Result<()> {
    let pool = setup_test_db_with_user().await;

    let workspace = db::create_workspace(
        &pool,
        db::CreateWorkspace {
            id: "ws-1".to_string(),
            project_id: "proj-1".to_string(),
            token_id: "token-1".to_string(),
            local_root: "/home/user/project".to_string(),
            repository_id: None,
            expires_at: None,
        },
    )
    .await?;

    assert_eq!(workspace.id, "ws-1");
    assert!(!workspace.is_expired());

    let fetched = db::get_workspace(&pool, "ws-1").await?;
    assert_eq!(fetched.local_root, "/home/user/project");

    Ok(())
}

#[tokio::test]
async fn test_workspace_find_by_local_root() -> Result<()> {
    let pool = setup_test_db_with_user().await;

    db::create_workspace(
        &pool,
        db::CreateWorkspace {
            id: "ws-1".to_string(),
            project_id: "proj-1".to_string(),
            token_id: "token-1".to_string(),
            local_root: "/home/user/project".to_string(),
            repository_id: None,
            expires_at: None,
        },
    )
    .await?;

    let found = db::get_workspace_by_local_root(&pool, "token-1", "/home/user/project").await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "ws-1");

    let not_found = db::get_workspace_by_local_root(&pool, "token-1", "/other/path").await?;
    assert!(not_found.is_none());

    Ok(())
}

// ============================================================================
// Team Status Tests
// ============================================================================

#[tokio::test]
async fn test_team_status_upsert() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create initial status
    let status = db::upsert_team_status(
        &pool,
        "proj-1",
        "alice",
        db::UpdateTeamStatus {
            status: db::TeamMemberStatus::Active,
            current_task: Some("Working on auth".to_string()),
            current_files: Some(vec!["src/auth.rs".to_string()]),
        },
    )
    .await?;

    assert_eq!(status.username, "alice");
    assert_eq!(status.status_enum(), db::TeamMemberStatus::Active);
    assert_eq!(status.current_files_vec(), vec!["src/auth.rs"]);

    // Update via upsert
    let updated = db::upsert_team_status(
        &pool,
        "proj-1",
        "alice",
        db::UpdateTeamStatus {
            status: db::TeamMemberStatus::Idle,
            current_task: None,
            current_files: None,
        },
    )
    .await?;

    assert_eq!(updated.status_enum(), db::TeamMemberStatus::Idle);

    Ok(())
}

#[tokio::test]
async fn test_team_status_list() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create statuses for multiple users
    for user in ["alice", "bob", "charlie"] {
        db::upsert_team_status(
            &pool,
            "proj-1",
            user,
            db::UpdateTeamStatus {
                status: if user == "bob" {
                    db::TeamMemberStatus::Away
                } else {
                    db::TeamMemberStatus::Active
                },
                current_task: Some(format!("{}'s task", user)),
                current_files: None,
            },
        )
        .await?;
    }

    let all = db::list_team_status(&pool, "proj-1").await?;
    assert_eq!(all.len(), 3);

    let active = db::list_active_team_members(&pool, "proj-1").await?;
    assert_eq!(active.len(), 2);

    Ok(())
}

// ============================================================================
// Count and Search Tests
// ============================================================================

#[tokio::test]
async fn test_project_search() -> Result<()> {
    let pool = setup_test_db().await;

    db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-1".to_string(),
            slug: "awesome-api".to_string(),
            name: "Awesome API".to_string(),
            description: Some("Backend service".to_string()),
        },
    )
    .await?;

    db::create_project(
        &pool,
        db::CreateProject {
            id: "proj-2".to_string(),
            slug: "frontend-app".to_string(),
            name: "Frontend App".to_string(),
            description: Some("React application".to_string()),
        },
    )
    .await?;

    // Search by name
    let results = db::search_projects(&pool, "awesome").await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].slug, "awesome-api");

    // Search by description
    let results = db::search_projects(&pool, "React").await?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].slug, "frontend-app");

    // Search by slug
    let results = db::search_projects(&pool, "api").await?;
    assert_eq!(results.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_project_count() -> Result<()> {
    let pool = setup_test_db().await;

    assert_eq!(db::count_projects(&pool).await?, 0);

    for i in 1..=5 {
        db::create_project(
            &pool,
            db::CreateProject {
                id: format!("proj-{}", i),
                slug: format!("project-{}", i),
                name: format!("Project {}", i),
                description: None,
            },
        )
        .await?;
    }

    assert_eq!(db::count_projects(&pool).await?, 5);

    Ok(())
}

#[tokio::test]
async fn test_memory_count_by_type() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories of different types
    for (i, mem_type) in [
        db::MemoryType::Decision,
        db::MemoryType::Decision,
        db::MemoryType::Task,
        db::MemoryType::Spec,
    ]
    .iter()
    .enumerate()
    {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: *mem_type,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    assert_eq!(db::count_project_memories(&pool, "proj-1").await?, 4);
    assert_eq!(
        db::count_project_memories_by_type(&pool, "proj-1", db::MemoryType::Decision).await?,
        2
    );
    assert_eq!(
        db::count_project_memories_by_type(&pool, "proj-1", db::MemoryType::Task).await?,
        1
    );
    assert_eq!(
        db::count_project_memories_by_type(&pool, "proj-1", db::MemoryType::General).await?,
        0
    );

    Ok(())
}

#[tokio::test]
async fn test_link_count() -> Result<()> {
    let pool = setup_test_db_with_project().await;

    // Create memories
    for i in 1..=3 {
        db::create_memory(
            &pool,
            db::CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: db::MemoryType::General,
                title: None,
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await?;
    }

    assert_eq!(db::count_project_links(&pool, "proj-1").await?, 0);

    // Create links
    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: db::LinkType::References,
            created_by: db::LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    db::create_link(
        &pool,
        db::CreateLink {
            id: "link-2".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-2".to_string(),
            target_id: "mem-3".to_string(),
            link_type: db::LinkType::DependsOn,
            created_by: db::LinkCreator::Ai,
            confidence: Some(0.95),
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        },
    )
    .await?;

    assert_eq!(db::count_project_links(&pool, "proj-1").await?, 2);

    Ok(())
}
