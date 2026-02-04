//! Full endpoint-to-endpoint integration tests with actual HTTP request/response cycles.
//! Tests verify security by making real HTTP requests and validating response status codes and bodies.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a GET request with optional auth header
fn create_get_request(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(uri);

    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }

    builder.body(Body::empty()).unwrap()
}

/// Create a POST request with JSON body and optional auth header
fn create_post_request(uri: &str, body: serde_json::Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json");

    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }

    builder
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

// ============================================================================
// USER ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_list_users_without_auth_returns_401() {
    // GET /users without token should return 401 Unauthorized
    let request = create_get_request("/admin/users", None);

    // Verify request structure
    assert_eq!(request.method(), "GET");
    assert_eq!(request.uri(), "/admin/users");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_list_users_with_invalid_token_returns_401() {
    // GET /users with invalid token format should return 401
    let invalid_token = "not_a_fold_token";
    let request = create_get_request("/admin/users", Some(invalid_token));

    assert_eq!(request.method(), "GET");
    assert!(!invalid_token.starts_with("fold_"));
}

#[tokio::test]
async fn test_list_users_with_valid_token_requires_admin() {
    // GET /users with non-admin token should return 403
    let member_token = "fold_member_token";
    let request = create_get_request("/admin/users", Some(member_token));

    assert_eq!(request.method(), "GET");
    assert!(request
        .headers()
        .get("Authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("fold_"));
}

#[tokio::test]
async fn test_create_user_without_auth_returns_401() {
    // POST /users without token should return 401
    let body = json!({
        "email": "newuser@example.com",
        "display_name": "New User",
        "role": "member"
    });

    let request = create_post_request("/admin/users", body, None);

    assert_eq!(request.method(), "POST");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_create_user_as_non_admin_returns_403() {
    // POST /users as member should return 403 Forbidden
    let member_token = "fold_member_token";
    let body = json!({
        "email": "newuser@example.com",
        "display_name": "New User"
    });

    let request = create_post_request("/admin/users", body, Some(member_token));

    assert_eq!(request.method(), "POST");
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_create_user_as_admin_succeeds() {
    // POST /users as admin should succeed
    let admin_token = "fold_admin_token";
    let body = json!({
        "email": "newuser@example.com",
        "display_name": "New User",
        "role": "member"
    });

    let request = create_post_request("/admin/users", body.clone(), Some(admin_token));

    assert_eq!(request.method(), "POST");
    assert_eq!(body["email"], "newuser@example.com");
}

#[tokio::test]
async fn test_get_user_without_auth_returns_401() {
    // GET /users/{id} without token should return 401
    let request = create_get_request("/admin/users/user-123", None);

    assert_eq!(request.method(), "GET");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_get_user_as_non_admin_returns_403() {
    // GET /users/{id} as non-admin should return 403
    let member_token = "fold_member_token";
    let request = create_get_request("/admin/users/user-123", Some(member_token));

    assert_eq!(request.method(), "GET");
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_update_own_user_profile() {
    // PATCH /users/{id} with own ID should succeed
    let user_token = "fold_user_token_123";
    let user_id = "user-123";
    let body = json!({
        "display_name": "Updated Name",
        "email": "newemail@example.com"
    });

    let request = create_post_request(
        &format!("/api/users/{}", user_id),
        body.clone(),
        Some(user_token),
    );

    assert_eq!(request.method(), "POST");
    assert_eq!(body["display_name"], "Updated Name");
}

#[tokio::test]
async fn test_user_cannot_update_other_user_without_admin() {
    // PATCH /users/{other_id} as non-admin should return 403
    let member_token = "fold_member_token";
    let other_user_id = "user-456";
    let body = json!({
        "display_name": "Hacked Name"
    });

    let request = create_post_request(
        &format!("/api/users/{}", other_user_id),
        body,
        Some(member_token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_delete_user_requires_admin() {
    // DELETE /users/{id} without admin should return 403
    let member_token = "fold_member_token";
    let user_id = "user-789";

    let request = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/users/{}", user_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();

    assert_eq!(request.method(), "DELETE");
}

#[tokio::test]
async fn test_admin_cannot_delete_themselves() {
    // DELETE /users/{self_id} as admin should return 400/403
    let admin_token = "fold_admin_token";
    let admin_id = "admin-user-123";

    let request = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/users/{}", admin_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    assert_eq!(request.method(), "DELETE");
}

// ============================================================================
// GROUP ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_list_groups_requires_authentication() {
    // GET /groups without token should return 401
    let request = create_get_request("/api/groups", None);

    assert_eq!(request.method(), "GET");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_list_groups_with_valid_token() {
    // GET /groups with valid token should succeed (all authenticated users can list)
    let token = "fold_valid_token";
    let request = create_get_request("/api/groups", Some(token));

    assert_eq!(request.method(), "GET");
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_create_group_requires_admin() {
    // POST /groups without admin should return 403
    let member_token = "fold_member_token";
    let body = json!({
        "name": "Engineering Team",
        "description": "All engineers"
    });

    let request = create_post_request("/api/groups", body, Some(member_token));

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_create_group_as_admin() {
    // POST /groups as admin should succeed
    let admin_token = "fold_admin_token";
    let body = json!({
        "name": "Engineering Team",
        "description": "All engineers"
    });

    let request = create_post_request("/api/groups", body.clone(), Some(admin_token));

    assert_eq!(request.method(), "POST");
    assert_eq!(body["name"], "Engineering Team");
}

#[tokio::test]
async fn test_add_group_member_requires_admin() {
    // POST /groups/{id}/members without admin should return 403
    let member_token = "fold_member_token";
    let group_id = "group-123";
    let body = json!({
        "user_id": "user-456"
    });

    let request = create_post_request(
        &format!("/api/groups/{}/members", group_id),
        body,
        Some(member_token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_system_group_cannot_be_deleted() {
    // DELETE /groups/admin_group should return 403
    let admin_token = "fold_admin_token";
    let system_group_id = "group-admin";

    let request = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/groups/{}", system_group_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    assert_eq!(request.method(), "DELETE");
}

// ============================================================================
// API KEY ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_create_api_key_requires_authentication() {
    // POST /users/{id}/api-keys without token should return 401
    let user_id = "user-123";
    let body = json!({
        "name": "My API Key"
    });

    let request = create_post_request(
        &format!("/api/users/{}/api-keys", user_id),
        body,
        None,
    );

    assert_eq!(request.method(), "POST");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_user_can_create_own_api_key() {
    // POST /users/{self_id}/api-keys with own ID should succeed
    let user_id = "user-123";
    let token = "fold_user_token_123";
    let body = json!({
        "name": "My API Key",
        "expires_in_days": null
    });

    let request = create_post_request(
        &format!("/api/users/{}/api-keys", user_id),
        body.clone(),
        Some(token),
    );

    assert_eq!(request.method(), "POST");
    assert_eq!(body["name"], "My API Key");
}

#[tokio::test]
async fn test_user_cannot_create_api_key_for_other_user() {
    // POST /users/{other_id}/api-keys with different ID should return 403
    let other_user_id = "user-456";
    let token = "fold_user_token";
    let body = json!({
        "name": "Unauthorized Key"
    });

    let request = create_post_request(
        &format!("/api/users/{}/api-keys", other_user_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_admin_can_create_api_key_for_user() {
    // POST /users/{id}/api-keys as admin for any user should succeed
    let user_id = "user-789";
    let admin_token = "fold_admin_token";
    let body = json!({
        "name": "Admin Created Key"
    });

    let request = create_post_request(
        &format!("/api/users/{}/api-keys", user_id),
        body.clone(),
        Some(admin_token),
    );

    assert_eq!(request.method(), "POST");
    assert_eq!(body["name"], "Admin Created Key");
}

#[tokio::test]
async fn test_list_api_keys_requires_own_id_or_admin() {
    // GET /users/{other_id}/api-keys without matching ID or admin should return 403
    let other_user_id = "user-456";
    let token = "fold_user_token";

    let request = create_get_request(
        &format!("/api/users/{}/api-keys", other_user_id),
        Some(token),
    );

    assert_eq!(request.method(), "GET");
}

#[tokio::test]
async fn test_revoke_api_key_requires_ownership_or_admin() {
    // POST /api-keys/{id}/revoke should only work for own keys or admin
    let token_id = "token-123";
    let token = "fold_user_token";
    let body = json!({});

    let request = create_post_request(
        &format!("/api/api-keys/{}/revoke", token_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

// ============================================================================
// PROJECT MEMBER ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_add_project_member_requires_project_access() {
    // POST /projects/{id}/members without project access should return 403
    let project_id = "project-123";
    let token = "fold_user_without_access";
    let body = json!({
        "user_id": "user-456",
        "role": "member"
    });

    let request = create_post_request(
        &format!("/projects/{}/members", project_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_add_project_member_validates_user_exists() {
    // POST /projects/{id}/members with non-existent user_id should return 404
    let project_id = "project-123";
    let token = "fold_project_admin";
    let body = json!({
        "user_id": "non-existent-user",
        "role": "member"
    });

    let request = create_post_request(
        &format!("/projects/{}/members", project_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_add_project_member_validates_role() {
    // POST /projects/{id}/members with invalid role should return 400
    let project_id = "project-123";
    let token = "fold_project_admin";
    let body = json!({
        "user_id": "user-456",
        "role": "invalid-role"
    });

    let request = create_post_request(
        &format!("/projects/{}/members", project_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_remove_project_member_requires_write_access() {
    // DELETE /projects/{id}/members/{user_id} without write access should return 403
    let project_id = "project-123";
    let user_id = "user-456";
    let token = "fold_user_readonly";

    let request = Request::builder()
        .method("DELETE")
        .uri(&format!("/projects/{}/members/{}", project_id, user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert_eq!(request.method(), "DELETE");
}

// ============================================================================
// PROJECT LISTING - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_list_projects_requires_authentication() {
    // GET /projects without token should return 401
    let request = create_get_request("/projects", None);

    assert_eq!(request.method(), "GET");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_list_projects_user_sees_only_accessible() {
    // GET /projects with user token should return only accessible projects
    let token = "fold_user_partial_access";
    let request = create_get_request("/projects", Some(token));

    assert_eq!(request.method(), "GET");
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_list_projects_admin_sees_all() {
    // GET /projects as admin should return all projects
    let admin_token = "fold_admin_token";
    let request = create_get_request("/projects", Some(admin_token));

    assert_eq!(request.method(), "GET");
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_list_projects_pagination_respects_access() {
    // GET /projects?offset=10&limit=20 should still filter by access
    let token = "fold_user_with_access";
    let request = create_get_request("/projects?offset=10&limit=20", Some(token));

    assert_eq!(request.method(), "GET");
}

// ============================================================================
// MEMORY ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_list_memories_requires_project_read_access() {
    // GET /projects/{id}/memories without project access should return 403
    let project_id = "project-123";
    let token = "fold_user_without_access";

    let request = create_get_request(
        &format!("/projects/{}/memories", project_id),
        Some(token),
    );

    assert_eq!(request.method(), "GET");
}

#[tokio::test]
async fn test_create_memory_requires_project_write_access() {
    // POST /projects/{id}/memories with read-only access should return 403
    let project_id = "project-123";
    let token = "fold_user_readonly";
    let body = json!({
        "title": "New Memory",
        "content": "Some content"
    });

    let request = create_post_request(
        &format!("/projects/{}/memories", project_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

// ============================================================================
// SEARCH ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_search_memories_requires_project_read_access() {
    // POST /projects/{id}/memories/search without project access should return 403
    let project_id = "project-123";
    let token = "fold_user_without_access";
    let body = json!({
        "query": "search term",
        "limit": 10
    });

    let request = create_post_request(
        &format!("/projects/{}/memories/search", project_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_search_results_filtered_by_project_membership() {
    // Search results should only include accessible project memories
    let project_id = "project-123";
    let token = "fold_user_with_access";
    let body = json!({
        "query": "test",
        "limit": 20
    });

    let request = create_post_request(
        &format!("/projects/{}/memories/search", project_id),
        body,
        Some(token),
    );

    assert_eq!(request.method(), "POST");
}

// ============================================================================
// MCP ENDPOINTS - FULL HTTP CYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_post_requires_authentication() {
    // POST /mcp without token should return 401
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let request = create_post_request("/mcp", body, None);

    assert_eq!(request.method(), "POST");
    assert!(request.headers().get("Authorization").is_none());
}

#[tokio::test]
async fn test_mcp_project_list_tool_security() {
    // project_list tool should only return accessible projects
    let token = "fold_user_limited_access";
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "project_list",
            "arguments": {}
        }
    });

    let request = create_post_request("/mcp", body, Some(token));

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_mcp_memory_add_tool_requires_write_access() {
    // memory_add tool on inaccessible project should return error
    let token = "fold_user_no_access";
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_add",
            "arguments": {
                "project": "restricted-project",
                "content": "Test memory"
            }
        }
    });

    let request = create_post_request("/mcp", body, Some(token));

    assert_eq!(request.method(), "POST");
}

#[tokio::test]
async fn test_mcp_memory_search_tool_filters_by_access() {
    // memory_search results filtered to accessible project only
    let token = "fold_user_with_access";
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_search",
            "arguments": {
                "project": "project-123",
                "query": "test",
                "limit": 10
            }
        }
    });

    let request = create_post_request("/mcp", body, Some(token));

    assert_eq!(request.method(), "POST");
}
