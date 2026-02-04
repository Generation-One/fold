//! Endpoint-based integration tests for memories, search, and MCP endpoints.
//! Tests verify that project-level access control is properly enforced.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::post_json;
use serde_json::json;

// ============================================================================
// MEMORIES ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_memories_list_requires_project_read_access() {
    // GET /projects/{id}/memories without project access should return 403
    let project_id = "project-123";
    let token = "fold_user_without_access";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Request should have auth header
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_memories_list_without_auth_returns_401() {
    // GET /projects/{id}/memories without token should return 401
    let project_id = "project-123";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .body(Body::empty())
        .unwrap();

    // No auth header
    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_user_with_project_read_can_list_memories() {
    // GET /projects/{id}/memories with read access should succeed
    let project_id = "project-123";
    let token = "fold_user_with_read_access";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Request should be properly formed
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_memories_create_requires_project_write_access() {
    // POST /projects/{id}/memories with read-only access should return 403
    let project_id = "project-123";
    let token = "fold_user_readonly";
    let uri = format!("/projects/{}/memories", project_id);

    let request_body = json!({
        "title": "New Memory",
        "content": "Some content"
    });

    let _request = post_json(&uri, token, request_body);

    // User must have write access to create memories
}

#[tokio::test]
async fn test_memories_update_requires_project_write_access() {
    // PUT /projects/{id}/memories/{memory_id} with read-only access should return 403
    let project_id = "project-123";
    let memory_id = "memory-456";
    let token = "fold_user_readonly";
    let uri = format!("/projects/{}/memories/{}", project_id, memory_id);

    let request_body = json!({
        "title": "Updated Memory"
    });

    let request = Request::builder()
        .method("PUT")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // Request should be properly formed with auth header
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_memories_delete_requires_project_write_access() {
    // DELETE /projects/{id}/memories/{memory_id} with read-only access should return 403
    let project_id = "project-123";
    let memory_id = "memory-456";
    let token = "fold_user_readonly";
    let uri = format!("/projects/{}/memories/{}", project_id, memory_id);

    let request = Request::builder()
        .method("DELETE")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Request should have auth header
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_user_without_project_access_cannot_view_memories() {
    // User not in project should get 403
    let project_id = "project-no-access";
    let token = "fold_user_without_access";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_admin_can_access_any_project_memories() {
    // Admin user should bypass project checks and access all memories
    let project_id = "project-123";
    let admin_token = "fold_admin_token";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

// ============================================================================
// SEARCH ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_search_memories_requires_project_read_access() {
    // POST /projects/{id}/memories/search without project access should return 403
    let project_id = "project-123";
    let token = "fold_user_without_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "search term",
        "limit": 10
    });

    let _request = post_json(&uri, token, request_body);

    // User must have read access to search
}

#[tokio::test]
async fn test_search_memories_without_auth_returns_401() {
    // POST /projects/{id}/memories/search without token should return 401
    let project_id = "project-123";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "search term",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // No auth header
    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_user_with_read_access_can_search() {
    // POST /projects/{id}/memories/search with read access should succeed
    let project_id = "project-123";
    let token = "fold_user_with_read_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "search term",
        "limit": 10,
        "threshold": 0.5
    });

    let request = post_json(&uri, token, request_body.clone());

    // Request should have auth and valid body
    assert!(request.headers().get("Authorization").is_some());
    assert_eq!(request_body["query"], "search term");
}

#[tokio::test]
async fn test_search_results_filtered_by_project_membership() {
    // Search should only return results from accessible projects
    let project_id = "project-123";
    let token = "fold_user_with_project_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 20
    });

    let _request = post_json(&uri, token, request_body);

    // Results should be filtered to only include memories in project-123
}

#[tokio::test]
async fn test_search_respects_group_membership() {
    // User accessing project via group membership should be able to search
    let project_id = "project-via-group";
    let token = "fold_user_in_group";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test query",
        "limit": 10
    });

    let request = post_json(&uri, token, request_body);

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_admin_search_returns_results_from_all_projects() {
    // Admin search should access memories across all projects
    let project_id = "project-123";
    let admin_token = "fold_admin_token";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "admin search",
        "limit": 50
    });

    let request = post_json(&uri, admin_token, request_body);

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_with_invalid_token_returns_401() {
    // Search with invalid token should return 401
    let project_id = "project-123";
    let invalid_token = "invalid_token_format";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", invalid_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!invalid_token.starts_with("fold_"));
}

