//! Attachments Routes
//!
//! File attachment operations for memories.
//!
//! Storage Structure:
//! Attachments are stored using a content-hash-based path structure:
//! `{attachments_path}/{first_hex}/{second_hex}/{full_hash}.{ext}`
//!
//! For example, a file with SHA-256 hash `acf8324...` and extension `.pdf`:
//! `attachments/a/c/acf8324abcd1234....pdf`
//!
//! This provides:
//! - Deduplication (same content = same hash = same file)
//! - Even distribution across directories
//! - Predictable paths from content hash
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
use std::path::PathBuf;
use uuid::Uuid;

use crate::{db, AppState, Error, Result};

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
    State(state): State<AppState>,
    Path(path): Path<MemoryAttachmentPath>,
) -> Result<Json<ListAttachmentsResponse>> {
    let memory_id = path.memory_id.to_string();

    // Verify memory exists
    let _memory = db::get_memory(&state.db, &memory_id).await?;

    // Fetch attachments from database
    let attachments = db::list_memory_attachments(&state.db, &memory_id).await?;

    let attachment_responses: Vec<AttachmentResponse> = attachments
        .into_iter()
        .map(|a| {
            // Extract hash from storage path for checksum
            let checksum = extract_hash_from_path(&a.storage_path).unwrap_or_default();
            AttachmentResponse {
                id: a.id.parse().unwrap_or_default(),
                memory_id: path.memory_id,
                filename: a.filename,
                content_type: a.content_type,
                size: a.size_bytes as u64,
                checksum,
                created_at: a.created_at.parse().unwrap_or_else(|_| Utc::now()),
            }
        })
        .collect();

    let total = attachment_responses.len() as u32;

    Ok(Json(ListAttachmentsResponse {
        attachments: attachment_responses,
        total,
    }))
}

/// Upload an attachment to a memory.
///
/// POST /projects/:project_id/memories/:memory_id/attachments
///
/// Accepts multipart/form-data with a single file field named "file".
///
/// Storage uses content-hash-based paths for deduplication:
/// `{attachments_path}/{first_hex}/{second_hex}/{hash}.{ext}`
#[axum::debug_handler]
async fn upload_attachment(
    State(state): State<AppState>,
    Path(path): Path<MemoryAttachmentPath>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>> {
    let memory_id = path.memory_id.to_string();
    let config = crate::config();

    // Verify memory exists
    let _memory = db::get_memory(&state.db, &memory_id).await?;

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

        // Calculate content hash (SHA-256)
        let content_hash = calculate_checksum(&data);

        // Extract file extension from original filename
        let extension = get_file_extension(&filename);

        // Generate hash-based storage path: {base}/{first_hex}/{second_hex}/{hash}.{ext}
        let storage_path = generate_hash_storage_path(
            &config.storage.attachments_path,
            &content_hash,
            extension.as_deref(),
        );

        // Check if file with same hash already exists (deduplication)
        let full_path = PathBuf::from(&storage_path);
        let file_exists = full_path.exists();

        if !file_exists {
            // Create parent directories
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to create directories: {}", e)))?;
            }

            // Write file to storage
            tokio::fs::write(&full_path, &data)
                .await
                .map_err(|e| Error::Internal(format!("Failed to write file: {}", e)))?;
        }

        // Generate attachment ID
        let attachment_id = Uuid::new_v4();

        // Store attachment metadata in database
        let attachment_record = db::create_attachment(
            &state.db,
            db::CreateAttachment {
                id: attachment_id.to_string(),
                memory_id: memory_id.clone(),
                filename: filename.clone(),
                content_type: content_type.clone(),
                size_bytes: data.len() as i64,
                storage_path: storage_path.clone(),
            },
        )
        .await?;

        let attachment = AttachmentResponse {
            id: attachment_id,
            memory_id: path.memory_id,
            filename,
            content_type,
            size: data.len() as u64,
            checksum: content_hash,
            created_at: attachment_record
                .created_at
                .parse()
                .unwrap_or_else(|_| Utc::now()),
        };

        return Ok(Json(UploadResponse {
            attachment,
            message: if file_exists {
                "File uploaded successfully (deduplicated)".into()
            } else {
                "File uploaded successfully".into()
            },
        }));
    }

    Err(Error::InvalidInput("No file provided".into()))
}

