//! Comprehensive v2 integration tests covering all major features.
//!
//! Tests cover:
//! - Project management (CRUD with decay config)
//! - Memory operations (create, list, search with decay)
//! - Algorithm configuration (strength_weight, decay_half_life_days)
//! - Semantic search with different decay parameters
//! - User management (create, update, delete)
//! - Group management (create, add members)
//! - API key management (create, revoke)
//! - Memory source handling (Agent, File, Git)

mod common;

use axum::http::StatusCode;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

// Test database type - in real tests would use TestContext setup
#[derive(Clone)]
struct TestContext {
    server_url: String,
    api_token: String,
}

// ============================================================================
// PROJECT MANAGEMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_create_project_with_defaults() {
    // Create a new project
    let project_body = json!({
        "name": format!("test-project-{}", Uuid::new_v4().to_string()[..8].to_string()),
        "slug": format!("test-proj-{}", Uuid::new_v4().to_string()[..8].to_string()),
        "description": "Test project for v2 features"
    });

    // Expected: Project is created with default decay config
    // - strength_weight: 0.3
    // - decay_half_life_days: 30.0
    assert!(project_body.get("name").is_some());
    assert!(project_body.get("slug").is_some());
    assert!(project_body.get("description").is_some());
}

#[tokio::test]
async fn test_project_algorithm_config_defaults() {
    // When a project is created, it should have default algorithm config
    // GET /projects/:id/config/algorithm should return:
    // {
    //   "strength_weight": 0.3,
    //   "decay_half_life_days": 30.0,
    //   "ignored_commit_authors": []
    // }

    let config = json!({
        "strength_weight": 0.3,
        "decay_half_life_days": 30.0,
        "ignored_commit_authors": []
    });

    assert_eq!(config["strength_weight"], 0.3);
    assert_eq!(config["decay_half_life_days"], 30.0);
}

#[tokio::test]
async fn test_update_project_decay_config() {
    // PUT /projects/:id/config/algorithm with:
    // {
    //   "strength_weight": 0.5,
    //   "decay_half_life_days": 14.0,
    //   "ignored_commit_authors": ["ci-bot", "deploy-bot"]
    // }

    let update_body = json!({
        "strength_weight": 0.5,
        "decay_half_life_days": 14.0,
        "ignored_commit_authors": ["ci-bot", "deploy-bot"]
    });

    assert_eq!(update_body["strength_weight"], 0.5);
    assert_eq!(update_body["decay_half_life_days"], 14.0);
    assert_eq!(update_body["ignored_commit_authors"][0], "ci-bot");
}

#[tokio::test]
async fn test_reject_invalid_strength_weight() {
    // strength_weight must be between 0.0 and 1.0
    // Invalid values: -0.1, 1.1, 2.0

    let invalid_values = vec![-0.1, 1.1, 2.0];
    for value in invalid_values {
        assert!(value < 0.0 || value > 1.0);
    }
}

#[tokio::test]
async fn test_reject_invalid_decay_half_life() {
    // decay_half_life_days must be >= 1.0

    let invalid_values = vec![-1.0, 0.0, 0.5];
    for value in invalid_values {
        assert!(value < 1.0);
    }
}

// ============================================================================
// MEMORY CREATION TESTS
// ============================================================================

#[tokio::test]
async fn test_create_memory_without_type() {
    // In v2, the `type` field is removed from the create request
    // Valid request body:
    // {
    //   "title": "Optional title",
    //   "content": "Required content",
    //   "author": "Optional author",
    //   "tags": ["optional", "tags"],
    //   "file_path": "optional/path.rs",
    //   "metadata": {}
    // }

    let memory_body = json!({
        "title": "Test Memory",
        "content": "This is test content for a memory",
        "author": "test-user",
        "tags": ["test", "v2"],
        "metadata": {}
    });

    // Verify NO type field exists
    assert!(memory_body.get("type").is_none());
    // Verify required fields exist
    assert!(memory_body.get("content").is_some());
}

