//! Event broadcaster for Server-Sent Events (SSE).
//!
//! Provides a multi-client broadcast channel for real-time event distribution.
//! Services emit events through this broadcaster, and SSE connections subscribe
//! to receive filtered events based on user permissions.

use std::sync::Arc;
use tokio::sync::broadcast;

use super::events::{
    FoldEvent, HeartbeatEvent, IndexingEvent, IndexingProgressEvent, JobEvent, JobFailedEvent,
    JobLogEvent, JobProgressEvent, ProviderEvent,
};

/// Channel capacity for event broadcasting.
/// Should be large enough to handle bursts without losing events.
const CHANNEL_CAPACITY: usize = 1000;

/// Event broadcaster service for SSE.
///
/// Uses a tokio broadcast channel to distribute events to multiple subscribers.
/// Services emit events through this broadcaster, and the SSE endpoint subscribes
/// to forward events to connected clients.
#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<FoldEvent>,
}

impl EventBroadcaster {
    /// Create a new event broadcaster.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Emit an event to all subscribers.
    ///
    /// If there are no subscribers, the event is silently dropped.
    pub fn emit(&self, event: FoldEvent) {
        // Ignore send errors (no subscribers is fine)
        let _ = self.sender.send(event);
    }

    /// Subscribe to events.
    ///
    /// Returns a receiver that will receive all broadcast events.
    /// The receiver should filter events based on user permissions.
    pub fn subscribe(&self) -> broadcast::Receiver<FoldEvent> {
        self.sender.subscribe()
    }

    /// Emit a job started event.
    pub fn job_started(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
    ) {
        self.emit(FoldEvent::JobStarted(JobEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit a job progress event.
    pub fn job_progress(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
        processed: i32,
        failed: i32,
        total: Option<i32>,
    ) {
        let percent = total.map(|t| {
            if t > 0 {
                (processed as f64 / t as f64) * 100.0
            } else {
                0.0
            }
        });

        self.emit(FoldEvent::JobProgress(JobProgressEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            processed,
            failed,
            total,
            percent,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit a job completed event.
    pub fn job_completed(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
    ) {
        self.emit(FoldEvent::JobCompleted(JobEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit a job failed event.
    pub fn job_failed(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
        error: &str,
    ) {
        self.emit(FoldEvent::JobFailed(JobFailedEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            error: error.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit a job paused event.
    pub fn job_paused(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
    ) {
        self.emit(FoldEvent::JobPaused(JobEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit a job resumed event.
    pub fn job_resumed(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
    ) {
        self.emit(FoldEvent::JobResumed(JobEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit an indexing started event.
    pub fn indexing_started(&self, project_id: &str, project_name: &str) {
        self.emit(FoldEvent::IndexingStarted(IndexingEvent {
            project_id: project_id.to_string(),
            project_name: project_name.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit an indexing progress event.
    pub fn indexing_progress(
        &self,
        project_id: &str,
        project_name: &str,
        files_indexed: i32,
        files_total: Option<i32>,
        current_file: Option<&str>,
    ) {
        self.emit(FoldEvent::IndexingProgress(IndexingProgressEvent {
            project_id: project_id.to_string(),
            project_name: project_name.to_string(),
            files_indexed,
            files_total,
            current_file: current_file.map(String::from),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit an indexing completed event.
    pub fn indexing_completed(&self, project_id: &str, project_name: &str) {
        self.emit(FoldEvent::IndexingCompleted(IndexingEvent {
            project_id: project_id.to_string(),
            project_name: project_name.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit a provider status change event.
    pub fn provider_status_changed(
        &self,
        provider_type: &str,
        provider_name: &str,
        available: bool,
    ) {
        let event = if available {
            FoldEvent::ProviderAvailable(ProviderEvent {
                provider_type: provider_type.to_string(),
                provider_name: provider_name.to_string(),
                available,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        } else {
            FoldEvent::ProviderUnavailable(ProviderEvent {
                provider_type: provider_type.to_string(),
                provider_name: provider_name.to_string(),
                available,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        };
        self.emit(event);
    }

    /// Emit a heartbeat event.
    pub fn heartbeat(&self) {
        self.emit(FoldEvent::Heartbeat(HeartbeatEvent::now()));
    }

    /// Emit a job log event (admin-only).
    pub fn job_log(
        &self,
        job_id: &str,
        job_type: &str,
        project_id: Option<&str>,
        project_name: Option<&str>,
        level: &str,
        message: &str,
        metadata: Option<serde_json::Value>,
    ) {
        self.emit(FoldEvent::JobLog(JobLogEvent {
            job_id: job_id.to_string(),
            job_type: job_type.to_string(),
            project_id: project_id.map(String::from),
            project_name: project_name.map(String::from),
            level: level.to_string(),
            message: message.to_string(),
            metadata,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }));
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared event broadcaster wrapped in Arc for use across services.
pub type SharedEventBroadcaster = Arc<EventBroadcaster>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_broadcast() {
        let broadcaster = EventBroadcaster::new();
        let mut receiver = broadcaster.subscribe();

        broadcaster.job_started("job-1", "index_repo", Some("proj-1"), Some("Test Project"));

        let event = receiver.recv().await.unwrap();
        match event {
            FoldEvent::JobStarted(e) => {
                assert_eq!(e.job_id, "job-1");
                assert_eq!(e.job_type, "index_repo");
                assert_eq!(e.project_id.as_deref(), Some("proj-1"));
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let broadcaster = EventBroadcaster::new();
        let mut receiver1 = broadcaster.subscribe();
        let mut receiver2 = broadcaster.subscribe();

        broadcaster.heartbeat();

        let event1 = receiver1.recv().await.unwrap();
        let event2 = receiver2.recv().await.unwrap();

        assert!(matches!(event1, FoldEvent::Heartbeat(_)));
        assert!(matches!(event2, FoldEvent::Heartbeat(_)));
    }

    #[test]
    fn test_no_subscribers_ok() {
        let broadcaster = EventBroadcaster::new();
        // Should not panic when there are no subscribers
        broadcaster.heartbeat();
        broadcaster.job_started("job-1", "test", None, None);
    }
}
