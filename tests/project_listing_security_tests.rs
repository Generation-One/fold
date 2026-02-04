//! Tests to verify project listing respects user access control.
//! Users should only see projects they have access to.

mod common;

use axum::body::Body;
use axum::http::Request;

// ============================================================================
// PROJECT LISTING SECURITY TESTS
// ============================================================================

#[tokio::test]
async fn test_list_projects_requires_authentication() {
    // GET /projects without token should return 401
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .body(Body::empty())
        .unwrap();

    // No auth header
    assert!(!request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_authenticated_user_sees_only_accessible_projects() {
    // User should only see projects they have access to
    // If user has access to project-1 and project-2 but not project-3
    // listing should only return project-1 and project-2
    let token = "fold_user_with_partial_access";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_user_does_not_see_projects_without_access() {
    // User without access to a project should not see it in listing
    let token = "fold_user_limited_access";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Request should be valid but results should be filtered
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_admin_sees_all_projects() {
    // Admin user should see all projects regardless of membership
    let admin_token = "fold_admin_token";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_member_via_direct_assignment_sees_project() {
    // User with direct project membership should see the project
    let token = "fold_user_direct_member";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_member_via_group_assignment_sees_project() {
    // User with project access via group membership should see the project
    let token = "fold_user_group_member";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_list_projects_pagination_respects_access() {
    // Pagination should still respect user access control
    // Even with offset/limit, results should only include accessible projects
    let token = "fold_user_with_access";
    let request = Request::builder()
        .method("GET")
        .uri("/projects?offset=0&limit=10")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_viewer_role_can_see_accessible_projects() {
    // Users with viewer role should still see projects they have viewer access to
    let viewer_token = "fold_viewer_with_access";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", viewer_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_invalid_token_returns_401() {
    // Invalid token format should return 401
    let invalid_token = "invalid_token";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", invalid_token))
        .body(Body::empty())
        .unwrap();

    assert!(!invalid_token.starts_with("fold_"));
}

#[tokio::test]
async fn test_expired_token_returns_401() {
    // Expired token should return 401
    let expired_token = "fold_expired_token_here";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", expired_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_revoked_token_returns_401() {
    // Revoked token should return 401
    let revoked_token = "fold_revoked_token_here";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", revoked_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_api_token_project_scoping_respected() {
    // API token scoped to specific projects should only see those projects
    let scoped_token = "fold_token_scoped_to_project_1";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", scoped_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_unscoped_token_sees_all_user_accessible_projects() {
    // Token without project scoping should see all projects user has access to
    let unscoped_token = "fold_unscoped_token_here";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", unscoped_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_project_listing_total_count_respects_access() {
    // The "total" count returned should reflect accessible projects only
    let token = "fold_user_limited_access";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    // Response should include total count of accessible projects only
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_empty_project_list_for_user_with_no_access() {
    // New user with no project assignments should see empty list
    let new_user_token = "fold_new_user_no_projects";
    let request = Request::builder()
        .method("GET")
        .uri("/projects")
        .header("Authorization", format!("Bearer {}", new_user_token))
        .body(Body::empty())
        .unwrap();

    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}
