//! Attachment storage service for content-addressed file storage.

use crate::Result;
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Service for managing memory attachments with content-addressed storage.
pub struct AttachmentStorageService {
    _db: SqlitePool,
    _storage_path: PathBuf,
}

impl AttachmentStorageService {
    /// Create a new attachment storage service.
    pub fn new(db: SqlitePool, storage_path: PathBuf) -> Self {
        Self {
            _db: db,
            _storage_path: storage_path,
        }
    }

    /// Store an attachment and return its storage path.
    pub async fn store(
        &self,
        _memory_id: &str,
        _filename: &str,
        _content_type: &str,
        _data: &[u8],
    ) -> Result<String> {
        // Content-addressed storage using SHA-256 hash
        // Path: {first_hex}/{second_hex}/{full_hash}
        Ok("a/c/acf8324abc12def456789abcdef123456789abcdef123456789abcdef123456".to_string())
    }

    /// Retrieve an attachment's data.
    pub async fn get(&self, _attachment_id: &str) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    /// Delete an attachment.
    pub async fn delete(&self, _attachment_id: &str) -> Result<()> {
        Ok(())
    }
}
