//! Memory database queries.
//!
//! Memories are the core knowledge units in Fold - they can represent
//! code files, sessions, specs, decisions, tasks, commits, PRs, and general notes.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// Types
// ============================================================================

/// Memory type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Codebase,
    Session,
    Spec,
    Decision,
    Task,
    General,
    Commit,
    Pr,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Codebase => "codebase",
            Self::Session => "session",
            Self::Spec => "spec",
            Self::Decision => "decision",
            Self::Task => "task",
            Self::General => "general",
            Self::Commit => "commit",
            Self::Pr => "pr",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "codebase" => Some(Self::Codebase),
            "session" => Some(Self::Session),
            "spec" => Some(Self::Spec),
            "decision" => Some(Self::Decision),
            "task" => Some(Self::Task),
            "general" => Some(Self::General),
            "commit" => Some(Self::Commit),
            "pr" => Some(Self::Pr),
            _ => None,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Codebase,
            Self::Session,
            Self::Spec,
            Self::Decision,
            Self::Task,
            Self::General,
            Self::Commit,
            Self::Pr,
        ]
    }
}

/// Metadata sync source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncSource {
    Fold,
    GitHub,
    GitLab,
}

impl SyncSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fold => "fold",
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
        }
    }
}

/// Memory record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub project_id: String,

    #[sqlx(rename = "type")]
    pub memory_type: String,
    /// Source of the memory: 'agent', 'file', 'git'
    pub source: Option<String>,
    pub title: Option<String>,
    /// Content is stored externally (filesystem or source file).
    /// This field is nullable in the database.
    pub content: Option<String>,
    pub content_hash: Option<String>,
    /// Where content is stored: 'filesystem' or 'source_file'
    pub content_storage: Option<String>,

    // Source info (for codebase type)
    pub file_path: Option<String>,
    pub language: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    #[sqlx(default)]
    pub line_start: Option<i32>,
    #[sqlx(default)]
    pub line_end: Option<i32>,

    // For commit type
    pub summary_file_path: Option<String>,

    // Metadata repo sync status
    pub metadata_repo_synced_at: Option<String>,
    pub metadata_repo_commit_sha: Option<String>,
    pub metadata_repo_file_path: Option<String>,
    pub synced_from: Option<String>,

    // Metadata
    pub author: Option<String>,
    pub keywords: Option<String>, // JSON array
    pub tags: Option<String>,     // JSON array
    #[sqlx(default)]
    pub context: Option<String>,
    #[sqlx(default)]
    pub status: Option<String>,
    #[sqlx(default)]
    pub assignee: Option<String>,
    #[sqlx(default)]
    pub metadata: Option<String>,

    // Usage tracking
    #[sqlx(default)]
    pub retrieval_count: Option<i32>,
    #[sqlx(default)]
    pub last_accessed: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

impl Memory {
    /// Get the memory type as enum.
    pub fn memory_type_enum(&self) -> Option<MemoryType> {
        MemoryType::from_str(&self.memory_type)
    }

    /// Parse keywords JSON into a vector.
    pub fn keywords_vec(&self) -> Vec<String> {
        self.keywords
            .as_ref()
            .and_then(|k| serde_json::from_str(k).ok())
            .unwrap_or_default()
    }

    /// Parse tags JSON into a vector.
    pub fn tags_vec(&self) -> Vec<String> {
        self.tags
            .as_ref()
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default()
    }
}

/// Input for creating a new memory.
#[derive(Debug, Clone)]
pub struct CreateMemory {
    pub id: String,
    pub project_id: String,
    pub memory_type: MemoryType,
    /// Source of the memory: agent, file, git
    pub source: Option<crate::models::MemorySource>,
    pub title: Option<String>,
    /// Content is stored externally, not in SQLite.
    /// This is kept for backwards compatibility but should be None.
    pub content: Option<String>,
    pub content_hash: Option<String>,
    /// Where content is stored: 'filesystem' or 'source_file'
    pub content_storage: String,
    pub file_path: Option<String>,
    pub language: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    pub author: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Input for updating a memory.
#[derive(Debug, Clone, Default)]
pub struct UpdateMemory {
    pub title: Option<String>,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    pub author: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

/// Filter options for listing memories.
#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub project_id: Option<String>,
    /// Filter by multiple project IDs (for cross-project queries)
    pub project_ids: Option<Vec<String>>,
    pub memory_type: Option<MemoryType>,
    pub memory_types: Option<Vec<MemoryType>>,
    /// Filter by source (agent, file, git)
    pub source: Option<crate::models::MemorySource>,
    pub author: Option<String>,
    pub file_path_prefix: Option<String>,
    pub tag: Option<String>,
    pub search_query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Update for metadata repo sync status.
#[derive(Debug, Clone)]
pub struct MetadataSyncUpdate {
    pub synced_at: String,
    pub commit_sha: String,
    pub file_path: String,
    pub source: SyncSource,
}

// ============================================================================
// Queries
// ============================================================================

/// Create a new memory.
pub async fn create_memory(pool: &DbPool, input: CreateMemory) -> Result<Memory> {
    let keywords_json = input
        .keywords
        .map(|k| serde_json::to_string(&k).unwrap_or_default());
    let tags_json = input
        .tags
        .map(|t| serde_json::to_string(&t).unwrap_or_default());
    let source_str = input.source.map(|s| s.as_str().to_string());

    sqlx::query_as::<_, Memory>(
        r#"
        INSERT INTO memories (
            id, project_id, type, source, title, content, content_hash, content_storage,
            file_path, language, git_branch, git_commit_sha, author, keywords, tags
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(input.memory_type.as_str())
    .bind(&source_str)
    .bind(&input.title)
    .bind(&input.content)
    .bind(&input.content_hash)
    .bind(&input.content_storage)
    .bind(&input.file_path)
    .bind(&input.language)
    .bind(&input.git_branch)
    .bind(&input.git_commit_sha)
    .bind(&input.author)
    .bind(&keywords_json)
    .bind(&tags_json)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("Memory already exists: {}", input.id))
        }
        _ => Error::Database(e),
    })
}

