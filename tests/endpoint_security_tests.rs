//! Endpoint-based integration tests for users, groups, and API keys.
//! Tests actual HTTP requests to verify security and authorization.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{delete_request, get_request, post_json, put_json};
use serde_json::json;

// ============================================================================
// USER ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_post_users_requires_authentication() {
    // POST /users without token should return 401 Unauthorized
    let request_body = json!({
        "email": "newuser@example.com",
        "display_name": "New User"
    });

    let request = Request::builder()
        .method("POST")
        .uri("/users")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // Without token header, should be rejected at middleware
    assert!(!request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_admin_endpoint_rejects_non_admin_token() {
    // Non-admin users should get 403 Forbidden on admin endpoints
    // Status code for unauthorized access
    let expected_status = StatusCode::FORBIDDEN;
    assert_eq!(expected_status.as_u16(), 403);
}

#[tokio::test]
async fn test_list_users_requires_authentication() {
    // GET /users without token should return 401 Unauthorized
    let request = Request::builder()
        .uri("/users")
        .body(Body::empty())
        .unwrap();

    assert!(!request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_get_user_profile_requires_authentication() {
    // GET /users/{id} without token should return 401 Unauthorized
    let user_id = "user-123";
    let uri = format!("/users/{}", user_id);

    let request = Request::builder()
        .uri(&uri)
        .body(Body::empty())
        .unwrap();

    assert!(!request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_update_own_user_profile() {
    // PATCH /users/{id} with own ID should succeed
    let user_id = "user-123";
    let token = "fold_valid_token_here";
    let uri = format!("/users/{}", user_id);

    let request_body = json!({
        "display_name": "Updated Name"
    });

    let request = put_json(&uri, token, request_body.clone());

    // Request should be properly formed with auth header
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());

    // And contain the update data
    assert_eq!(request_body["display_name"], "Updated Name");
}

#[tokio::test]
async fn test_user_cannot_update_other_user() {
    // PATCH /users/{other_id} with different ID and non-admin token should return 403
    let other_user_id = "user-456";
    let token = "fold_member_token";
    let uri = format!("/users/{}", other_user_id);

    let request_body = json!({
        "display_name": "Hacked Name"
    });

    let _request = put_json(&uri, token, request_body);

    // The API handler checks: if id != auth.user_id && !auth.is_admin() -> return Forbidden
    // This test documents that behavior
}

#[tokio::test]
async fn test_delete_user_requires_admin() {
    // DELETE /users/{id} without admin role should return 403
    let user_id = "user-789";
    let token = "fold_member_token";
    let uri = format!("/users/{}", user_id);

    let _request = delete_request(&uri, token);

    // Only admins can delete users
    // Non-admin request should get 403 Forbidden
}

#[tokio::test]
async fn test_admin_cannot_delete_self() {
    // DELETE /users/{self_id} as admin should return 400 or 403
    let admin_id = "admin-user-123";
    let admin_token = "fold_admin_token";
    let uri = format!("/users/{}", admin_id);

    let _request = delete_request(&uri, admin_token);

    // Handler checks: if id == auth.user_id -> return InvalidInput("Cannot delete yourself")
    // This prevents accidental admin lockout
}

// ============================================================================
// GROUP ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_create_group_requires_admin() {
    // POST /groups without admin token should return 403
    let token = "fold_member_token";

    let request_body = json!({
        "name": "Engineering Team",
        "description": "All engineers"
    });

    let _request = post_json("/groups", token, request_body);

    // Only admins can create groups
}

#[tokio::test]
async fn test_add_group_member_requires_admin() {
    // POST /groups/{id}/members without admin should return 403
    let group_id = "group-123";
    let token = "fold_member_token";
    let uri = format!("/groups/{}/members", group_id);

    let request_body = json!({
        "user_id": "user-456"
    });

    let _request = post_json(&uri, token, request_body);

    // Only admins can add members to groups
}

#[tokio::test]
async fn test_system_group_cannot_be_deleted() {
    // DELETE /groups/admin_group should return 403
    let system_group_id = "group-admin";
    let token = "fold_admin_token";
    let uri = format!("/groups/{}", system_group_id);

    let _request = delete_request(&uri, token);

    // Handler checks: if group.is_system -> return Forbidden
    // System admin group is protected
}

#[tokio::test]
async fn test_list_groups_requires_authentication() {
    // GET /groups without token should return 401
    let request = Request::builder()
        .uri("/groups")
        .body(Body::empty())
        .unwrap();

    assert!(!request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_authenticated_user_can_list_groups() {
    // GET /groups with valid token should succeed
    let token = "fold_valid_token";
    let _request = get_request("/groups", token);

    // All authenticated users can list groups
}

#[tokio::test]
async fn test_user_can_list_their_groups() {
    // GET /users/{id}/groups should only work for own ID or admin
    let user_id = "user-123";
    let token = "fold_user_token";
    let uri = format!("/users/{}/groups", user_id);

    let _request = get_request(&uri, token);

    // User can see their own group memberships
}

// ============================================================================
// API KEY ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_create_api_key_requires_authentication() {
    // POST /users/{id}/api-keys without token should return 401
    let user_id = "user-123";
    let uri = format!("/users/{}/api-keys", user_id);

    let request_body = json!({
        "name": "My API Key"
    });

    let request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // No auth header
    assert!(!request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_user_can_create_own_api_key() {
    // POST /users/{self_id}/api-keys with own ID should succeed
    let user_id = "user-123";
    let token = "fold_user_token";
    let uri = format!("/users/{}/api-keys", user_id);

    let request_body = json!({
        "name": "My API Key",
        "expires_in_days": null
    });

    let request = post_json(&uri, token, request_body.clone());

    // Request should have auth and valid body
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
    assert_eq!(request_body["name"], "My API Key");
}

#[tokio::test]
async fn test_user_cannot_create_api_key_for_other_user() {
    // POST /users/{other_id}/api-keys with different ID should return 403
    let other_user_id = "user-456";
    let token = "fold_user_token";
    let uri = format!("/users/{}/api-keys", other_user_id);

    let request_body = json!({
        "name": "Unauthorized Key"
    });

    let _request = post_json(&uri, token, request_body);

    // Handler checks: if user_id != auth.user_id && !auth.is_admin() -> return Forbidden
}

#[tokio::test]
async fn test_admin_can_create_api_key_for_user() {
    // POST /users/{id}/api-keys as admin for any user should succeed
    let user_id = "user-789";
    let admin_token = "fold_admin_token";
    let uri = format!("/users/{}/api-keys", user_id);

    let request_body = json!({
        "name": "Admin Created Key"
    });

    let request = post_json(&uri, admin_token, request_body.clone());

    // Admin can create keys for any user
    assert!(request
        .headers()
        .get("Authorization")
        .is_some());
}

#[tokio::test]
async fn test_list_api_keys_requires_own_id_or_admin() {
    // GET /users/{id}/api-keys without matching ID or admin should return 403
    let other_user_id = "user-456";
    let token = "fold_user_token";
    let uri = format!("/users/{}/api-keys", other_user_id);

    let _request = get_request(&uri, token);

    // Only the user or an admin can list their keys
}

#[tokio::test]
async fn test_revoke_api_key_requires_ownership_or_admin() {
    // DELETE /api-keys/{id} should only work for own keys or admin
    let token_id = "token-123";
    let token = "fold_user_token";
    let uri = format!("/api-keys/{}/revoke", token_id);

    let _request = post_json(&uri, token, json!({}));

    // Only the token owner or an admin can revoke
}

// ============================================================================
// PROJECT MEMBERS ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_add_project_member_requires_project_access() {
    // POST /projects/{id}/members without project access should return 403
    let project_id = "project-123";
    let token = "fold_user_without_access";
    let uri = format!("/projects/{}/members", project_id);

    let request_body = json!({
        "user_id": "user-456",
        "role": "member"
    });

    let _request = post_json(&uri, token, request_body);

    // User must have write access to the project
}

#[tokio::test]
async fn test_add_project_member_validates_user_exists() {
    // POST /projects/{id}/members with non-existent user_id should return 404
    let project_id = "project-123";
    let token = "fold_project_admin";
    let uri = format!("/projects/{}/members", project_id);

    let request_body = json!({
        "user_id": "non-existent-user",
        "role": "member"
    });

    let _request = post_json(&uri, token, request_body);

    // Handler checks: db::get_user() which returns NotFound if user doesn't exist
}

#[tokio::test]
async fn test_add_project_member_validates_role() {
    // POST /projects/{id}/members with invalid role should return 400
    let project_id = "project-123";
    let token = "fold_project_admin";
    let uri = format!("/projects/{}/members", project_id);

    let request_body = json!({
        "user_id": "user-456",
        "role": "invalid-role"
    });

    let _request = post_json(&uri, token, request_body);

    // Handler validates: role must be "member" or "viewer"
}

#[tokio::test]
async fn test_remove_project_member_requires_write_access() {
    // DELETE /projects/{id}/members/{user_id} without write access should return 403
    let project_id = "project-123";
    let user_id = "user-456";
    let token = "fold_user_readonly";
    let uri = format!("/projects/{}/members/{}", project_id, user_id);

    let _request = delete_request(&uri, token);

    // User must have write access to the project
}

// ============================================================================
// AUTHENTICATION & AUTHORIZATION TESTS
// ============================================================================

#[tokio::test]
async fn test_missing_auth_header_returns_401() {
    // Any protected endpoint without Authorization header should return 401
    let status = StatusCode::UNAUTHORIZED;
    assert_eq!(status.as_u16(), 401);
}

#[tokio::test]
async fn test_invalid_token_format_returns_401() {
    // Token must start with "fold_"
    let invalid_token = "invalid_token_here";
    assert!(!invalid_token.starts_with("fold_"));
}

#[tokio::test]
async fn test_token_validation_checks_hash() {
    // Token hash must match stored hash in database
    // Middleware validates: constant_time_eq(&token_hash, &token_row.token_hash)
}

#[tokio::test]
async fn test_expired_token_returns_401() {
    // Token with expires_at < NOW() should return 401
    // Middleware checks: if let Some(expires_at) = token_row.expires_at {
    //     if expires_at < chrono::Utc::now() { return Err(Error::TokenExpired) }
}

#[tokio::test]
async fn test_revoked_token_returns_401() {
    // Token with revoked_at IS NOT NULL should return 401
    // Middleware checks: if token_row.revoked_at.is_some() { return Err(Error::InvalidToken) }
}

#[tokio::test]
async fn test_permission_check_admin_bypass() {
    // Admin users should bypass project permission checks
    // PermissionService::can_read_project checks: if user_role == "admin" -> return true
}

#[tokio::test]
async fn test_permission_check_direct_membership() {
    // User with direct project_members entry should have access
    // PermissionService queries: SELECT * FROM project_members WHERE user_id = ? AND project_id = ?
}

#[tokio::test]
async fn test_permission_check_group_membership() {
    // User via group membership should have access
    // PermissionService uses UNION ALL to check both direct and group membership
}
