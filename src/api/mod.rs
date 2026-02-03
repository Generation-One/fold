//! API Routes for Fold
//!
//! This module combines all API routes into a single router.
//! Routes are organized by domain and apply appropriate middleware.

mod auth;
pub mod mcp;
mod memories;
mod projects;
mod providers;
mod repositories;
mod search;
pub mod status;
mod webhooks;

use axum::Router;

use crate::middleware::require_token;
use crate::AppState;

/// Build the complete API router.
///
/// Route structure:
/// - /auth/* - Authentication (public + session-protected)
/// - /projects/* - Project management (token-protected)
/// - /providers/* - Provider management (session-protected, admin)
/// - /mcp - MCP JSON-RPC endpoint (token-protected)
/// - /webhooks/* - Git webhooks (signature-verified)
/// - /health, /status, /metrics - Health checks (public)
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Health and status endpoints (public)
        .merge(status::routes())
        // Authentication routes (mixed public/protected)
        .nest("/auth", auth::routes(state.clone()))
        // Webhook routes (signature-verified, no auth middleware)
        .nest("/webhooks", webhooks::routes())
        // MCP endpoint (token auth)
        .nest("/mcp", mcp::routes(state.clone()))
        // Provider OAuth routes (public - initiates OAuth flows)
        .nest("/providers", providers::oauth_routes())
        // Provider management (token auth for admin API)
        .nest("/providers", admin_routes(state.clone()))
        // Protected API routes
        .nest("/projects", protected_routes(state))
}

/// Protected routes that require authentication.
fn protected_routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Project CRUD
        .merge(projects::routes())
        // Nested project resources
        .nest("/:project_id/members", projects::members_routes())
        .nest("/:project_id/memories", memories::routes())
        .nest("/:project_id/repositories", repositories::routes())
        .nest("/:project_id/config", projects::config_routes())
        // Search and context endpoints
        .merge(search::routes())
        // File source provider information (non-project-specific)
        .nest("/file-sources", repositories::file_source_routes())
        // Apply token authentication to all protected routes
        .layer(axum::middleware::from_fn_with_state(state, require_token))
}

/// Admin routes that require authentication (token or session).
/// These routes manage system-wide settings like LLM providers.
pub fn admin_routes(state: AppState) -> Router<AppState> {
    Router::new()
        // Provider management (LLM and embedding providers)
        .merge(providers::routes())
        // Use token auth for API access (same as other protected routes)
        .layer(axum::middleware::from_fn_with_state(state, require_token))
}
