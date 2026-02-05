//! API Integration Tests for Fold Server
//!
//! Tests the REST API endpoints using axum-test.
//! Uses in-memory SQLite and mock services where needed.

mod common;

use axum::{
    http::{header::AUTHORIZATION, HeaderValue, StatusCode},
    Router,
};
use axum_test::TestServer;
use fold::api;
use fold::config::{AuthConfig, EmbeddingConfig, LlmConfig, QdrantConfig};
use fold::db::{self, DbPool};
use fold::services::{
    AuthService, EmbeddingService, FoldStorageService, GitHubService, GitLabService,
    GitLocalService, GitService, GitSyncService, GraphService, IndexerService, LinkerService,
    LlmService, MemoryService, MetaStorageService, ProjectService, ProviderRegistry, QdrantService,
};
use fold::AppState;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Helper function to create a Bearer Authorization header value
fn bearer_auth(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("Bearer {}", token)).unwrap()
}

// ============================================================================
// Test Setup Helpers
// ============================================================================

/// Create a test database with migrations applied
async fn setup_test_db() -> DbPool {
    let pool = db::init_pool(":memory:").await.expect("Failed to create test database");
    db::migrate(&pool).await.expect("Failed to run migrations");
    pool
}

/// Create a test user and return the user ID
#[allow(dead_code)]
async fn create_test_user(pool: &DbPool) -> String {
    let user_id = nanoid::nanoid!();
    sqlx::query(
        "INSERT INTO users (id, provider, subject, email, display_name, created_at) VALUES (?, 'github', ?, 'test@test.com', 'Test User', datetime('now'))"
    )
    .bind(&user_id)
    .bind(nanoid::nanoid!())
    .execute(pool)
    .await
    .expect("Failed to create test user");
    user_id
}