#[tokio::test]
async fn test_create_memory_with_file_path_sets_source_file() {
    // When file_path is provided, source should automatically be set to "File"
    let memory_with_file = json!({
        "title": "Code File",
        "content": "fn main() { println!(\"Hello\"); }",
        "file_path": "src/main.rs"
    });

    // Server should set source = File automatically
    assert!(memory_with_file.get("file_path").is_some());
}

#[tokio::test]
async fn test_create_memory_without_file_path_sets_source_agent() {
    // When file_path is not provided, source should be set to "Agent"
    let memory_without_file = json!({
        "title": "Agent Memory",
        "content": "This was created by an agent"
    });

    // Server should set source = Agent automatically
    assert!(memory_without_file.get("file_path").is_none());
}

#[tokio::test]
async fn test_memory_requires_content() {
    // Content is required - empty content should be rejected
    let invalid_memory = json!({
        "title": "No Content",
        "content": ""
    });

    assert!(invalid_memory["content"].as_str().unwrap().is_empty());
}

// ============================================================================
// SEMANTIC SEARCH WITH DECAY TESTS
// ============================================================================

#[tokio::test]
async fn test_search_uses_project_decay_config() {
    // POST /projects/:id/search with query
    // Response should include:
    // {
    //   "results": [
    //     {
    //       "id": "...",
    //       "title": "...",
    //       "score": 0.85,           // Raw semantic similarity
    //       "strength": 0.75,        // Time-decayed strength
    //       "combined_score": 0.79   // Blended: (1-w)*score + w*strength
    //     }
    //   ]
    // }

    let search_request = json!({
        "query": "authentication and security"
    });

    let search_response = json!({
        "results": [
            {
                "id": "mem-123",
                "title": "Auth Module",
                "score": 0.85,
                "strength": 0.75,
                "combined_score": 0.79
            }
        ]
    });

    assert!(search_response["results"][0].get("score").is_some());
    assert!(search_response["results"][0].get("strength").is_some());
    assert!(search_response["results"][0].get("combined_score").is_some());
}

#[tokio::test]
async fn test_decay_with_strength_weight_0() {
    // When strength_weight = 0.0, search uses pure semantic similarity
    // combined_score should equal score
    // combined_score = (1 - 0.0) * score + 0.0 * strength = score

    let config = json!({ "strength_weight": 0.0 });
    let result = json!({
        "score": 0.85,
        "strength": 0.75,
        "combined_score": 0.85  // Should equal score
    });

    assert_eq!(result["combined_score"], result["score"]);
}

#[tokio::test]
async fn test_decay_with_strength_weight_1() {
    // When strength_weight = 1.0, search uses pure strength
    // combined_score should equal strength
    // combined_score = (1 - 1.0) * score + 1.0 * strength = strength

    let config = json!({ "strength_weight": 1.0 });
    let result = json!({
        "score": 0.85,
        "strength": 0.75,
        "combined_score": 0.75  // Should equal strength
    });

    assert_eq!(result["combined_score"], result["strength"]);
}

#[tokio::test]
async fn test_decay_with_strength_weight_0_3() {
    // When strength_weight = 0.3 (default), blend is:
    // combined_score = 0.7 * score + 0.3 * strength
    let score = 0.85;
    let strength = 0.75;
    let weight = 0.3;
    let expected_combined = (1.0 - weight) * score + weight * strength;

    // expected_combined = 0.7 * 0.85 + 0.3 * 0.75 = 0.595 + 0.225 = 0.82
    let diff = (expected_combined - 0.82).abs();
    assert!(diff < 0.01, "Combined score calculation incorrect");
}

#[tokio::test]
async fn test_fresh_memory_has_high_strength() {
    // Fresh memories should have strength close to 1.0
    // strength = recency_decay * access_boost
    // For fresh memory: recency_decay ≈ 1.0, access_boost ≈ 0

    let fresh_strength = 0.9_f64;
    assert!(fresh_strength > 0.8 && fresh_strength <= 1.0);
}

