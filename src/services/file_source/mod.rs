//! File Source Provider abstraction.
//!
//! This module provides a generic interface for file sources, allowing Fold to
//! index and track changes from multiple providers:
//! - Git providers (GitHub, GitLab)
//! - Cloud storage (Google Drive, OneDrive)
//! - Local filesystem
//!
//! Each provider implements the `FileSourceProvider` trait, which defines
//! common operations for connecting, listing files, retrieving content,
//! and receiving change notifications.

mod github;
mod google_drive;
mod local;
mod registry;
mod types;

pub use github::GitHubFileSource;
pub use google_drive::GoogleDriveFileSource;
pub use local::LocalFileSource;
pub use registry::ProviderRegistry;
pub use types::*;

use async_trait::async_trait;

use crate::error::Result;

/// Core trait for file source providers.
///
/// Implementors provide access to files from various sources (GitHub, Google Drive, etc.)
/// with a unified interface for connecting, listing, retrieving content, and
/// receiving change notifications.
#[async_trait]
pub trait FileSourceProvider: Send + Sync {
    /// Unique identifier for this provider type (e.g., "github", "google-drive").
    fn provider_type(&self) -> &'static str;

    /// Human-readable display name.
    fn display_name(&self) -> &'static str;

    /// Whether this provider supports webhooks for real-time notifications.
    fn supports_webhooks(&self) -> bool;

    /// Whether this provider requires polling for change detection.
    fn requires_polling(&self) -> bool {
        !self.supports_webhooks()
    }

    /// Connect to a file source and return metadata.
    ///
    /// For Git providers, this might involve fetching repository info.
    /// For cloud storage, this might validate folder access.
    async fn connect(&self, config: SourceConfig, token: &str) -> Result<SourceInfo>;

    /// Disconnect from a file source.
    ///
    /// This should clean up any resources like webhooks.
    async fn disconnect(&self, source: &SourceInfo, token: &str) -> Result<()>;

    /// Get file content from the source.
    ///
    /// - `source`: The connected source info
    /// - `path`: File path relative to the source root
    /// - `version`: Optional version/ref (commit SHA, revision ID, etc.)
    /// - `token`: Access token
    async fn get_file(
        &self,
        source: &SourceInfo,
        path: &str,
        version: Option<&str>,
        token: &str,
    ) -> Result<FileContent>;

    /// List files in a directory/prefix.
    ///
    /// - `source`: The connected source info
    /// - `prefix`: Optional path prefix to filter
    /// - `version`: Optional version/ref
    /// - `token`: Access token
    async fn list_files(
        &self,
        source: &SourceInfo,
        prefix: Option<&str>,
        version: Option<&str>,
        token: &str,
    ) -> Result<Vec<FileInfo>>;

    /// Register for change notifications.
    ///
    /// For webhook-capable providers, this registers a webhook.
    /// For polling providers, this returns polling configuration.
    async fn register_notifications(
        &self,
        source: &SourceInfo,
        callback_url: &str,
        secret: &str,
        token: &str,
    ) -> Result<NotificationConfig>;

    /// Unregister from change notifications.
    async fn unregister_notifications(
        &self,
        source: &SourceInfo,
        notification_id: &str,
        token: &str,
    ) -> Result<()>;

    /// Verify an incoming notification's authenticity.
    ///
    /// For webhooks, this verifies the signature.
    /// Returns true if the notification is valid.
    fn verify_notification(&self, payload: &[u8], signature: &str, secret: &str) -> bool;

    /// Parse an incoming notification into change events.
    ///
    /// Returns the event type and parsed events.
    fn parse_notification(&self, event_type: &str, payload: &[u8]) -> Result<Vec<ChangeEvent>>;

    /// Get the list of event types this provider can emit.
    fn supported_events(&self) -> Vec<&'static str>;

    /// Detect changes since last sync (for polling providers).
    ///
    /// Returns changes since the given cursor/token.
    async fn detect_changes(
        &self,
        source: &SourceInfo,
        cursor: Option<&str>,
        token: &str,
    ) -> Result<ChangeDetectionResult>;

    /// Write file content to the source.
    ///
    /// - `source`: The connected source info
    /// - `path`: File path relative to the source root
    /// - `content`: File content as bytes
    /// - `token`: Access token
    ///
    /// Returns Ok(()) on success. Not all providers support writing.
    async fn write_file(
        &self,
        source: &SourceInfo,
        path: &str,
        content: &[u8],
        token: &str,
    ) -> Result<()> {
        let _ = (source, path, content, token);
        Err(crate::error::Error::Validation(
            "Write not supported by this provider".to_string(),
        ))
    }

    /// Delete a file from the source.
    ///
    /// - `source`: The connected source info
    /// - `path`: File path relative to the source root
    /// - `token`: Access token
    ///
    /// Returns Ok(()) on success. Not all providers support deletion.
    async fn delete_file(
        &self,
        source: &SourceInfo,
        path: &str,
        token: &str,
    ) -> Result<()> {
        let _ = (source, path, token);
        Err(crate::error::Error::Validation(
            "Delete not supported by this provider".to_string(),
        ))
    }
}