/// Create a test API token and return the token string
async fn create_test_token(pool: &DbPool, user_id: &str, project_ids: Option<&str>) -> String {
    let token = format!("fold_test1234_{}", nanoid::nanoid!(16));
    let prefix = &token[5..13]; // "test1234"
    let hash = hex::encode(Sha256::digest(token.as_bytes()));

    // project_ids is NOT NULL in schema, use empty array if not provided
    let project_ids_json = project_ids.unwrap_or("[]");

    sqlx::query(
        r#"
        INSERT INTO api_tokens (id, user_id, name, token_prefix, token_hash, project_ids, created_at)
        VALUES (?, ?, 'Test Token', ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(nanoid::nanoid!())
    .bind(user_id)
    .bind(prefix)
    .bind(&hash)
    .bind(project_ids_json)
    .execute(pool)
    .await
    .expect("Failed to create test token");

    token
}

/// Create a test project in the database
async fn create_test_project(pool: &DbPool, slug: &str, name: &str) -> String {
    let project = db::create_project(
        pool,
        db::CreateProject {
            id: nanoid::nanoid!(),
            name: name.to_string(),
            slug: slug.to_string(),
            description: Some(format!("Test project: {}", name)),
        },
    )
    .await
    .expect("Failed to create test project");

    project.id
}

/// Build a test AppState with mocked services
async fn build_test_state(pool: DbPool) -> AppState {
    // Create mock/minimal service configurations
    let qdrant_config = QdrantConfig {
        url: "http://localhost:6334".to_string(),
        collection_prefix: "test_".to_string(),
    };

    let embedding_config = EmbeddingConfig {
        providers: vec![], // Use hash fallback
        dimension: 384,
    };

    let llm_config = LlmConfig {
        providers: vec![], // No providers = stub responses
    };

    let auth_config = AuthConfig {
        providers: std::collections::HashMap::new(),
        bootstrap_token: None,
    };

    // Initialize services (some will be mocked/stubbed)
    let qdrant = Arc::new(
        QdrantService::new(&qdrant_config)
            .await
            .expect("Failed to create Qdrant service"),
    );
    let embeddings = Arc::new(
        EmbeddingService::new(pool.clone(), &embedding_config)
            .await
            .expect("Failed to create embedding service"),
    );
    let llm = Arc::new(
        LlmService::new(pool.clone(), &llm_config)
            .await
            .expect("Failed to create LLM service"),
    );
    let github = Arc::new(GitHubService::new());
    let gitlab = Arc::new(GitLabService::new());
    let git_local = Arc::new(GitLocalService::new());

    // Initialize storage services
    let meta_storage = Arc::new(MetaStorageService::new(std::path::PathBuf::from("./test_fold")));
    let fold_storage = Arc::new(FoldStorageService::new());
    let content_resolver = Arc::new(fold::services::ContentResolverService::new(
        pool.clone(),
        meta_storage.clone(),
    ));

    // Initialize agentic memory service
    let memory = MemoryService::new(
        pool.clone(),
        qdrant.clone(),
        embeddings.clone(),
        llm.clone(),
        fold_storage.clone(),
    );

    let project = ProjectService::new(pool.clone(), qdrant.clone(), embeddings.clone());

    // Initialize git service
    let git_service = Arc::new(GitService::new(
        pool.clone(),
        memory.clone(),
        fold_storage.clone(),
        qdrant.clone(),
        embeddings.clone(),
    ));

    let indexer = IndexerService::with_git_service(memory.clone(), llm.clone(), git_service.clone());

    let git_sync = GitSyncService::new(
        pool.clone(),
        github.clone(),
        gitlab.clone(),
        memory.clone(),
        llm.clone(),
        indexer.clone(),
    );

    let graph = GraphService::new(pool.clone());

    let linker = LinkerService::new(
        pool.clone(),
        memory.clone(),
        llm.clone(),
        qdrant.clone(),
        embeddings.clone(),
    );

    let auth = AuthService::new(pool.clone(), auth_config);

    let providers = Arc::new(ProviderRegistry::with_defaults());

    AppState {
        db: pool,
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
    }
}

/// Build a test router with the API routes
async fn build_test_app() -> (TestServer, DbPool) {
    let pool = setup_test_db().await;

    // Create a default test user for token authentication
    sqlx::query(
        "INSERT INTO users (id, provider, subject, email, display_name, created_at) VALUES ('test-user', 'github', 'test-subject', 'test@test.com', 'Test User', datetime('now'))"
    )
    .execute(&pool)
    .await
    .expect("Failed to create test user");

    let state = build_test_state(pool.clone()).await;

    let app = Router::new()
        .merge(api::routes(state.clone()))
        .with_state(state);

    let server = TestServer::new(app).expect("Failed to create test server");

    (server, pool)
}

// ============================================================================
// Health Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_returns_healthy() {
    let (server, _pool) = build_test_app().await;

    let response = server.get("/health").await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["status"], "healthy");
    assert!(body["version"].is_string());
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_liveness_check_returns_ok() {
    let (server, _pool) = build_test_app().await;

    let response = server.get("/health/live").await;

    response.assert_status_ok();
}

#[tokio::test]
async fn test_readiness_check_returns_checks() {
    let (server, _pool) = build_test_app().await;

    let response = server.get("/health/ready").await;

    // Should return 200 or 503 depending on dependencies
    let status = response.status_code();
    assert!(status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE);

    let body: Value = response.json();
    assert!(body["ready"].is_boolean());
    assert!(body["checks"].is_array());
}

// ============================================================================
// Projects Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_list_projects_requires_auth() {
    let (server, _pool) = build_test_app().await;

    let response = server.get("/projects").await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn test_list_projects_with_valid_token() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;

    let response = server
        .get("/projects")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body["projects"].is_array());
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn test_create_project_success() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;

    let response = server
        .post("/projects")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "slug": "my-project",
            "name": "My Test Project",
            "description": "A test project for integration tests"
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["slug"], "my-project");
    assert_eq!(body["name"], "My Test Project");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn test_create_project_invalid_slug() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;

    let response = server
        .post("/projects")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "slug": "INVALID_SLUG!",
            "name": "Invalid Project"
        }))
        .await;

    response.assert_status_bad_request();
    let body: Value = response.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("lowercase"));
}