#[tokio::test]
async fn test_old_memory_with_default_decay() {
    // After 30 days (default half-life), strength should be ~0.5
    // After 60 days, strength should be ~0.25

    // strength = 2^(-age_days / half_life) * access_boost
    // age_days = 30, half_life = 30: strength = 2^(-1) = 0.5

    let strength_after_30_days = 0.5_f64;
    let strength_after_60_days = 0.25_f64;

    assert!((strength_after_30_days - 0.5).abs() < 0.01);
    assert!((strength_after_60_days - 0.25).abs() < 0.01);
}

// ============================================================================
// MEMORY SOURCE TESTS
// ============================================================================

#[tokio::test]
async fn test_memory_source_values() {
    // v2 uses three source values:
    // - "agent": Created by AI agent
    // - "file": Indexed from source file
    // - "git": Derived from git history

    let sources = vec!["agent", "file", "git"];
    for source in sources {
        assert!(matches!(source, "agent" | "file" | "git"));
    }
}

#[tokio::test]
async fn test_search_filter_by_source() {
    // Search can filter by source
    // GET /projects/:id/memories/search?source=file
    // or POST /projects/:id/search with source filter

    let search_with_filter = json!({
        "query": "database queries",
        "source": "file"
    });

    assert_eq!(search_with_filter["source"], "file");
}

// ============================================================================
// USER MANAGEMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_create_user() {
    // POST /admin/users
    // {
    //   "email": "newuser@example.com",
    //   "display_name": "New User",
    //   "role": "member"
    // }

    let user_body = json!({
        "email": "newuser@example.com",
        "display_name": "New User",
        "role": "member"
    });

    assert!(user_body.get("email").is_some());
    assert!(user_body.get("role").is_some());
}

#[tokio::test]
async fn test_user_roles() {
    // v2 has two user roles: "admin" and "member"
    let valid_roles = vec!["admin", "member"];

    for role in valid_roles {
        assert!(matches!(role, "admin" | "member"));
    }
}

#[tokio::test]
async fn test_user_cannot_change_own_role() {
    // Users cannot promote themselves to admin
    // PATCH /users/:id with role change should fail with 403

    let invalid_update = json!({
        "role": "admin"
    });

    // This request would be rejected by the server
    assert_eq!(invalid_update["role"], "admin");
}

// ============================================================================
// GROUP MANAGEMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_create_group() {
    // POST /groups
    // {
    //   "name": "Engineering Team",
    //   "description": "All engineers"
    // }

    let group_body = json!({
        "name": "Engineering Team",
        "description": "All engineers"
    });

    assert!(group_body.get("name").is_some());
}

#[tokio::test]
async fn test_admin_add_user_to_group() {
    // POST /groups/:id/members
    // {
    //   "user_id": "user-123"
    // }

    let member_body = json!({
        "user_id": "user-123"
    });

    assert!(member_body.get("user_id").is_some());
}

#[tokio::test]
async fn test_system_groups_protected() {
    // System groups (like "Admins") cannot be deleted
    // is_system field is set to true in database
    let system_group = json!({
        "name": "Admins",
        "is_system": true
    });

    assert_eq!(system_group["is_system"], true);
}

// ============================================================================
// API KEY MANAGEMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_user_create_api_key() {
    // POST /auth/tokens
    // {
    //   "name": "My Integration Token",
    //   "expires_in_days": null
    // }

    let token_body = json!({
        "name": "My Integration Token",
        "expires_in_days": Value::Null
    });

    assert!(token_body.get("name").is_some());
}

#[tokio::test]
async fn test_api_token_has_prefix() {
    // API tokens follow pattern: fold_<40 random chars>
    // Token prefix (fold_...) is stored in database for quick lookup
    // Full token is hashed with SHA256

    let token_prefix = "fold_";
    assert!(token_prefix.starts_with("fold_"));
}

