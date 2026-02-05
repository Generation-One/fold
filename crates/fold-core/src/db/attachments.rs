//! Attachment database queries.
//!
//! Attachments are files associated with memories (images, PDFs, etc.).

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// Types
// ============================================================================

/// Attachment record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub memory_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub created_at: String,
}

impl Attachment {
    /// Get file extension from filename.
    pub fn extension(&self) -> Option<&str> {
        self.filename.rsplit('.').next()
    }

    /// Check if attachment is an image.
    pub fn is_image(&self) -> bool {
        self.content_type.starts_with("image/")
    }

    /// Check if attachment is a PDF.
    pub fn is_pdf(&self) -> bool {
        self.content_type == "application/pdf"
    }

    /// Get human-readable file size.
    pub fn human_size(&self) -> String {
        const KB: i64 = 1024;
        const MB: i64 = KB * 1024;
        const GB: i64 = MB * 1024;

        if self.size_bytes >= GB {
            format!("{:.2} GB", self.size_bytes as f64 / GB as f64)
        } else if self.size_bytes >= MB {
            format!("{:.2} MB", self.size_bytes as f64 / MB as f64)
        } else if self.size_bytes >= KB {
            format!("{:.2} KB", self.size_bytes as f64 / KB as f64)
        } else {
            format!("{} bytes", self.size_bytes)
        }
    }
}

/// Input for creating a new attachment.
#[derive(Debug, Clone)]
pub struct CreateAttachment {
    pub id: String,
    pub memory_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
}

