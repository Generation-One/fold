//! HTTP-based integration tests for v2 API.
//!
//! These tests make actual HTTP requests to a running Fold server and verify
//! the complete API behavior including:
//! - Project creation and management
//! - Memory operations with decay configuration
//! - Semantic search with different decay parameters
//! - User and group management
//! - API key operations
//!
//! Prerequisites:
//! - Fold server running on http://localhost:8765
//! - Valid API token in FOLD_TOKEN environment variable
//! - Qdrant vector database running

use serde_json::{json, Value};
use std::env;

// ============================================================================
// Test Utilities
// ============================================================================

struct TestClient {
    base_url: String,
    token: String,
    http_client: reqwest::Client,
}

impl TestClient {
    fn new() -> Self {
        let base_url = env::var("FOLD_URL").unwrap_or_else(|_| "http://localhost:8765".to_string());
        let token = env::var("FOLD_TOKEN").expect("FOLD_TOKEN environment variable required");

        Self {
            base_url,
            token,
            http_client: reqwest::Client::new(),
        }
    }

    async fn post(&self, path: &str, body: Value) -> reqwest::Result<reqwest::Response> {
        self.http_client
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
    }

    async fn get(&self, path: &str) -> reqwest::Result<reqwest::Response> {
        self.http_client
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .send()
            .await
    }

