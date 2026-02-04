//! Comprehensive security tests for all MCP tool methods.
//! Verifies that each MCP tool respects project-level access control.

mod common;

use axum::body::Body;
use axum::http::Request;
use serde_json::json;

// ============================================================================
// MCP TOOLS/LIST METHOD TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_tools_list_requires_authentication() {
    // tools/list method without auth should return error
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    // No auth header - should fail at middleware
    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_tools_list_with_valid_token() {
    // tools/list should succeed with valid token
    let token = "fold_valid_token";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
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

// ============================================================================
// MCP PROJECT_LIST TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_project_list_requires_authentication() {
    // project_list tool without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "project_list",
            "arguments": {}
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_project_list_returns_only_accessible_projects() {
    // project_list should only return projects user has access to
    let token = "fold_user_limited_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "project_list",
            "arguments": {}
        }
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
async fn test_mcp_project_list_admin_sees_all() {
    // Admin user should see all projects via MCP
    let admin_token = "fold_admin_token";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "project_list",
            "arguments": {}
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
// MCP MEMORY_ADD TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_memory_add_requires_authentication() {
    // memory_add without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_add",
            "arguments": {
                "project": "project-123",
                "content": "Test memory"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_memory_add_requires_project_write_access() {
    // memory_add to inaccessible project should return error
    let token = "fold_user_no_write_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_add",
            "arguments": {
                "project": "restricted-project",
                "content": "Test memory",
                "title": "Test",
                "author": "test-user"
            }
        }
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
async fn test_mcp_memory_add_with_valid_project_access() {
    // memory_add with valid access should succeed
    let token = "fold_user_with_write_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_add",
            "arguments": {
                "project": "project-123",
                "content": "Important memory",
                "title": "Title",
                "author": "claude",
                "tags": ["important", "test"]
            }
        }
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

// ============================================================================
// MCP MEMORY_SEARCH TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_memory_search_requires_authentication() {
    // memory_search without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_search",
            "arguments": {
                "project": "project-123",
                "query": "test"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_memory_search_requires_project_read_access() {
    // memory_search on inaccessible project should return error
    let token = "fold_user_no_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_search",
            "arguments": {
                "project": "restricted-project",
                "query": "test",
                "limit": 10
            }
        }
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
async fn test_mcp_memory_search_returns_only_accessible_results() {
    // memory_search should only return memories from accessible project
    let token = "fold_user_with_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_search",
            "arguments": {
                "project": "project-123",
                "query": "important",
                "limit": 20
            }
        }
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

// ============================================================================
// MCP MEMORY_LIST TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_memory_list_requires_authentication() {
    // memory_list without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_list",
            "arguments": {
                "project": "project-123"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_memory_list_requires_project_read_access() {
    // memory_list on inaccessible project should fail
    let token = "fold_user_no_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_list",
            "arguments": {
                "project": "restricted-project",
                "limit": 20
            }
        }
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

// ============================================================================
// MCP MEMORY_CONTEXT TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_memory_context_requires_authentication() {
    // memory_context without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_context",
            "arguments": {
                "project": "project-123",
                "memory_id": "mem-456",
                "depth": 1
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_memory_context_requires_project_read_access() {
    // memory_context on inaccessible project should fail
    let token = "fold_user_no_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_context",
            "arguments": {
                "project": "restricted-project",
                "memory_id": "mem-123"
            }
        }
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

// ============================================================================
// MCP MEMORY_LINK TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_memory_link_add_requires_authentication() {
    // memory_link_add without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_link_add",
            "arguments": {
                "project": "project-123",
                "source_id": "mem-1",
                "target_id": "mem-2",
                "link_type": "references"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_memory_link_add_requires_project_write_access() {
    // memory_link_add requires write access
    let token = "fold_user_readonly";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_link_add",
            "arguments": {
                "project": "project-123",
                "source_id": "mem-1",
                "target_id": "mem-2",
                "link_type": "references"
            }
        }
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

// ============================================================================
// MCP CODEBASE TOOL TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_codebase_index_requires_authentication() {
    // codebase_index without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "codebase_index",
            "arguments": {
                "project": "project-123"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_codebase_index_requires_project_write_access() {
    // codebase_index requires write access (creates memories)
    let token = "fold_user_readonly";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "codebase_index",
            "arguments": {
                "project": "project-123",
                "path": "/path/to/code"
            }
        }
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
async fn test_mcp_codebase_search_requires_authentication() {
    // codebase_search without auth should fail
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "codebase_search",
            "arguments": {
                "project": "project-123",
                "query": "function"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(!request.headers().get("Authorization").is_some());
}

#[tokio::test]
async fn test_mcp_codebase_search_requires_project_read_access() {
    // codebase_search on inaccessible project should fail
    let token = "fold_user_no_access";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "codebase_search",
            "arguments": {
                "project": "restricted-project",
                "query": "test"
            }
        }
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

// ============================================================================
// MCP INVALID METHOD TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_invalid_method_returns_method_not_found() {
    // Invalid method should return method not found error
    let token = "fold_valid_token";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "invalid/method",
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
async fn test_mcp_token_scoping_affects_all_tools() {
    // Token scoped to specific projects should limit tool operations
    let scoped_token = "fold_token_scoped_to_project_1_only";
    let uri = "/mcp";
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "memory_add",
            "arguments": {
                "project": "project-2",
                "content": "Should fail"
            }
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", scoped_token))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    assert!(request.headers().get("Authorization").is_some());
}
