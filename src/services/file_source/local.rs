//! Local filesystem file source provider.
//!
//! Provides file source operations for local directories.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::fs;

use super::{
    ChangeDetectionResult, ChangeEvent, FileContent, FileInfo, FileSourceProvider,
    NotificationConfig, NotificationType, SourceConfig, SourceInfo,
};
use crate::error::Result;

/// Local filesystem file source provider.
pub struct LocalFileSource {
    base_path: Option<PathBuf>,
}

impl LocalFileSource {
    /// Create a new local file source.
    pub fn new() -> Self {
        Self { base_path: None }
    }

    /// Create a local file source with a base path.
    pub fn with_base_path(base_path: PathBuf) -> Self {
        Self {
            base_path: Some(base_path),
        }
    }

    /// Resolve a path relative to the source.
    fn resolve_path(&self, source: &SourceInfo, path: &str) -> PathBuf {
        let base = self
            .base_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&source.name));
        base.join(path)
    }
}

impl Default for LocalFileSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSourceProvider for LocalFileSource {
    fn provider_type(&self) -> &'static str {
        "local"
    }

    fn display_name(&self) -> &'static str {
        "Local Filesystem"
    }

    fn supports_webhooks(&self) -> bool {
        false
    }

    async fn connect(&self, config: SourceConfig, _token: &str) -> Result<SourceInfo> {
        let path = PathBuf::from(&config.name);

        // Verify the path exists
        if !path.exists() {
            return Err(crate::error::Error::NotFound(format!(
                "Path not found: {}",
                config.name
            )));
        }

        Ok(SourceInfo {
            id: config.name.clone(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| config.name.clone()),
            full_name: config.name.clone(),
            url: Some(format!("file://{}", config.name)),
            default_version: None,
            is_private: false,
            owner: None,
            metadata: serde_json::json!({}),
        })
    }

    async fn disconnect(&self, _source: &SourceInfo, _token: &str) -> Result<()> {
        Ok(())
    }

    async fn get_file(
        &self,
        source: &SourceInfo,
        path: &str,
        _version: Option<&str>,
        _token: &str,
    ) -> Result<FileContent> {
        let file_path = self.resolve_path(source, path);

        if !file_path.exists() {
            return Err(crate::error::Error::NotFound(format!(
                "File not found: {}",
                path
            )));
        }

        let metadata = fs::metadata(&file_path).await?;
        let modified_at = metadata
            .modified()
            .ok()
            .map(|t| DateTime::<Utc>::from(t));

        let content = fs::read_to_string(&file_path).await.ok();
        let bytes = if content.is_none() {
            Some(fs::read(&file_path).await?)
        } else {
            None
        };

        Ok(FileContent {
            path: path.to_string(),
            name: file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            content,
            bytes,
            hash: None,
            size: metadata.len() as i64,
            mime_type: None,
            modified_at,
        })
    }

    async fn list_files(
        &self,
        source: &SourceInfo,
        prefix: Option<&str>,
        _version: Option<&str>,
        _token: &str,
    ) -> Result<Vec<FileInfo>> {
        let base_path = self.resolve_path(source, prefix.unwrap_or(""));

        if !base_path.exists() {
            return Ok(vec![]);
        }

        let mut files = Vec::new();
        let mut entries = fs::read_dir(&base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            let path = entry.path();
            let relative_path = path
                .strip_prefix(self.base_path.as_ref().unwrap_or(&PathBuf::from(&source.name)))
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            files.push(FileInfo {
                path: relative_path,
                name: entry.file_name().to_string_lossy().to_string(),
                is_directory: metadata.is_dir(),
                size: metadata.len() as i64,
                hash: None,
                modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
            });
        }

        Ok(files)
    }

    async fn register_notifications(
        &self,
        _source: &SourceInfo,
        _callback_url: &str,
        _secret: &str,
        _token: &str,
    ) -> Result<NotificationConfig> {
        // Local filesystem uses polling
        Ok(NotificationConfig {
            notification_type: NotificationType::Polling,
            notification_id: uuid::Uuid::new_v4().to_string(),
            events: vec!["file_changed".to_string()],
            poll_interval_secs: Some(60),
            expires_at: None,
        })
    }

    async fn unregister_notifications(
        &self,
        _source: &SourceInfo,
        _notification_id: &str,
        _token: &str,
    ) -> Result<()> {
        Ok(())
    }

    fn verify_notification(&self, _payload: &[u8], _signature: &str, _secret: &str) -> bool {
        // Local filesystem doesn't have signatures
        true
    }

    fn parse_notification(&self, _event_type: &str, _payload: &[u8]) -> Result<Vec<ChangeEvent>> {
        // Local filesystem uses polling, not webhooks
        Ok(vec![])
    }

    fn supported_events(&self) -> Vec<&'static str> {
        vec!["file_created", "file_modified", "file_deleted"]
    }

    async fn detect_changes(
        &self,
        _source: &SourceInfo,
        _cursor: Option<&str>,
        _token: &str,
    ) -> Result<ChangeDetectionResult> {
        // Basic implementation - would need file watching for real change detection
        Ok(ChangeDetectionResult {
            events: vec![],
            next_cursor: None,
            has_more: false,
        })
    }

    async fn write_file(
        &self,
        source: &SourceInfo,
        path: &str,
        content: &[u8],
        _token: &str,
    ) -> Result<()> {
        let file_path = self.resolve_path(source, path);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&file_path, content).await?;
        Ok(())
    }

    async fn delete_file(&self, source: &SourceInfo, path: &str, _token: &str) -> Result<()> {
        let file_path = self.resolve_path(source, path);

        if file_path.exists() {
            fs::remove_file(&file_path).await?;
        }

        Ok(())
    }
}