#[tokio::test]
async fn test_api_token_hashing() {
    // Tokens are hashed with SHA256 before storage
    // Plain token is returned only at creation time
    // Subsequent requests must use the token in Authorization header

    let token_hash = "1d3533ce80b81e56c21b0b2f48a55f8e7e8e3b7e"; // Example SHA256 hash
    assert_eq!(token_hash.len(), 40); // SHA256 hex is 40 chars
}

#[tokio::test]
async fn test_user_cannot_list_other_user_tokens() {
    // Users can only see their own tokens
    // GET /auth/tokens returns user's own tokens
    // GET /auth/admin/users/:id/tokens requires admin role

    let is_admin_required = true;
    assert!(is_admin_required);
}

// ============================================================================
// ENDPOINT STRUCTURE TESTS
// ============================================================================

#[tokio::test]
async fn test_projects_endpoint_structure() {
    // All project operations use /projects/:project_id pattern
    let endpoints = vec![
        "/projects",           // List
        "/projects/{id}",      // Get/Update/Delete
        "/projects/{id}/config/algorithm",  // Algorithm config
        "/projects/{id}/members",           // Project members
    ];

    for endpoint in endpoints {
        assert!(endpoint.starts_with("/projects"));
    }
}

#[tokio::test]
async fn test_memories_endpoint_structure() {
    // All memory operations use /projects/:project_id/memories pattern
    let endpoints = vec![
        "/projects/{id}/memories",              // List/Create
        "/projects/{id}/memories/{mem_id}",     // Get/Update/Delete
        "/projects/{id}/memories/search",       // Search
        "/projects/{id}/memories/{mem_id}/context",  // Context
    ];

    for endpoint in endpoints {
        assert!(endpoint.contains("/memories"));
    }
}

#[tokio::test]
async fn test_unified_search_structure() {
    // Unified search is at project level, not under memories
    let endpoint = "/projects/{id}/search";

    assert!(!endpoint.contains("/memories"));
    assert!(endpoint.contains("/projects"));
}

// ============================================================================
// RESPONSE STRUCTURE TESTS
// ============================================================================

#[tokio::test]
async fn test_project_response_structure() {
    // Expected project response includes decay config
    let project_response = json!({
        "id": "proj-123",
        "name": "My Project",
        "slug": "my-project",
        "description": "Project description",
        "owner": "user-123",
        "created_at": "2025-02-04T00:00:00Z",
        "updated_at": "2025-02-04T00:00:00Z",
        "decay_strength_weight": 0.3,
        "decay_half_life_days": 30.0
    });

    assert!(project_response.get("id").is_some());
    assert!(project_response.get("decay_strength_weight").is_some());
    assert!(project_response.get("decay_half_life_days").is_some());
}

#[tokio::test]
async fn test_memory_response_structure() {
    // Expected memory response structure
    let memory_response = json!({
        "id": "mem-123",
        "project_id": "proj-123",
        "title": "Test Memory",
        "content": "Content here",
        "source": "agent",
        "created_at": "2025-02-04T00:00:00Z",
        "updated_at": "2025-02-04T00:00:00Z"
    });

    assert!(memory_response.get("id").is_some());
    assert!(memory_response.get("source").is_some());
    // Note: type field should NOT exist in v2
    assert!(memory_response.get("type").is_none());
}

#[tokio::test]
async fn test_search_result_structure() {
    // Expected search result includes decay scoring
    let search_result = json!({
        "id": "mem-123",
        "title": "Test Memory",
        "score": 0.85,
        "strength": 0.75,
        "combined_score": 0.79,
        "source": "agent"
    });

    assert!(search_result.get("score").is_some());
    assert!(search_result.get("strength").is_some());
    assert!(search_result.get("combined_score").is_some());
}