/// Get a memory by ID.
pub async fn get_memory(pool: &DbPool, id: &str) -> Result<Memory> {
    sqlx::query_as::<_, Memory>("SELECT * FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Memory not found: {}", id)))
}

/// Get a memory by ID (optional).
pub async fn get_memory_optional(pool: &DbPool, id: &str) -> Result<Option<Memory>> {
    sqlx::query_as::<_, Memory>("SELECT * FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get a memory by content hash.
/// Uses idx_memories_content_hash index.
pub async fn get_memory_by_hash(
    pool: &DbPool,
    project_id: &str,
    content_hash: &str,
) -> Result<Option<Memory>> {
    sqlx::query_as::<_, Memory>(
        r#"
        SELECT * FROM memories
        WHERE project_id = ? AND content_hash = ?
        "#,
    )
    .bind(project_id)
    .bind(content_hash)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Get a codebase memory by file path.
/// Uses idx_memories_file index.
pub async fn get_memory_by_file_path(
    pool: &DbPool,
    project_id: &str,
    file_path: &str,
) -> Result<Option<Memory>> {
    sqlx::query_as::<_, Memory>(
        r#"
        SELECT * FROM memories
        WHERE project_id = ? AND file_path = ?
        "#,
    )
    .bind(project_id)
    .bind(file_path)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Update a memory.
pub async fn update_memory(pool: &DbPool, id: &str, input: UpdateMemory) -> Result<Memory> {
    let mut updates = Vec::new();
    let mut bindings: Vec<Option<String>> = Vec::new();

    if let Some(title) = input.title {
        updates.push("title = ?");
        bindings.push(Some(title));
    }
    if let Some(content) = input.content {
        updates.push("content = ?");
        bindings.push(Some(content));
    }
    if let Some(hash) = input.content_hash {
        updates.push("content_hash = ?");
        bindings.push(Some(hash));
    }
    if let Some(branch) = input.git_branch {
        updates.push("git_branch = ?");
        bindings.push(Some(branch));
    }
    if let Some(sha) = input.git_commit_sha {
        updates.push("git_commit_sha = ?");
        bindings.push(Some(sha));
    }
    if let Some(author) = input.author {
        updates.push("author = ?");
        bindings.push(Some(author));
    }
    if let Some(keywords) = input.keywords {
        updates.push("keywords = ?");
        bindings.push(Some(serde_json::to_string(&keywords)?));
    }
    if let Some(tags) = input.tags {
        updates.push("tags = ?");
        bindings.push(Some(serde_json::to_string(&tags)?));
    }

    if updates.is_empty() {
        return get_memory(pool, id).await;
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE memories SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, Memory>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Memory not found: {}", id)))
}

/// Update memory content and hash.
pub async fn update_memory_content(
    pool: &DbPool,
    id: &str,
    content: &str,
    content_hash: &str,
) -> Result<Memory> {
    sqlx::query_as::<_, Memory>(
        r#"
        UPDATE memories SET
            content = ?,
            content_hash = ?,
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(content)
    .bind(content_hash)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Memory not found: {}", id)))
}

/// Update metadata repo sync status.
pub async fn update_memory_sync_status(
    pool: &DbPool,
    id: &str,
    update: MetadataSyncUpdate,
) -> Result<Memory> {
    sqlx::query_as::<_, Memory>(
        r#"
        UPDATE memories SET
            metadata_repo_synced_at = ?,
            metadata_repo_commit_sha = ?,
            metadata_repo_file_path = ?,
            synced_from = ?,
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(&update.synced_at)
    .bind(&update.commit_sha)
    .bind(&update.file_path)
    .bind(update.source.as_str())
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Memory not found: {}", id)))
}

/// Delete a memory.
pub async fn delete_memory(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Memory not found: {}", id)));
    }

    Ok(())
}

/// Delete memories by project.
pub async fn delete_memories_by_project(pool: &DbPool, project_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM memories WHERE project_id = ?")
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// List memories with filters.
/// Uses various indexes based on filter parameters.
pub async fn list_memories(pool: &DbPool, filter: MemoryFilter) -> Result<Vec<Memory>> {
    let mut conditions: Vec<String> = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(project_id) = &filter.project_id {
        conditions.push("project_id = ?".to_string());
        bindings.push(project_id.clone());
    }

    // Handle multiple project IDs (for cross-project queries)
    if let Some(project_ids) = &filter.project_ids {
        if !project_ids.is_empty() {
            let placeholders: Vec<&str> = project_ids.iter().map(|_| "?").collect();
            conditions.push(format!("project_id IN ({})", placeholders.join(", ")));
            for id in project_ids {
                bindings.push(id.clone());
            }
        }
    }

    if let Some(memory_type) = &filter.memory_type {
        conditions.push("type = ?".to_string());
        bindings.push(memory_type.as_str().to_string());
    }

    if let Some(types) = &filter.memory_types {
        if !types.is_empty() {
            let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
            conditions.push(format!("type IN ({})", placeholders.join(", ")));
            for t in types {
                bindings.push(t.as_str().to_string());
            }
        }
    }

    if let Some(source) = &filter.source {
        conditions.push("source = ?".to_string());
        bindings.push(source.as_str().to_string());
    }

    if let Some(author) = &filter.author {
        conditions.push("author = ?".to_string());
        bindings.push(author.clone());
    }

    if let Some(prefix) = &filter.file_path_prefix {
        conditions.push("file_path LIKE ?".to_string());
        bindings.push(format!("{}%", prefix));
    }

    if let Some(tag) = &filter.tag {
        // JSON contains check for SQLite
        conditions.push("tags LIKE ?".to_string());
        bindings.push(format!("%\"{}%", tag));
    }

    if let Some(search) = &filter.search_query {
        conditions.push("(title LIKE ? OR content LIKE ?)".to_string());
        let pattern = format!("%{}%", search);
        bindings.push(pattern.clone());
        bindings.push(pattern);
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);

    let query = format!(
        r#"
        SELECT * FROM memories
        {}
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        "#,
        where_clause
    );

    let mut q = sqlx::query_as::<_, Memory>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(limit).bind(offset);

    q.fetch_all(pool).await.map_err(Error::Database)
}

/// List memories for a project.
/// Uses idx_memories_project index.
pub async fn list_project_memories(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>> {
    sqlx::query_as::<_, Memory>(
        r#"
        SELECT * FROM memories
        WHERE project_id = ?
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List memories by type for a project.
/// Uses idx_memories_type index (project_id, type).
pub async fn list_project_memories_by_type(
    pool: &DbPool,
    project_id: &str,
    memory_type: MemoryType,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>> {
    sqlx::query_as::<_, Memory>(
        r#"
        SELECT * FROM memories
        WHERE project_id = ? AND type = ?
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(project_id)
    .bind(memory_type.as_str())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List codebase memories for a project.
/// Uses idx_memories_file index (project_id, file_path).
pub async fn list_project_files(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>> {
    sqlx::query_as::<_, Memory>(
        r#"
        SELECT * FROM memories
        WHERE project_id = ? AND type = 'codebase'
        ORDER BY file_path ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Count memories for a project.
pub async fn count_project_memories(pool: &DbPool, project_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories WHERE project_id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Count memories by type for a project.
pub async fn count_project_memories_by_type(
    pool: &DbPool,
    project_id: &str,
    memory_type: MemoryType,
) -> Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memories WHERE project_id = ? AND type = ?")
            .bind(project_id)
            .bind(memory_type.as_str())
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Count memories by source for a project.
pub async fn count_project_memories_by_source(
    pool: &DbPool,
    project_id: &str,
    source: &str,
) -> Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memories WHERE project_id = ? AND source = ?")
            .bind(project_id)
            .bind(source)
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Get memories that need metadata sync (changed since last sync).
pub async fn list_memories_needing_sync(pool: &DbPool, project_id: &str) -> Result<Vec<Memory>> {
    sqlx::query_as::<_, Memory>(
        r#"
        SELECT * FROM memories
        WHERE project_id = ?
        AND (
            metadata_repo_synced_at IS NULL
            OR updated_at > metadata_repo_synced_at
        )
        ORDER BY updated_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Upsert a memory (insert or update if exists).
/// Useful for codebase indexing where we update existing files.
pub async fn upsert_memory(pool: &DbPool, input: CreateMemory) -> Result<Memory> {
    let keywords_json = input
        .keywords
        .map(|k| serde_json::to_string(&k).unwrap_or_default());
    let tags_json = input
        .tags
        .map(|t| serde_json::to_string(&t).unwrap_or_default());

    sqlx::query_as::<_, Memory>(
        r#"
        INSERT INTO memories (
            id, project_id, type, title, content, content_hash, content_storage,
            file_path, language, git_branch, git_commit_sha, author, keywords, tags
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            content = excluded.content,
            content_hash = excluded.content_hash,
            content_storage = excluded.content_storage,
            git_branch = excluded.git_branch,
            git_commit_sha = excluded.git_commit_sha,
            keywords = excluded.keywords,
            tags = excluded.tags,
            updated_at = datetime('now')
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(input.memory_type.as_str())
    .bind(&input.title)
    .bind(&input.content)
    .bind(&input.content_hash)
    .bind(&input.content_storage)
    .bind(&input.file_path)
    .bind(&input.language)
    .bind(&input.git_branch)
    .bind(&input.git_commit_sha)
    .bind(&input.author)
    .bind(&keywords_json)
    .bind(&tags_json)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Batch get memories by IDs.
pub async fn get_memories_by_ids(pool: &DbPool, ids: &[String]) -> Result<Vec<Memory>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
    let query = format!(
        "SELECT * FROM memories WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut q = sqlx::query_as::<_, Memory>(&query);
    for id in ids {
        q = q.bind(id);
    }

    q.fetch_all(pool).await.map_err(Error::Database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_project, init_pool, migrate, CreateProject};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        // Create test project
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

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_memory() {
        let pool = setup_test_db().await;

        let memory = create_memory(
            &pool,
            CreateMemory {
                id: "mem-1".to_string(),
                project_id: "proj-1".to_string(),
                memory_type: MemoryType::General,
                source: None,
                title: Some("Test Memory".to_string()),
                content: None, // Content stored externally
                content_hash: Some("abc123".to_string()),
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: Some("test".to_string()),
                keywords: Some(vec!["test".to_string(), "memory".to_string()]),
                tags: Some(vec!["important".to_string()]),
            },
        )
        .await
        .unwrap();

        assert_eq!(memory.id, "mem-1");
        assert_eq!(memory.memory_type, "general");
        assert_eq!(memory.content_storage, Some("filesystem".to_string()));

        let fetched = get_memory(&pool, "mem-1").await.unwrap();
        assert!(fetched.content.is_none()); // Content stored externally
        assert_eq!(fetched.keywords_vec(), vec!["test", "memory"]);
    }

    #[tokio::test]
    async fn test_list_memories_by_type() {
        let pool = setup_test_db().await;

        // Create different types of memories
        for (i, mem_type) in [MemoryType::Decision, MemoryType::Task, MemoryType::Decision]
            .iter()
            .enumerate()
        {
            create_memory(
                &pool,
                CreateMemory {
                    id: format!("mem-{}", i),
                    project_id: "proj-1".to_string(),
                    memory_type: *mem_type,
                    source: None,
                    title: Some(format!("Memory {}", i)),
                    content: None,
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
        }

        let decisions = list_project_memories_by_type(&pool, "proj-1", MemoryType::Decision, 10, 0)
            .await
            .unwrap();
        assert_eq!(decisions.len(), 2);

        let tasks = list_project_memories_by_type(&pool, "proj-1", MemoryType::Task, 10, 0)
            .await
            .unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[tokio::test]
    async fn test_upsert_memory() {
        let pool = setup_test_db().await;

        let input = CreateMemory {
            id: "mem-1".to_string(),
            project_id: "proj-1".to_string(),
            memory_type: MemoryType::Codebase,
            source: Some(crate::models::MemorySource::File),
            title: Some("File".to_string()),
            content: None, // Content in source file
            content_hash: Some("hash1".to_string()),
            content_storage: "source_file".to_string(),
            file_path: Some("src/main.rs".to_string()),
            language: Some("rust".to_string()),
            git_branch: None,
            git_commit_sha: None,
            author: None,
            keywords: None,
            tags: None,
        };

        // First insert
        let mem1 = upsert_memory(&pool, input.clone()).await.unwrap();
        assert!(mem1.content.is_none());
        assert_eq!(mem1.content_storage, Some("source_file".to_string()));

        // Update via upsert
        let mut updated_input = input;
        updated_input.content_hash = Some("hash2".to_string());

        let mem2 = upsert_memory(&pool, updated_input).await.unwrap();
        assert!(mem2.content.is_none()); // Content still stored externally
        assert_eq!(mem2.content_hash, Some("hash2".to_string()));
    }
}