/// Filter options for listing attachments.
#[derive(Debug, Clone, Default)]
pub struct AttachmentFilter {
    pub memory_id: Option<String>,
    pub content_type_prefix: Option<String>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ============================================================================
// Queries
// ============================================================================

/// Create a new attachment.
pub async fn create_attachment(pool: &DbPool, input: CreateAttachment) -> Result<Attachment> {
    sqlx::query_as::<_, Attachment>(
        r#"
        INSERT INTO attachments (id, memory_id, filename, content_type, size_bytes, storage_path)
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.memory_id)
    .bind(&input.filename)
    .bind(&input.content_type)
    .bind(input.size_bytes)
    .bind(&input.storage_path)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get an attachment by ID.
pub async fn get_attachment(pool: &DbPool, id: &str) -> Result<Attachment> {
    sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Attachment not found: {}", id)))
}

/// Get an attachment by ID (optional).
pub async fn get_attachment_optional(pool: &DbPool, id: &str) -> Result<Option<Attachment>> {
    sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Delete an attachment by ID.
/// Note: This only deletes the database record, not the file.
/// File deletion should be handled by the caller.
pub async fn delete_attachment(pool: &DbPool, id: &str) -> Result<Attachment> {
    sqlx::query_as::<_, Attachment>("DELETE FROM attachments WHERE id = ? RETURNING *")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Attachment not found: {}", id)))
}

/// Delete all attachments for a memory.
/// Returns the deleted attachments for file cleanup.
pub async fn delete_memory_attachments(pool: &DbPool, memory_id: &str) -> Result<Vec<Attachment>> {
    // First get all attachments to return for cleanup
    let attachments = list_memory_attachments(pool, memory_id).await?;

    // Then delete them
    sqlx::query("DELETE FROM attachments WHERE memory_id = ?")
        .bind(memory_id)
        .execute(pool)
        .await?;

    Ok(attachments)
}

/// List attachments for a memory.
/// Uses idx_attachments_memory index.
pub async fn list_memory_attachments(pool: &DbPool, memory_id: &str) -> Result<Vec<Attachment>> {
    sqlx::query_as::<_, Attachment>(
        r#"
        SELECT * FROM attachments
        WHERE memory_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List attachments with filters.
pub async fn list_attachments(pool: &DbPool, filter: AttachmentFilter) -> Result<Vec<Attachment>> {
    let mut conditions = Vec::new();
    let mut bindings: Vec<String> = Vec::new();
    let _int_bindings: Vec<i64> = Vec::new();

    if let Some(memory_id) = &filter.memory_id {
        conditions.push("memory_id = ?");
        bindings.push(memory_id.clone());
    }

    if let Some(prefix) = &filter.content_type_prefix {
        conditions.push("content_type LIKE ?");
        bindings.push(format!("{}%", prefix));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Add size filters separately since they're i64
    let mut size_conditions = Vec::new();
    if let Some(min) = filter.min_size {
        size_conditions.push(format!("size_bytes >= {}", min));
    }
    if let Some(max) = filter.max_size {
        size_conditions.push(format!("size_bytes <= {}", max));
    }

    let full_where = if where_clause.is_empty() && !size_conditions.is_empty() {
        format!("WHERE {}", size_conditions.join(" AND "))
    } else if !size_conditions.is_empty() {
        format!("{} AND {}", where_clause, size_conditions.join(" AND "))
    } else {
        where_clause
    };

    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);

    let query = format!(
        r#"
        SELECT * FROM attachments
        {}
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        "#,
        full_where
    );

    let mut q = sqlx::query_as::<_, Attachment>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(limit).bind(offset);

    q.fetch_all(pool).await.map_err(Error::Database)
}

/// Count attachments for a memory.
pub async fn count_memory_attachments(pool: &DbPool, memory_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM attachments WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Get total size of attachments for a memory.
pub async fn get_memory_attachments_size(pool: &DbPool, memory_id: &str) -> Result<i64> {
    let (size,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(size_bytes), 0) FROM attachments WHERE memory_id = ?")
            .bind(memory_id)
            .fetch_one(pool)
            .await?;
    Ok(size)
}

/// Get total storage used by all attachments.
pub async fn get_total_storage_used(pool: &DbPool) -> Result<i64> {
    let (size,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(size_bytes), 0) FROM attachments")
        .fetch_one(pool)
        .await?;
    Ok(size)
}

/// Count attachments for a project (via memories).
pub async fn count_project_attachments(pool: &DbPool, project_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM attachments a
        JOIN memories m ON a.memory_id = m.id
        WHERE m.project_id = ?
        "#,
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Find attachments by filename pattern.
pub async fn find_attachments_by_filename(
    pool: &DbPool,
    pattern: &str,
    limit: i64,
) -> Result<Vec<Attachment>> {
    sqlx::query_as::<_, Attachment>(
        r#"
        SELECT * FROM attachments
        WHERE filename LIKE ?
        ORDER BY created_at DESC
        LIMIT ?
        "#,
    )
    .bind(format!("%{}%", pattern))
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Get attachments by storage path (for orphan cleanup).
pub async fn get_attachment_by_storage_path(
    pool: &DbPool,
    storage_path: &str,
) -> Result<Option<Attachment>> {
    sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE storage_path = ?")
        .bind(storage_path)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// List all unique storage paths (for cleanup operations).
pub async fn list_all_storage_paths(pool: &DbPool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT storage_path FROM attachments")
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(|(path,)| path).collect())
}

/// Batch get attachments by IDs.
pub async fn get_attachments_by_ids(pool: &DbPool, ids: &[String]) -> Result<Vec<Attachment>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
    let query = format!(
        "SELECT * FROM attachments WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut q = sqlx::query_as::<_, Attachment>(&query);
    for id in ids {
        q = q.bind(id);
    }

    q.fetch_all(pool).await.map_err(Error::Database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        create_memory, create_project, init_pool, migrate, CreateMemory, CreateProject, MemoryType,
    };

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        create_project(
            &pool,
            CreateProject {
                id: "proj-1".to_string(),
                slug: "test".to_string(),
                name: "Test".to_string(),
                description: None,
            },
        )
        .await
        .unwrap();

        create_memory(
            &pool,
            CreateMemory {
                id: "mem-1".to_string(),
                project_id: "proj-1".to_string(),
                memory_type: MemoryType::General,
                source: None,
                title: Some("Test".to_string()),
                content: Some("Content".to_string()),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            },
        )
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_attachment() {
        let pool = setup_test_db().await;

        let attachment = create_attachment(
            &pool,
            CreateAttachment {
                id: "att-1".to_string(),
                memory_id: "mem-1".to_string(),
                filename: "document.pdf".to_string(),
                content_type: "application/pdf".to_string(),
                size_bytes: 1024 * 1024,
                storage_path: "/data/attachments/att-1.pdf".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(attachment.id, "att-1");
        assert!(attachment.is_pdf());
        assert_eq!(attachment.extension(), Some("pdf"));
        assert_eq!(attachment.human_size(), "1.00 MB");

        let fetched = get_attachment(&pool, "att-1").await.unwrap();
        assert_eq!(fetched.filename, "document.pdf");
    }

    #[tokio::test]
    async fn test_list_memory_attachments() {
        let pool = setup_test_db().await;

        for i in 1..=3 {
            create_attachment(
                &pool,
                CreateAttachment {
                    id: format!("att-{}", i),
                    memory_id: "mem-1".to_string(),
                    filename: format!("file-{}.txt", i),
                    content_type: "text/plain".to_string(),
                    size_bytes: 100 * i,
                    storage_path: format!("/data/att-{}.txt", i),
                },
            )
            .await
            .unwrap();
        }

        let attachments = list_memory_attachments(&pool, "mem-1").await.unwrap();
        assert_eq!(attachments.len(), 3);

        let count = count_memory_attachments(&pool, "mem-1").await.unwrap();
        assert_eq!(count, 3);

        let total_size = get_memory_attachments_size(&pool, "mem-1").await.unwrap();
        assert_eq!(total_size, 600); // 100 + 200 + 300
    }

    #[tokio::test]
    async fn test_delete_memory_attachments() {
        let pool = setup_test_db().await;

        create_attachment(
            &pool,
            CreateAttachment {
                id: "att-1".to_string(),
                memory_id: "mem-1".to_string(),
                filename: "file.txt".to_string(),
                content_type: "text/plain".to_string(),
                size_bytes: 100,
                storage_path: "/data/att-1.txt".to_string(),
            },
        )
        .await
        .unwrap();

        let deleted = delete_memory_attachments(&pool, "mem-1").await.unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].storage_path, "/data/att-1.txt");

        let count = count_memory_attachments(&pool, "mem-1").await.unwrap();
        assert_eq!(count, 0);
    }
}
