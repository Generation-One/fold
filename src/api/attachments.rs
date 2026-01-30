//! Attachments Routes
//!
//! File attachment operations for memories.
//!
//! Routes:
//! - GET /projects/:project_id/memories/:memory_id/attachments - List attachments
//! - POST /projects/:project_id/memories/:memory_id/attachments - Upload attachment
//! - GET /projects/:project_id/memories/:memory_id/attachments/:id - Download attachment
//! - DELETE /projects/:project_id/memories/:memory_id/attachments/:id - Delete attachment

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::header,
    response::Response,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build attachment routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_attachments).post(upload_attachment))
        .route(
            "/:attachment_id",
            get(download_attachment).delete(delete_attachment),
        )
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Attachment metadata response.
#[derive(Debug, Serialize)]
pub struct AttachmentResponse {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
}

/// List attachments response.
#[derive(Debug, Serialize)]
pub struct ListAttachmentsResponse {
    pub attachments: Vec<AttachmentResponse>,
    pub total: u32,
}

/// Upload response.
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub attachment: AttachmentResponse,
    pub message: String,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct MemoryAttachmentPath {
    pub project_id: String,
    pub memory_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentPath {
    pub project_id: String,
    pub memory_id: Uuid,
    pub attachment_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// List attachments for a memory.
///
/// GET /projects/:project_id/memories/:memory_id/attachments
#[axum::debug_handler]
async fn list_attachments(
    State(_state): State<AppState>,
    Path(path): Path<MemoryAttachmentPath>,
) -> Result<Json<ListAttachmentsResponse>> {
    let _memory_id = path.memory_id;

    // TODO: Verify memory exists and user has access
    // TODO: Fetch attachments from database

    Ok(Json(ListAttachmentsResponse {
        attachments: vec![],
        total: 0,
    }))
}

/// Upload an attachment to a memory.
///
/// POST /projects/:project_id/memories/:memory_id/attachments
///
/// Accepts multipart/form-data with a single file field named "file".
#[axum::debug_handler]
async fn upload_attachment(
    State(_state): State<AppState>,
    Path(path): Path<MemoryAttachmentPath>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>> {
    let _memory_id = path.memory_id;
    let config = crate::config();

    // TODO: Verify memory exists and user has access

    // Process multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        Error::InvalidInput(format!("Failed to read multipart field: {}", e))
    })? {
        let field_name = field.name().unwrap_or_default().to_string();

        if field_name != "file" {
            continue;
        }

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unnamed".into());

        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());

        // Read file data
        let data = field
            .bytes()
            .await
            .map_err(|e| Error::InvalidInput(format!("Failed to read file: {}", e)))?;

        // Check file size
        if data.len() > config.storage.max_attachment_size {
            return Err(Error::FileTooLarge {
                max_size: config.storage.max_attachment_size,
            });
        }

        // Validate content type (optional - could restrict to certain types)
        if !is_allowed_content_type(&content_type) {
            return Err(Error::InvalidFileType(content_type));
        }

        // Calculate checksum
        let checksum = calculate_checksum(&data);

        // Generate ID and storage path
        let attachment_id = Uuid::new_v4();
        let _storage_path = format!(
            "{}/{}/{}/{}",
            config.storage.attachments_path,
            path.project_id,
            path.memory_id,
            attachment_id
        );

        // TODO: Create directory if needed
        // TODO: Write file to storage
        // TODO: Store attachment metadata in database

        let attachment = AttachmentResponse {
            id: attachment_id,
            memory_id: path.memory_id,
            filename,
            content_type,
            size: data.len() as u64,
            checksum,
            created_at: Utc::now(),
        };

        return Ok(Json(UploadResponse {
            attachment,
            message: "File uploaded successfully".into(),
        }));
    }

    Err(Error::InvalidInput("No file provided".into()))
}

/// Download an attachment.
///
/// GET /projects/:project_id/memories/:memory_id/attachments/:attachment_id
#[axum::debug_handler]
async fn download_attachment(
    State(_state): State<AppState>,
    Path(path): Path<AttachmentPath>,
) -> Result<Response> {
    let _attachment_id = path.attachment_id;
    let config = crate::config();

    // TODO: Verify memory and attachment exist and user has access
    // TODO: Fetch attachment metadata from database

    let storage_path = format!(
        "{}/{}/{}/{}",
        config.storage.attachments_path, path.project_id, path.memory_id, path.attachment_id
    );

    // Read file from storage
    let data = tokio::fs::read(&storage_path)
        .await
        .map_err(|_| Error::FileNotFound(storage_path))?;

    // TODO: Get actual content type and filename from database
    let content_type = "application/octet-stream";
    let filename = "attachment";

    let response = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// Delete an attachment.
///
/// DELETE /projects/:project_id/memories/:memory_id/attachments/:attachment_id
#[axum::debug_handler]
async fn delete_attachment(
    State(_state): State<AppState>,
    Path(path): Path<AttachmentPath>,
) -> Result<Json<serde_json::Value>> {
    let _attachment_id = path.attachment_id;
    let config = crate::config();

    // TODO: Verify memory and attachment exist and user has access

    let storage_path = format!(
        "{}/{}/{}/{}",
        config.storage.attachments_path, path.project_id, path.memory_id, path.attachment_id
    );

    // Delete file from storage
    tokio::fs::remove_file(&storage_path)
        .await
        .map_err(|_| Error::FileNotFound(storage_path))?;

    // TODO: Delete attachment metadata from database

    Ok(Json(serde_json::json!({
        "message": "Attachment deleted successfully"
    })))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if a content type is allowed for upload.
fn is_allowed_content_type(content_type: &str) -> bool {
    // Allow common document and media types
    let allowed_prefixes = [
        "text/",
        "image/",
        "audio/",
        "video/",
        "application/pdf",
        "application/json",
        "application/xml",
        "application/zip",
        "application/gzip",
        "application/x-tar",
    ];

    // Disallow potentially dangerous types
    let blocked_types = [
        "application/x-executable",
        "application/x-msdownload",
        "application/x-msdos-program",
    ];

    if blocked_types.iter().any(|t| content_type.starts_with(t)) {
        return false;
    }

    allowed_prefixes.iter().any(|p| content_type.starts_with(p))
        || content_type == "application/octet-stream"
}

/// Calculate SHA-256 checksum of data.
fn calculate_checksum(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