#[tokio::test]
async fn test_create_project_missing_required_fields() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;

    // Missing slug - Axum returns 422 for JSON validation errors
    let response = server
        .post("/projects")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "name": "Missing Slug Project"
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_get_project_not_found() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;

    let response = server
        .get("/projects/nonexistent-project")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .await;

    response.assert_status_not_found();
}

// ============================================================================
// Memories Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_list_memories_requires_auth() {
    let (server, pool) = build_test_app().await;

    let project_id = create_test_project(&pool, "mem-test", "Memory Test").await;

    let response = server.get(&format!("/projects/{}/memories", project_id)).await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn test_list_memories_empty_project() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "mem-test", "Memory Test").await;

    let response = server
        .get(&format!("/projects/{}/memories", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["memories"], json!([]));
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn test_create_memory_success() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "mem-create", "Memory Create Test").await;

    let response = server
        .post(&format!("/projects/{}/memories", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "content": "This is a test memory content that should be stored.",
            "type": "general",
            "title": "Test Memory",
            "tags": ["test", "integration"]
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["title"], "Test Memory");
    assert_eq!(body["type"], "general");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn test_create_memory_empty_content_fails() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "mem-empty", "Memory Empty Test").await;

    let response = server
        .post(&format!("/projects/{}/memories", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "content": "",
            "type": "general"
        }))
        .await;

    response.assert_status_bad_request();
    let body: Value = response.json();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("empty"));
}

#[tokio::test]
async fn test_search_memories_empty_query_fails() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "mem-search", "Memory Search Test").await;

    let response = server
        .post(&format!("/projects/{}/memories/search", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "query": ""
        }))
        .await;

    response.assert_status_bad_request();
}

#[tokio::test]
async fn test_search_memories_with_query() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "mem-search-2", "Memory Search Test 2").await;

    let response = server
        .post(&format!("/projects/{}/memories/search", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "query": "authentication implementation",
            "limit": 10
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body["results"].is_array());
    assert_eq!(body["query"], "authentication implementation");
}

// ============================================================================
// Search Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_unified_search_requires_auth() {
    let (server, pool) = build_test_app().await;

    let project_id = create_test_project(&pool, "search-test", "Search Test").await;

    let response = server
        .post(&format!("/projects/{}/search", project_id))
        .json(&json!({
            "query": "test query"
        }))
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn test_unified_search_success() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "search-test-2", "Search Test 2").await;

    let response = server
        .post(&format!("/projects/{}/search", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "query": "database connection handling",
            "include_code": true,
            "include_memories": true,
            "limit": 20
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["query"], "database connection handling");
    assert!(body["results"].is_array());
    assert!(body["took_ms"].is_number());
}

#[tokio::test]
async fn test_context_endpoint_success() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "context-test", "Context Test").await;

    let response = server
        .post(&format!("/projects/{}/context", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "task": "Implement user authentication with OAuth2",
            "include_code": true,
            "include_sessions": true,
            "limit": 15
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["task"], "Implement user authentication with OAuth2");
    assert!(body["context"].is_array());
    assert!(body["suggestions"].is_array());
}