/// Download an attachment.
///
/// GET /projects/:project_id/memories/:memory_id/attachments/:attachment_id
#[axum::debug_handler]
async fn download_attachment(
    State(state): State<AppState>,
    Path(path): Path<AttachmentPath>,
) -> Result<Response> {
    let attachment_id = path.attachment_id.to_string();

    // Fetch attachment metadata from database
    let attachment = db::get_attachment(&state.db, &attachment_id).await?;

    // Verify attachment belongs to the requested memory
    if attachment.memory_id != path.memory_id.to_string() {
        return Err(Error::NotFound(format!(
            "Attachment {} not found for memory {}",
            path.attachment_id, path.memory_id
        )));
    }

    // Read file from hash-based storage path
    let data = tokio::fs::read(&attachment.storage_path)
        .await
        .map_err(|_| Error::FileNotFound(attachment.storage_path.clone()))?;

    let response = Response::builder()
        .header(header::CONTENT_TYPE, &attachment.content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", attachment.filename),
        )
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// Delete an attachment.
///
/// DELETE /projects/:project_id/memories/:memory_id/attachments/:attachment_id
///
/// Note: Due to content-based deduplication, the actual file is only deleted
/// if no other attachments reference it.
#[axum::debug_handler]
async fn delete_attachment(
    State(state): State<AppState>,
    Path(path): Path<AttachmentPath>,
) -> Result<Json<serde_json::Value>> {
    let attachment_id = path.attachment_id.to_string();

    // Get attachment from database first to get storage path
    let attachment = db::get_attachment(&state.db, &attachment_id).await?;

    // Verify attachment belongs to the requested memory
    if attachment.memory_id != path.memory_id.to_string() {
        return Err(Error::NotFound(format!(
            "Attachment {} not found for memory {}",
            path.attachment_id, path.memory_id
        )));
    }

    let storage_path = attachment.storage_path.clone();

    // Delete attachment metadata from database
    db::delete_attachment(&state.db, &attachment_id).await?;

    // Check if any other attachments reference the same storage path (deduplication)
    // If not, delete the actual file
    let other_refs = db::get_attachment_by_storage_path(&state.db, &storage_path).await?;

    if other_refs.is_none() {
        // No other references, safe to delete the file
        if let Err(e) = tokio::fs::remove_file(&storage_path).await {
            // Log but don't fail if file doesn't exist (may have been cleaned up)
            tracing::warn!("Failed to delete attachment file {}: {}", storage_path, e);
        }
    }

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

/// Get file extension from filename.
fn get_file_extension(filename: &str) -> Option<String> {
    std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
}

/// Generate hash-based storage path.
///
/// Creates a path structure: `{base}/{first_hex}/{second_hex}/{hash}.{ext}`
///
/// For example, with hash "acf8324abc..." and extension "pdf":
/// - `attachments/a/c/acf8324abc....pdf`
///
/// This provides:
/// - Even distribution across 256 directories (16 * 16)
/// - Content-based deduplication (same content = same path)
/// - Easy cleanup (can check if path exists)
pub fn generate_hash_storage_path(base_path: &str, hash: &str, extension: Option<&str>) -> String {
    // Hash must be at least 2 characters (it's a SHA-256, so 64 chars)
    let first_char = hash.chars().next().unwrap_or('0').to_ascii_lowercase();
    let second_char = hash.chars().nth(1).unwrap_or('0').to_ascii_lowercase();

    let filename = match extension {
        Some(ext) if !ext.is_empty() => format!("{}.{}", hash, ext),
        _ => hash.to_string(),
    };

    format!("{}/{}/{}/{}", base_path, first_char, second_char, filename)
}

/// Extract content hash from a hash-based storage path.
///
/// Given a path like `attachments/a/c/acf8324....pdf`, extracts `acf8324...`
fn extract_hash_from_path(storage_path: &str) -> Option<String> {
    // Get the filename from the path
    let filename = std::path::Path::new(storage_path)
        .file_stem()
        .and_then(|s| s.to_str())?;

    // The filename is the hash
    Some(filename.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_hash_storage_path() {
        let hash = "acf8324abc123def456";
        let base = "attachments";

        // With extension
        let path = generate_hash_storage_path(base, hash, Some("pdf"));
        assert_eq!(path, "attachments/a/c/acf8324abc123def456.pdf");

        // Without extension
        let path = generate_hash_storage_path(base, hash, None);
        assert_eq!(path, "attachments/a/c/acf8324abc123def456");

        // Empty extension
        let path = generate_hash_storage_path(base, hash, Some(""));
        assert_eq!(path, "attachments/a/c/acf8324abc123def456");
    }

    #[test]
    fn test_generate_hash_storage_path_uppercase_hash() {
        let hash = "ACF8324ABC123DEF456";
        let base = "attachments";

        let path = generate_hash_storage_path(base, hash, Some("png"));
        // Directories should be lowercase
        assert_eq!(path, "attachments/a/c/ACF8324ABC123DEF456.png");
    }

    #[test]
    fn test_extract_hash_from_path() {
        let path = "attachments/a/c/acf8324abc123def456.pdf";
        let hash = extract_hash_from_path(path);
        assert_eq!(hash, Some("acf8324abc123def456".to_string()));

        let path_no_ext = "attachments/a/c/acf8324abc123def456";
        let hash = extract_hash_from_path(path_no_ext);
        assert_eq!(hash, Some("acf8324abc123def456".to_string()));
    }

    #[test]
    fn test_get_file_extension() {
        assert_eq!(get_file_extension("document.pdf"), Some("pdf".to_string()));
        assert_eq!(get_file_extension("image.PNG"), Some("png".to_string()));
        assert_eq!(get_file_extension("archive.tar.gz"), Some("gz".to_string()));
        assert_eq!(get_file_extension("noextension"), None);
        // Note: .hidden files are treated as having no extension (the whole name is the stem)
        assert_eq!(get_file_extension(".hidden"), None);
        assert_eq!(get_file_extension(".gitignore"), None);
    }

    #[test]
    fn test_calculate_checksum() {
        let data = b"Hello, World!";
        let hash = calculate_checksum(data);
        // SHA-256 of "Hello, World!" is well-known
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }
}