    async fn put(&self, path: &str, body: Value) -> reqwest::Result<reqwest::Response> {
        self.http_client
            .put(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
    }

    async fn delete(&self, path: &str) -> reqwest::Result<reqwest::Response> {
        self.http_client
            .delete(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .send()
            .await
    }
}

// ============================================================================
// PROJECT MANAGEMENT INTEGRATION TESTS
// ============================================================================

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_create_and_get_project() {
    let client = TestClient::new();
    let project_name = format!("test-proj-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let create_response = client
        .post(
            "/projects",
            json!({
                "name": project_name,
                "slug": project_slug,
                "description": "Test project"
            }),
        )
        .await
        .expect("Failed to create project");

    assert_eq!(create_response.status(), 201);
    let project: Value = create_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Get project
    let get_response = client
        .get(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to get project");

    assert_eq!(get_response.status(), 200);
    let fetched: Value = get_response.json().await.expect("Invalid response JSON");
    assert_eq!(fetched["id"], project_id);
    assert_eq!(fetched["decay_strength_weight"], 0.3);
    assert_eq!(fetched["decay_half_life_days"], 30.0);

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_update_project_decay_config() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let create_response = client
        .post(
            "/projects",
            json!({
                "name": "decay-test-project",
                "slug": project_slug,
                "description": "Test decay config"
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = create_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Update algorithm config
    let update_response = client
        .put(
            &format!("/projects/{}/config/algorithm", project_id),
            json!({
                "strength_weight": 0.5,
                "decay_half_life_days": 14.0,
                "ignored_commit_authors": ["ci-bot"]
            }),
        )
        .await
        .expect("Failed to update config");

    assert_eq!(update_response.status(), 200);
    let config: Value = update_response.json().await.expect("Invalid response JSON");
    assert_eq!(config["strength_weight"], 0.5);
    assert_eq!(config["decay_half_life_days"], 14.0);

    // Verify config persists
    let get_response = client
        .get(&format!("/projects/{}/config/algorithm", project_id))
        .await
        .expect("Failed to get config");

    let fetched_config: Value = get_response.json().await.expect("Invalid response JSON");
    assert_eq!(fetched_config["strength_weight"], 0.5);

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

// ============================================================================
// MEMORY OPERATIONS INTEGRATION TESTS
// ============================================================================

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_create_memory_without_type() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let project_response = client
        .post(
            "/projects",
            json!({
                "name": "mem-test",
                "slug": project_slug
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = project_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Create memory WITHOUT type field
    let memory_response = client
        .post(
            &format!("/projects/{}/memories", project_id),
            json!({
                "title": "Test Memory",
                "content": "This is test content",
                "author": "test-user",
                "tags": ["test"]
            }),
        )
        .await
        .expect("Failed to create memory");

    assert_eq!(memory_response.status(), 201);
    let memory: Value = memory_response.json().await.expect("Invalid response JSON");
    assert!(memory.get("id").is_some());
    assert_eq!(memory["title"], "Test Memory");
    assert!(memory.get("type").is_none(), "v2 should not have type field");
    assert_eq!(memory["source"], "agent");

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_memory_with_file_path_sets_source() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let project_response = client
        .post(
            "/projects",
            json!({
                "name": "file-mem-test",
                "slug": project_slug
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = project_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Create memory WITH file_path
    let memory_response = client
        .post(
            &format!("/projects/{}/memories", project_id),
            json!({
                "title": "Code File",
                "content": "fn main() {}",
                "file_path": "src/main.rs"
            }),
        )
        .await
        .expect("Failed to create memory");

    assert_eq!(memory_response.status(), 201);
    let memory: Value = memory_response.json().await.expect("Invalid response JSON");
    assert_eq!(memory["source"], "file", "source should be 'file' when file_path provided");

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

// ============================================================================
// SEMANTIC SEARCH WITH DECAY INTEGRATION TESTS
// ============================================================================

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_search_with_project_decay_config() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let project_response = client
        .post(
            "/projects",
            json!({
                "name": "search-decay-test",
                "slug": project_slug
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = project_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Create test memory
    let memory_response = client
        .post(
            &format!("/projects/{}/memories", project_id),
            json!({
                "title": "API Documentation",
                "content": "REST API authentication uses JWT tokens with RSA-256 signing"
            }),
        )
        .await
        .expect("Failed to create memory");

    let memory: Value = memory_response.json().await.expect("Invalid response JSON");
    assert!(memory.get("id").is_some());

    // Wait for embedding
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Search with default config
    let search_response = client
        .post(
            &format!("/projects/{}/search", project_id),
            json!({
                "query": "JWT authentication"
            }),
        )
        .await
        .expect("Failed to search");

    assert_eq!(search_response.status(), 200);
    let search_result: Value = search_response.json().await.expect("Invalid response JSON");
    assert!(search_result["results"].is_array());

    // Verify response includes decay fields
    if let Some(first_result) = search_result["results"].get(0) {
        assert!(first_result.get("score").is_some(), "Response should include 'score'");
        assert!(
            first_result.get("strength").is_some(),
            "Response should include 'strength' (v2)"
        );
        assert!(
            first_result.get("combined_score").is_some(),
            "Response should include 'combined_score' (v2)"
        );
    }

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_decay_config_affects_search_results() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let project_response = client
        .post(
            "/projects",
            json!({
                "name": "decay-effect-test",
                "slug": project_slug
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = project_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Create memory
    let _memory_response = client
        .post(
            &format!("/projects/{}/memories", project_id),
            json!({
                "title": "Test Memory",
                "content": "This is a test for decay effects"
            }),
        )
        .await
        .expect("Failed to create memory");

    // Wait for embedding
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Set strength_weight = 0 (pure semantic)
    let _config_response = client
        .put(
            &format!("/projects/{}/config/algorithm", project_id),
            json!({
                "strength_weight": 0.0
            }),
        )
        .await
        .expect("Failed to update config");

    // Search
    let search1: Value = client
        .post(
            &format!("/projects/{}/search", project_id),
            json!({
                "query": "test"
            }),
        )
        .await
        .expect("Failed to search")
        .json()
        .await
        .expect("Invalid JSON");

    let score1 = search1["results"][0]["combined_score"]
        .as_f64()
        .expect("No combined_score");

    // Change strength_weight = 1.0 (pure strength)
    let _config_response = client
        .put(
            &format!("/projects/{}/config/algorithm", project_id),
            json!({
                "strength_weight": 1.0
            }),
        )
        .await
        .expect("Failed to update config");

    // Search again - should have different combined_score
    let search2: Value = client
        .post(
            &format!("/projects/{}/search", project_id),
            json!({
                "query": "test"
            }),
        )
        .await
        .expect("Failed to search")
        .json()
        .await
        .expect("Invalid JSON");

    let score2 = search2["results"][0]["combined_score"]
        .as_f64()
        .expect("No combined_score");

    // Scores should be different (one is pure semantic, one is pure strength)
    assert!((score1 - score2).abs() > 0.01, "Decay config should affect search results");

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_reject_invalid_strength_weight() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let project_response = client
        .post(
            "/projects",
            json!({
                "name": "invalid-test",
                "slug": project_slug
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = project_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Try to set invalid strength_weight
    let invalid_response = client
        .put(
            &format!("/projects/{}/config/algorithm", project_id),
            json!({
                "strength_weight": 1.5
            }),
        )
        .await
        .expect("Failed to send request");

    assert!(invalid_response.status().is_client_error(), "Should reject invalid strength_weight");

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}

#[tokio::test]
#[ignore] // Only run with server running
async fn test_http_memory_requires_content() {
    let client = TestClient::new();
    let project_slug = format!("ts-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

    // Create project
    let project_response = client
        .post(
            "/projects",
            json!({
                "name": "empty-content-test",
                "slug": project_slug
            }),
        )
        .await
        .expect("Failed to create project");

    let project: Value = project_response.json().await.expect("Invalid response JSON");
    let project_id = project["id"].as_str().expect("No project ID");

    // Try to create memory with empty content
    let invalid_response = client
        .post(
            &format!("/projects/{}/memories", project_id),
            json!({
                "title": "No Content",
                "content": ""
            }),
        )
        .await
        .expect("Failed to send request");

    assert!(
        invalid_response.status().is_client_error(),
        "Should reject empty content"
    );

    // Cleanup
    client
        .delete(&format!("/projects/{}", project_id))
        .await
        .expect("Failed to delete project");
}
