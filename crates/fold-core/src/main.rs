//! Fold - Holographic Memory System
//!
//! A semantic knowledge storage system for development teams with git integration
//! and multi-provider LLM support.

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod config;
mod db;
mod error;
mod middleware;
mod models;
mod services;
mod state;

pub use config::config;
pub use error::{Error, Result};
pub use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Create event broadcaster early so it can be used by tracing layer
    let events = std::sync::Arc::new(services::EventBroadcaster::new());

    // Initialize tracing with SSE layer for real-time log streaming
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fold=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(services::SseTracingLayer::new(events.clone()))
        .init();

    // Load configuration
    let config = config::init();
    tracing::info!(
        "Starting Fold server on {}:{}",
        config.server.host,
        config.server.port
    );

    // Initialize application state with the pre-created event broadcaster
    let state = AppState::new_with_events(events).await?;
    tracing::info!("Application state initialized");

    // Initialize startup time for uptime tracking
    api::status::init_startup_time();

    // Start background job worker
    let job_worker = services::JobWorker::new(
        state.db.clone(),
        state.memory.clone(),
        state.git_sync.clone(),
        state.github.clone(),
        state.git_local.clone(),
        state.indexer.clone(),
        state.llm.clone(),
        state.embeddings.clone(),
        state.events.clone(),
    );
    let _job_worker_handle = job_worker.start().await;
    tracing::info!("Background job worker started");

    // Start MCP session cleanup task
    api::mcp::start_session_cleanup();
    tracing::debug!("MCP session cleanup task started");

    // Build router
    let app = Router::new()
        .merge(api::routes(state.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .expect("Invalid address");

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);
    tracing::info!("========================================");
    tracing::info!("  FOLD SERVER STARTED SUCCESSFULLY");
    tracing::info!("  Ready to accept connections on {}", addr);
    tracing::info!("========================================");

    // Check if TLS is configured
    if let (Some(cert_path), Some(key_path)) =
        (&config.server.tls_cert_path, &config.server.tls_key_path)
    {
        tracing::warn!(
            "TLS configuration found ({}, {}) but is not yet implemented",
            cert_path,
            key_path
        );
        tracing::info!("Running on HTTP (not HTTPS)");
    }

    // Start HTTP server
    axum::serve(listener, app).await?;

    Ok(())
}