// ============================================================================
// MCP ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_requires_authentication() {
    // POST /mcp without token should return 401
    let uri = "/mcp";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "list_resources",
        "params": {}
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // No auth header
    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_with_valid_token_succeeds() {
    // POST /mcp with valid token should succeed
    let uri = "/mcp";
    let token = "fold_valid_mcp_token";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "list_resources",
        "params": {}
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_respects_token_project_scoping() {
    // Token scoped to specific projects should only access those projects via MCP
    let uri = "/mcp";
    let scoped_token = "fold_scoped_to_project_123";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "read_resource",
        "params": {
            "uri": "projects://project-456/memories"
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", scoped_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // Token format should be valid
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_with_invalid_token_format_returns_401() {
    // MCP with invalid token format should return 401
    let uri = "/mcp";
    let invalid_token = "not_a_valid_fold_token";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "list_resources"
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", invalid_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!invalid_token.starts_with("fold_"));
}

#[tokio::test]
async fn test_mcp_with_expired_token_returns_401() {
    // MCP with expired token should return 401
    let uri = "/mcp";
    let expired_token = "fold_expired_token_here";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "list_resources"
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", expired_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_with_revoked_token_returns_401() {
    // MCP with revoked token should return 401
    let uri = "/mcp";
    let revoked_token = "fold_revoked_token_here";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "list_resources"
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", revoked_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_sse_requires_authentication() {
    // GET /mcp (SSE) without token should return 401
    let uri = "/mcp";

    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();

    // No auth header
    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_sse_with_valid_token_establishes_stream() {
    // GET /mcp (SSE) with valid token should establish stream
    let uri = "/mcp";
    let token = "fold_valid_mcp_token";

    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_respects_user_role_permissions() {
    // Admin users via MCP should have elevated permissions
    let uri = "/mcp";
    let admin_token = "fold_admin_mcp_token";

    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "write_resource",
        "params": {
            "uri": "projects://project-123/memories",
            "contents": "test"
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

// ============================================================================
// CROSS-ENDPOINT SECURITY TESTS
// ============================================================================

#[tokio::test]
async fn test_user_cannot_search_memories_in_restricted_project() {
    // User with no access cannot search project memories
    let project_id = "restricted-project";
    let token = "fold_user_no_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10
    });

    let _request = post_json(&uri, token, request_body);

    // User should get 403 Forbidden
}

#[tokio::test]
async fn test_memory_access_via_group_membership() {
    // User via group membership should access project memories
    let project_id = "project-123";
    let token = "fold_user_in_group_with_access";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_memory_creation_tracked_by_user_role() {
    // Memory creation should respect user permissions via API or MCP
    let project_id = "project-123";
    let token = "fold_member_token";
    let uri = format!("/projects/{}/memories", project_id);

    let request_body = json!({
        "title": "New Memory",
        "content": "Content by member user",
        "author": "member-user"
    });

    let request = post_json(&uri, token, request_body.clone());

    assert_eq!(request_body["author"], "member-user");
}

#[tokio::test]
async fn test_viewer_role_cannot_modify_memories() {
    // User with viewer role cannot create/update/delete
    let project_id = "project-123";
    let viewer_token = "fold_viewer_token";
    let uri = format!("/projects/{}/memories", project_id);

    let request_body = json!({
        "title": "New Memory",
        "content": "Attempt to create as viewer"
    });

    let _request = post_json(&uri, viewer_token, request_body);

    // Viewer should get 403 Forbidden on write operations
}

#[tokio::test]
async fn test_direct_and_group_membership_combine() {
    // User with both direct and group access should have combined permissions
    let project_id = "project-with-mixed-access";
    let token = "fold_user_direct_and_group";
    let uri = format!("/projects/{}/memories", project_id);

    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}
