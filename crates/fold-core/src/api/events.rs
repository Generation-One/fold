//! Server-Sent Events (SSE) endpoint for real-time notifications.
//!
//! Provides a secure, filtered event stream for authenticated clients.
//! Events are filtered based on user's project membership and role.
//! Admin-only events (like job logs) require admin privileges.

use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Extension, Router,
};
use futures::stream::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, warn};

use crate::db;
use crate::middleware::AuthContext;
use crate::services::events::FoldEvent;
use crate::AppState;

/// User context for SSE filtering.
struct SseUserContext {
    user_id: String,
    is_admin: bool,
    project_ids: HashSet<String>,
}

/// SSE endpoint handler for real-time events.
///
/// Requires token authentication. Events are filtered to only include
/// events for projects the user has access to, plus global events
/// like provider availability and health status.
/// Admin-only events (job logs) are only sent to admin users.
pub async fn events_stream(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Get user's role to check admin status
    let is_admin = match db::get_user(&state.db, &auth.user_id).await {
        Ok(user) => user.role == "admin",
        Err(e) => {
            warn!(
                user_id = %auth.user_id,
                error = %e,
                "Failed to load user role for SSE filtering"
            );
            false
        }
    };

    // Get user's accessible project IDs for filtering
    let user_projects: HashSet<String> =
        match db::list_user_projects(&state.db, &auth.user_id).await {
            Ok(projects) => projects.into_iter().map(|p| p.id).collect(),
            Err(e) => {
                warn!(
                    user_id = %auth.user_id,
                    error = %e,
                    "Failed to load user projects for SSE filtering"
                );
                HashSet::new()
            }
        };

    debug!(
        user_id = %auth.user_id,
        is_admin,
        project_count = user_projects.len(),
        "SSE connection established"
    );

    let user_ctx = SseUserContext {
        user_id: auth.user_id.clone(),
        is_admin,
        project_ids: user_projects,
    };

    // Subscribe to the event broadcaster
    let receiver = state.events.subscribe();
    let stream = BroadcastStream::new(receiver);

    // Filter events based on user's project access and admin status
    let event_stream = stream.filter_map(move |result| {
        let ctx = SseUserContext {
            user_id: user_ctx.user_id.clone(),
            is_admin: user_ctx.is_admin,
            project_ids: user_ctx.project_ids.clone(),
        };
        async move {
            match result {
                Ok(event) => {
                    // Check if user should receive this event
                    if should_send_event(&event, &ctx) {
                        // Serialize event to JSON
                        match serde_json::to_string(&event) {
                            Ok(json) => {
                                Some(Ok(Event::default().event(event.event_type()).data(json)))
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to serialize SSE event");
                                None
                            }
                        }
                    } else {
                        // User doesn't have access to this event
                        None
                    }
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(count)) => {
                    // Channel lagged, some events were dropped
                    warn!(count, "SSE stream lagged, events dropped");
                    None
                }
            }
        }
    });

    // Configure SSE with keepalive
    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("heartbeat"),
    )
}

/// Check if an event should be sent to a user based on project access and role.
///
/// - Global events (provider, health, heartbeat) are sent to all authenticated users.
/// - Admin-only events (job logs) are only sent to admin users.
/// - Project-scoped events are only sent if the user has access to the project.
fn should_send_event(event: &FoldEvent, ctx: &SseUserContext) -> bool {
    // Admin-only events require admin privileges
    if event.is_admin_only() && !ctx.is_admin {
        return false;
    }

    // Global events are always sent (to users with appropriate privileges)
    if event.is_global() {
        return true;
    }

    // For project-scoped events, check if user has access
    match event.project_id() {
        Some(project_id) => ctx.project_ids.contains(project_id),
        // Events without a project_id are treated as global
        None => true,
    }
}

/// Build the events router.
pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(events_stream))
}
