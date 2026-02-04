//! Database operations for chunks.
//!
//! Chunks are semantic pieces of code/text extracted from source files.
//! They enable fine-grained search at the function/class/section level.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::error::Result;
use crate::models::{Chunk, ChunkCreate};

use super::DbPool;

/// Database row for chunks
#[derive(Debug, FromRow)]
struct ChunkRow {
    id: String,
    memory_id: String,
    project_id: String,
    content: String,
    content_hash: String,
    start_line: i32,
    end_line: i32,
    start_byte: i32,
    end_byte: i32,
    node_type: String,
    node_name: Option<String>,
    language: String,
    created_at: String,
    updated_at: String,
}

impl From<ChunkRow> for Chunk {
    fn from(row: ChunkRow) -> Self {
        Self {
            id: row.id,
            memory_id: row.memory_id,
            project_id: row.project_id,
            content: row.content,
            content_hash: row.content_hash,
            start_line: row.start_line,
            end_line: row.end_line,
            start_byte: row.start_byte,
            end_byte: row.end_byte,
            node_type: row.node_type,
            node_name: row.node_name,
            language: row.language,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

/// Insert a new chunk into the database.
pub async fn insert_chunk(pool: &DbPool, create: ChunkCreate) -> Result<Chunk> {
    let chunk = create.into_chunk();

    sqlx::query(
        r#"
        INSERT INTO chunks (
            id, memory_id, project_id, content, content_hash,
            start_line, end_line, start_byte, end_byte,
            node_type, node_name, language, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&chunk.id)
    .bind(&chunk.memory_id)
    .bind(&chunk.project_id)
    .bind(&chunk.content)
    .bind(&chunk.content_hash)
    .bind(chunk.start_line)
    .bind(chunk.end_line)
    .bind(chunk.start_byte)
    .bind(chunk.end_byte)
    .bind(&chunk.node_type)
    .bind(&chunk.node_name)
    .bind(&chunk.language)
    .bind(chunk.created_at.to_rfc3339())
    .bind(chunk.updated_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(chunk)
}

/// Insert multiple chunks in a batch.
pub async fn insert_chunks(pool: &DbPool, creates: Vec<ChunkCreate>) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::with_capacity(creates.len());

    for create in creates {
        let chunk = insert_chunk(pool, create).await?;
        chunks.push(chunk);
    }

    Ok(chunks)
}

/// Get a chunk by ID.
pub async fn get_chunk(pool: &DbPool, id: &str) -> Result<Option<Chunk>> {
    let row: Option<ChunkRow> = sqlx::query_as(
        r#"
        SELECT id, memory_id, project_id, content, content_hash,
               start_line, end_line, start_byte, end_byte,
               node_type, node_name, language, created_at, updated_at
        FROM chunks
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Into::into))
}

/// Get all chunks for a memory.
pub async fn get_chunks_for_memory(pool: &DbPool, memory_id: &str) -> Result<Vec<Chunk>> {
    let rows: Vec<ChunkRow> = sqlx::query_as(
        r#"
        SELECT id, memory_id, project_id, content, content_hash,
               start_line, end_line, start_byte, end_byte,
               node_type, node_name, language, created_at, updated_at
        FROM chunks
        WHERE memory_id = ?
        ORDER BY start_line
        "#,
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Get all chunks for a project.
pub async fn get_chunks_for_project(pool: &DbPool, project_id: &str) -> Result<Vec<Chunk>> {
    let rows: Vec<ChunkRow> = sqlx::query_as(
        r#"
        SELECT id, memory_id, project_id, content, content_hash,
               start_line, end_line, start_byte, end_byte,
               node_type, node_name, language, created_at, updated_at
        FROM chunks
        WHERE project_id = ?
        ORDER BY memory_id, start_line
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Get chunks by content hash (for deduplication).
pub async fn get_chunks_by_hash(pool: &DbPool, content_hash: &str) -> Result<Vec<Chunk>> {
    let rows: Vec<ChunkRow> = sqlx::query_as(
        r#"
        SELECT id, memory_id, project_id, content, content_hash,
               start_line, end_line, start_byte, end_byte,
               node_type, node_name, language, created_at, updated_at
        FROM chunks
        WHERE content_hash = ?
        "#,
    )
    .bind(content_hash)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Delete a chunk by ID.
pub async fn delete_chunk(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM chunks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Delete all chunks for a memory.
pub async fn delete_chunks_for_memory(pool: &DbPool, memory_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM chunks WHERE memory_id = ?")
        .bind(memory_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Delete all chunks for a project.
pub async fn delete_chunks_for_project(pool: &DbPool, project_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM chunks WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Count chunks for a memory.
pub async fn count_chunks_for_memory(pool: &DbPool, memory_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chunks WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;

    Ok(count)
}

/// Count chunks for a project.
pub async fn count_chunks_for_project(pool: &DbPool, project_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chunks WHERE project_id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await?;

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_pool, initialize_schema};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        // Create a test project and memory
        sqlx::query(
            "INSERT INTO projects (id, slug, name) VALUES ('proj-1', 'test-project', 'Test Project')",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memories (id, project_id, type, content) VALUES ('mem-1', 'proj-1', 'codebase', 'test content')",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_insert_and_get_chunk() {
        let pool = setup_test_db().await;

        let create = ChunkCreate {
            memory_id: "mem-1".to_string(),
            project_id: "proj-1".to_string(),
            content: "fn hello() {}".to_string(),
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            end_byte: 13,
            node_type: "function".to_string(),
            node_name: Some("hello".to_string()),
            language: "rust".to_string(),
        };

        let chunk = insert_chunk(&pool, create).await.unwrap();
        assert_eq!(chunk.node_type, "function");
        assert_eq!(chunk.node_name, Some("hello".to_string()));

        let fetched = get_chunk(&pool, &chunk.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().content, "fn hello() {}");
    }

    #[tokio::test]
    async fn test_chunks_for_memory() {
        let pool = setup_test_db().await;

        // Insert multiple chunks
        for i in 1..=3 {
            let create = ChunkCreate {
                memory_id: "mem-1".to_string(),
                project_id: "proj-1".to_string(),
                content: format!("chunk {}", i),
                start_line: i,
                end_line: i,
                start_byte: 0,
                end_byte: 10,
                node_type: "test".to_string(),
                node_name: None,
                language: "text".to_string(),
            };
            insert_chunk(&pool, create).await.unwrap();
        }

        let chunks = get_chunks_for_memory(&pool, "mem-1").await.unwrap();
        assert_eq!(chunks.len(), 3);

        let count = count_chunks_for_memory(&pool, "mem-1").await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_delete_chunks() {
        let pool = setup_test_db().await;

        let create = ChunkCreate {
            memory_id: "mem-1".to_string(),
            project_id: "proj-1".to_string(),
            content: "test".to_string(),
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            end_byte: 4,
            node_type: "test".to_string(),
            node_name: None,
            language: "text".to_string(),
        };

        let chunk = insert_chunk(&pool, create).await.unwrap();

        let deleted = delete_chunks_for_memory(&pool, "mem-1").await.unwrap();
        assert_eq!(deleted, 1);

        let fetched = get_chunk(&pool, &chunk.id).await.unwrap();
        assert!(fetched.is_none());
    }
}