// ============================================================================
// Sessions Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_list_sessions_requires_auth() {
    let (server, pool) = build_test_app().await;

    let project_id = create_test_project(&pool, "session-test", "Session Test").await;

    let response = server
        .get(&format!("/projects/{}/sessions", project_id))
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn test_start_session_success() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "session-start", "Session Start Test").await;

    let response = server
        .post(&format!("/projects/{}/sessions", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "title": "Feature Implementation Session",
            "focus": "Working on user authentication",
            "notes": "Starting work on OAuth2 integration"
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["title"], "Feature Implementation Session");
    assert_eq!(body["status"], "active");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn test_add_note_to_session() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "session-notes", "Session Notes Test").await;

    // First, start a session
    let start_response = server
        .post(&format!("/projects/{}/sessions", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "title": "Note Testing Session"
        }))
        .await;

    start_response.assert_status_ok();
    let session: Value = start_response.json();
    let session_id = session["id"].as_str().unwrap();

    // Add a note to the session
    let note_response = server
        .post(&format!(
            "/projects/{}/sessions/{}/notes",
            project_id, session_id
        ))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "content": "Found a bug in the authentication flow",
            "note_type": "bug",
            "tags": ["auth", "bug"]
        }))
        .await;

    note_response.assert_status_ok();
    let note: Value = note_response.json();
    assert_eq!(note["note_type"], "bug");
    assert!(note["content"]
        .as_str()
        .unwrap()
        .contains("authentication flow"));
}

#[tokio::test]
async fn test_add_empty_note_fails() {
    let (server, pool) = build_test_app().await;

    let token = create_test_token(&pool, "test-user", None).await;
    let project_id = create_test_project(&pool, "session-empty-note", "Empty Note Test").await;

    // Start a session
    let start_response = server
        .post(&format!("/projects/{}/sessions", project_id))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "title": "Empty Note Test Session"
        }))
        .await;

    let session: Value = start_response.json();
    let session_id = session["id"].as_str().unwrap();

    // Try to add empty note
    let note_response = server
        .post(&format!(
            "/projects/{}/sessions/{}/notes",
            project_id, session_id
        ))
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .json(&json!({
            "content": "   "
        }))
        .await;

    note_response.assert_status_bad_request();
}

// ============================================================================
// Token Authentication Tests
// ============================================================================

#[tokio::test]
async fn test_invalid_token_format_rejected() {
    let (server, _pool) = build_test_app().await;

    let response = server
        .get("/projects")
        .add_header(AUTHORIZATION, HeaderValue::from_static("Bearer invalid-token-format"))
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn test_expired_token_rejected() {
    let (server, pool) = build_test_app().await;

    // test-user already created by build_test_app
    // Create an expired token
    let token = format!("fold_test1234_{}", nanoid::nanoid!(16));
    let prefix = &token[5..13];
    let hash = hex::encode(Sha256::digest(token.as_bytes()));

    sqlx::query(
        r#"
        INSERT INTO api_tokens (id, user_id, name, token_prefix, token_hash, project_ids, expires_at, created_at)
        VALUES (?, 'test-user', 'Expired Token', ?, ?, '[]', datetime('now', '-1 day'), datetime('now', '-2 days'))
        "#,
    )
    .bind(nanoid::nanoid!())
    .bind(prefix)
    .bind(&hash)
    .execute(&pool)
    .await
    .expect("Failed to create expired token");

    let response = server
        .get("/projects")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .await;

    response.assert_status_unauthorized();
}

#[tokio::test]
async fn test_nonexistent_token_rejected() {
    let (server, _pool) = build_test_app().await;

    // Use a valid format token that doesn't exist in DB
    let token = format!("fold_test1234_{}", nanoid::nanoid!(16));

    let response = server
        .get("/projects")
        .add_header(AUTHORIZATION, bearer_auth(&token))
        .await;

    response.assert_status_unauthorized();
}

// ============================================================================
// Metrics Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_format() {
    let (server, _pool) = build_test_app().await;

    let response = server.get("/metrics").await;

    response.assert_status_ok();
    let body = response.text();
    assert!(body.contains("fold_requests_total"));
    assert!(body.contains("fold_errors_total"));
    assert!(body.contains("fold_up 1"));
}

// ============================================================================
// Status Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_system_status_returns_details() {
    let (server, _pool) = build_test_app().await;

    let response = server.get("/status").await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body["status"].is_string());
    assert!(body["version"].is_string());
    assert!(body["database"]["connected"].is_boolean());
    assert!(body["embeddings"]["dimension"].is_number());
}
