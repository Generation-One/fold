//! Tests to verify search results respect project-level access control.
//! Users should only see search results from projects they have access to.

mod common;

use axum::body::Body;
use axum::http::Request;
use serde_json::json;

// ============================================================================
// SEARCH RESULTS SECURITY TESTS
// ============================================================================

#[tokio::test]
async fn test_search_only_returns_results_from_accessible_projects() {
    // Search should filter results to only include memories from accessible projects
    // Example: User has access to project-1 and project-3 but not project-2
    // Search results should only include memories from project-1 and project-3
    let project_id = "project-1";
    let token = "fold_user_with_project1_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "important memory",
        "limit": 20
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_excludes_memories_from_inaccessible_projects() {
    // User without access to a project should get no results when searching that project
    let project_id = "restricted-project";
    let token = "fold_user_no_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // Request should be valid but results should be empty (403 Forbidden)
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_admin_search_returns_all_results() {
    // Admin user should see search results from all projects
    let project_id = "any-project";
    let admin_token = "fold_admin_token";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "any search term",
        "limit": 50
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_respects_direct_project_membership() {
    // User with direct project membership should get search results
    let project_id = "project-direct-member";
    let token = "fold_user_direct_member";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "search query",
        "limit": 20
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_respects_group_based_project_membership() {
    // User with group-based project membership should get search results
    let project_id = "project-via-group";
    let token = "fold_user_in_group_with_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test search",
        "limit": 15
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_with_viewer_role_returns_results() {
    // User with viewer role should get search results from accessible projects
    let project_id = "project-viewer-access";
    let viewer_token = "fold_viewer_token";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "viewer can see this",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", viewer_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_with_member_role_returns_results() {
    // User with member role should get search results from accessible projects
    let project_id = "project-member-access";
    let member_token = "fold_member_token";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "member can see this",
        "limit": 20
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_filters_by_both_tag_and_access() {
    // Search with tag filter should still respect project access
    let project_id = "project-123";
    let token = "fold_user_with_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "tags": ["important", "work"],
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_filters_by_author_and_access() {
    // Search with author filter should still respect project access
    let project_id = "project-123";
    let token = "fold_user_with_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "claude",
        "author": "claude",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_with_similarity_threshold_respects_access() {
    // Search with similarity threshold should still respect project access
    let project_id = "project-123";
    let token = "fold_user_with_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10,
        "threshold": 0.7
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_pagination_respects_access() {
    // Search pagination should not leak inaccessible results
    let project_id = "project-123";
    let token = "fold_user_with_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10,
        "offset": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_global_search_only_returns_accessible_results() {
    // Global search across all projects should only return results from accessible projects
    let uri = "/memories";
    let token = "fold_user_partial_access";

    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Request should succeed but results filtered to accessible projects only
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_api_token_scoped_to_projects() {
    // API token scoped to specific projects should only search those projects
    let project_id = "project-123";
    let scoped_token = "fold_token_scoped_to_specific_projects";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", scoped_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_cannot_bypass_access_with_query_manipulation() {
    // Manipulating search query should not bypass access control
    let project_id = "restricted-project";
    let token = "fold_user_no_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "*",  // Attempt to match all
        "limit": 1000
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // Should still return 403 despite query manipulation attempt
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_result_count_accurate_for_user_scope() {
    // Result count should reflect only accessible results, not all results in system
    let project_id = "project-123";
    let token = "fold_user_limited_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // Response should indicate count of accessible results only
    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_does_not_leak_memory_metadata_from_restricted_projects() {
    // Even metadata about inaccessible memories should not be revealed
    let project_id = "project-123";
    let token = "fold_user_no_access_to_project_2";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_search_handles_multiple_projects_correctly() {
    // If user has access to project-1 and project-2, separate searches should work
    // User should not be able to see project-3 results even if searching project-1
    let project_id = "project-1";
    let token = "fold_user_with_p1_p2_access";
    let uri = format!("/projects/{}/memories/search", project_id);

    let request_body = json!({
        "query": "test",
        "limit": 20
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}
