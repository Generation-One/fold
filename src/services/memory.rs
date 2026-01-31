//! Memory service for high-level memory operations.
//!
//! Orchestrates embedding generation, vector storage, and SQLite storage
//! to provide a unified API for memory operations.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::config::config;
use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{Memory, MemoryCreate, MemorySearchResult, MemoryType, MemoryUpdate};

use super::qdrant::{QdrantService, SearchFilter, VectorSearchResult};
use super::EmbeddingService;
use super::LlmService;

/// Service for managing memories with embedding and vector storage.
#[derive(Clone)]
pub struct MemoryService {
    db: DbPool,
    qdrant: Arc<QdrantService>,
    embeddings: Arc<EmbeddingService>,
    llm: Arc<LlmService>,
}

impl MemoryService {
    /// Create a new memory service.
    pub fn new(
        db: DbPool,
        qdrant: Arc<QdrantService>,
        embeddings: Arc<EmbeddingService>,
        llm: Arc<LlmService>,
    ) -> Self {
        Self {
            db,
            qdrant,
            embeddings,
            llm,
        }
    }

    /// Add a memory with automatic embedding and metadata generation.
    pub async fn add(
        &self,
        project_id: &str,
        project_slug: &str,
        data: MemoryCreate,
        auto_metadata: bool,
    ) -> Result<Memory> {
        // Generate metadata if requested and not provided
        let (title, keywords, tags, context) = if auto_metadata
            && (data.keywords.is_empty() || data.tags.is_empty())
        {
            if self.llm.is_available() {
                match self
                    .llm
                    .generate_metadata(&data.content, data.memory_type.as_str())
                    .await
                {
                    Ok(meta) => (
                        data.title.or(Some(meta.title).filter(|s| !s.is_empty())),
                        if data.keywords.is_empty() {
                            meta.keywords
                        } else {
                            data.keywords.clone()
                        },
                        if data.tags.is_empty() { meta.tags } else { data.tags.clone() },
                        data.context.or(Some(meta.context).filter(|s| !s.is_empty())),
                    ),
                    Err(e) => {
                        warn!(error = %e, "Failed to generate metadata, using provided values");
                        (data.title.clone(), data.keywords.clone(), data.tags.clone(), data.context.clone())
                    }
                }
            } else {
                (data.title.clone(), data.keywords.clone(), data.tags.clone(), data.context.clone())
            }
        } else {
            (data.title.clone(), data.keywords.clone(), data.tags.clone(), data.context.clone())
        };

        // Create memory object
        let now = Utc::now();
        let memory = Memory {
            id: crate::models::new_id(),
            project_id: project_id.to_string(),
            memory_type: data.memory_type.as_str().to_string(),
            content: data.content.clone(),
            title,
            author: data.author.clone(),
            keywords: if keywords.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&keywords).unwrap())
            },
            tags: if tags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&tags).unwrap())
            },
            context,
            file_path: data.file_path.clone(),
            language: data.language.clone(),
            line_start: None,
            line_end: None,
            status: data.status.clone(),
            assignee: data.assignee.clone(),
            metadata: if data.metadata.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&data.metadata).unwrap())
            },
            created_at: now,
            updated_at: now,
            retrieval_count: 0,
            last_accessed: None,
        };

        // Store in SQLite
        self.insert_memory(&memory).await?;

        // Generate embedding and store in Qdrant
        let search_text = memory.to_search_text();
        let embedding = self.embeddings.embed_single(&search_text).await?;

        // Ensure collection exists
        self.qdrant
            .create_collection(project_slug, self.embeddings.dimension())
            .await?;

        // Build payload
        let mut payload: HashMap<String, Value> = HashMap::new();
        payload.insert("memory_id".to_string(), json!(memory.id));
        payload.insert("project_id".to_string(), json!(memory.project_id));
        payload.insert("type".to_string(), json!(memory.memory_type));
        if let Some(ref t) = memory.title {
            payload.insert("title".to_string(), json!(t));
        }
        if let Some(ref a) = memory.author {
            payload.insert("author".to_string(), json!(a));
        }
        if let Some(ref fp) = memory.file_path {
            payload.insert("file_path".to_string(), json!(fp));
        }
        payload.insert("created_at".to_string(), json!(memory.created_at.to_rfc3339()));

        self.qdrant
            .upsert(project_slug, &memory.id, embedding.clone(), payload)
            .await?;

        // Auto-link to similar memories using vector similarity
        self.auto_link_similar(&memory.id, project_id, project_slug, &embedding)
            .await;

        info!(id = %memory.id, memory_type = %memory.memory_type, "Added memory");

        Ok(memory)
    }

    /// Get a memory by ID.
    pub async fn get(&self, project_id: &str, memory_id: &str) -> Result<Option<Memory>> {
        let memory = sqlx::query_as::<_, Memory>(
            r#"
            SELECT * FROM memories
            WHERE id = ? AND project_id = ?
            "#,
        )
        .bind(memory_id)
        .bind(project_id)
        .fetch_optional(&self.db)
        .await?;

        // Update access tracking
        if memory.is_some() {
            let _ = sqlx::query(
                r#"
                UPDATE memories
                SET retrieval_count = retrieval_count + 1,
                    last_accessed = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(memory_id)
            .execute(&self.db)
            .await;
        }

        Ok(memory)
    }

    /// List memories with optional filters.
    pub async fn list(
        &self,
        project_id: &str,
        memory_type: Option<MemoryType>,
        author: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Memory>> {
        let mut query = String::from(
            r#"
            SELECT * FROM memories
            WHERE project_id = ?
            "#,
        );

        if memory_type.is_some() {
            query.push_str(" AND type = ?");
        }
        if author.is_some() {
            query.push_str(" AND author = ?");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query_as::<_, Memory>(&query).bind(project_id);

        if let Some(mt) = memory_type {
            q = q.bind(mt.as_str());
        }
        if let Some(a) = author {
            q = q.bind(a);
        }

        q = q.bind(limit).bind(offset);

        let memories = q.fetch_all(&self.db).await?;

        Ok(memories)
    }

    /// Update a memory.
    pub async fn update(
        &self,
        project_id: &str,
        project_slug: &str,
        memory_id: &str,
        update: MemoryUpdate,
    ) -> Result<Memory> {
        // Get existing memory
        let existing = self
            .get(project_id, memory_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Memory {}", memory_id)))?;

        // Build update query
        let now = Utc::now();
        let content = update.content.unwrap_or(existing.content);
        let title = update.title.or(existing.title);
        let keywords = update
            .keywords
            .map(|k| serde_json::to_string(&k).unwrap())
            .or(existing.keywords);
        let tags = update
            .tags
            .map(|t| serde_json::to_string(&t).unwrap())
            .or(existing.tags);
        let context = update.context.or(existing.context);
        let status = update.status.or(existing.status);
        let assignee = update.assignee.or(existing.assignee);
        let metadata = update
            .metadata
            .map(|m| serde_json::to_string(&m).unwrap())
            .or(existing.metadata);

        sqlx::query(
            r#"
            UPDATE memories
            SET content = ?,
                title = ?,
                keywords = ?,
                tags = ?,
                context = ?,
                status = ?,
                assignee = ?,
                metadata = ?,
                updated_at = ?
            WHERE id = ? AND project_id = ?
            "#,
        )
        .bind(&content)
        .bind(&title)
        .bind(&keywords)
        .bind(&tags)
        .bind(&context)
        .bind(&status)
        .bind(&assignee)
        .bind(&metadata)
        .bind(now)
        .bind(memory_id)
        .bind(project_id)
        .execute(&self.db)
        .await?;

        // Re-embed if content changed
        let updated = Memory {
            content: content.clone(),
            title: title.clone(),
            keywords: keywords.clone(),
            tags: tags.clone(),
            context: context.clone(),
            updated_at: now,
            status,
            assignee,
            metadata,
            ..existing
        };

        let search_text = updated.to_search_text();
        let embedding = self.embeddings.embed_single(&search_text).await?;

        let mut payload: HashMap<String, Value> = HashMap::new();
        payload.insert("memory_id".to_string(), json!(updated.id));
        payload.insert("project_id".to_string(), json!(updated.project_id));
        payload.insert("type".to_string(), json!(updated.memory_type));
        if let Some(ref t) = updated.title {
            payload.insert("title".to_string(), json!(t));
        }
        if let Some(ref a) = updated.author {
            payload.insert("author".to_string(), json!(a));
        }
        if let Some(ref fp) = updated.file_path {
            payload.insert("file_path".to_string(), json!(fp));
        }
        payload.insert("created_at".to_string(), json!(updated.created_at.to_rfc3339()));

        self.qdrant
            .upsert(project_slug, &updated.id, embedding, payload)
            .await?;

        debug!(id = %memory_id, "Updated memory");

        Ok(updated)
    }

    /// Delete a memory.
    pub async fn delete(&self, project_id: &str, project_slug: &str, memory_id: &str) -> Result<()> {
        // Delete from SQLite
        let result = sqlx::query(
            r#"
            DELETE FROM memories
            WHERE id = ? AND project_id = ?
            "#,
        )
        .bind(memory_id)
        .bind(project_id)
        .execute(&self.db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("Memory {}", memory_id)));
        }

        // Delete from Qdrant
        self.qdrant.delete(project_slug, memory_id).await?;

        // Delete related links
        sqlx::query(
            r#"
            DELETE FROM memory_links
            WHERE source_id = ? OR target_id = ?
            "#,
        )
        .bind(memory_id)
        .bind(memory_id)
        .execute(&self.db)
        .await?;

        debug!(id = %memory_id, "Deleted memory");

        Ok(())
    }

    /// Search memories using semantic similarity.
    ///
    /// This is a convenience method that uses default decay parameters.
    /// For full control, use `search_with_params`.
    pub async fn search(
        &self,
        project_id: &str,
        project_slug: &str,
        query: &str,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        let params = crate::models::SearchParams {
            query: query.to_string(),
            memory_type,
            limit,
            ..Default::default()
        };
        self.search_with_params(project_id, project_slug, params).await
    }

    /// Search memories with full control over decay parameters.
    ///
    /// Returns results ranked by a combination of semantic similarity
    /// and retrieval strength (recency decay + access frequency).
    pub async fn search_with_params(
        &self,
        project_id: &str,
        project_slug: &str,
        params: crate::models::SearchParams,
    ) -> Result<Vec<MemorySearchResult>> {
        use super::decay::{blend_scores, calculate_strength};

        // Generate query embedding
        let embedding = self.embeddings.embed_single(&params.query).await?;

        // Build filter
        let filter = params.memory_type.map(|mt| SearchFilter::new().with_type(mt.as_str()));

        // Request more results than needed to account for re-ranking
        // (strength-based re-ranking might push some results up)
        let fetch_limit = (params.limit * 2).max(20);

        // Search in Qdrant
        let vector_results = self.qdrant.search(project_slug, embedding, fetch_limit, filter).await?;

        // Fetch full memories and compute combined scores
        let mut results = Vec::with_capacity(vector_results.len());
        for vr in vector_results {
            // Fetch memory without updating access count (we'll batch update later)
            let memory = match self.get_without_tracking(project_id, &vr.id).await? {
                Some(m) => m,
                None => continue,
            };

            // Calculate retrieval strength based on recency and access count
            let strength = calculate_strength(
                memory.updated_at,
                memory.last_accessed,
                memory.retrieval_count,
                params.decay_half_life_days,
            );

            // Blend semantic relevance with retrieval strength
            let relevance = vr.score as f64;
            let combined = blend_scores(relevance, strength, params.strength_weight);

            results.push(MemorySearchResult::with_decay(
                memory,
                vr.score,
                strength as f32,
                combined as f32,
            ));
        }

        // Re-rank by combined score
        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Trim to requested limit
        results.truncate(params.limit);

        // Update access tracking for returned results
        self.track_search_access(&results).await;

        Ok(results)
    }

    /// Get a memory without updating access tracking (for internal use during search).
    async fn get_without_tracking(&self, project_id: &str, memory_id: &str) -> Result<Option<Memory>> {
        let memory = sqlx::query_as::<_, Memory>(
            r#"
            SELECT * FROM memories
            WHERE id = ? AND project_id = ?
            "#,
        )
        .bind(memory_id)
        .bind(project_id)
        .fetch_optional(&self.db)
        .await?;

        Ok(memory)
    }

    /// Update access tracking for memories returned in search results.
    async fn track_search_access(&self, results: &[MemorySearchResult]) {
        if results.is_empty() {
            return;
        }

        // Batch update retrieval counts and last_accessed for all results
        for result in results {
            let _ = sqlx::query(
                r#"
                UPDATE memories
                SET retrieval_count = retrieval_count + 1,
                    last_accessed = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(&result.memory.id)
            .execute(&self.db)
            .await;
        }
    }

    /// Get relevant context for a task.
    ///
    /// This is a convenience method that uses default decay parameters.
    /// For full control, use `get_context_with_params`.
    pub async fn get_context(
        &self,
        project_id: &str,
        project_slug: &str,
        task: &str,
        types: Option<Vec<MemoryType>>,
        limit: usize,
    ) -> Result<ContextResult> {
        self.get_context_with_params(
            project_id,
            project_slug,
            task,
            types,
            limit,
            super::decay::DEFAULT_STRENGTH_WEIGHT,
            super::decay::DEFAULT_HALF_LIFE_DAYS,
        )
        .await
    }

    /// Get relevant context for a task with decay parameters.
    pub async fn get_context_with_params(
        &self,
        project_id: &str,
        project_slug: &str,
        task: &str,
        types: Option<Vec<MemoryType>>,
        limit: usize,
        strength_weight: f64,
        decay_half_life_days: f64,
    ) -> Result<ContextResult> {
        let types = types.unwrap_or_else(|| {
            vec![
                MemoryType::Codebase,
                MemoryType::Spec,
                MemoryType::Decision,
                MemoryType::Session,
            ]
        });

        let per_type_limit = (limit / types.len()).max(1);

        let mut context = ContextResult {
            task: task.to_string(),
            code: Vec::new(),
            specs: Vec::new(),
            decisions: Vec::new(),
            sessions: Vec::new(),
            other: Vec::new(),
        };

        for memory_type in types {
            let search_params = crate::models::SearchParams {
                query: task.to_string(),
                memory_type: Some(memory_type),
                limit: per_type_limit,
                strength_weight,
                decay_half_life_days,
                ..Default::default()
            };

            let results = self
                .search_with_params(project_id, project_slug, search_params)
                .await?;

            for result in results {
                let item = ContextItem {
                    id: result.memory.id.clone(),
                    title: result.memory.title.clone(),
                    content: result.memory.content.chars().take(500).collect(),
                    score: result.combined_score, // Use combined score
                    file_path: result.memory.file_path.clone(),
                    author: result.memory.author.clone(),
                };

                match memory_type {
                    MemoryType::Codebase => context.code.push(item),
                    MemoryType::Spec => context.specs.push(item),
                    MemoryType::Decision => context.decisions.push(item),
                    MemoryType::Session => context.sessions.push(item),
                    _ => context.other.push(item),
                }
            }
        }

        Ok(context)
    }

    /// Delete all memories for a project.
    pub async fn delete_all_for_project(&self, project_id: &str, project_slug: &str) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM memories WHERE project_id = ?
            "#,
        )
        .bind(project_id)
        .execute(&self.db)
        .await?;

        // Delete collection from Qdrant
        self.qdrant.delete_collection(project_slug).await?;

        // Delete related links
        sqlx::query(
            r#"
            DELETE FROM memory_links WHERE project_id = ?
            "#,
        )
        .bind(project_id)
        .execute(&self.db)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get memory count by type for a project.
    pub async fn count_by_type(&self, project_id: &str) -> Result<HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT type, COUNT(*) as count
            FROM memories
            WHERE project_id = ?
            GROUP BY type
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// Auto-link memory to similar existing memories using vector similarity.
    ///
    /// Searches Qdrant for the top-k most similar memories and creates
    /// "related" links for any that exceed the similarity threshold.
    /// This runs asynchronously and doesn't fail the memory creation if linking fails.
    async fn auto_link_similar(
        &self,
        memory_id: &str,
        project_id: &str,
        project_slug: &str,
        embedding: &[f32],
    ) {
        const SIMILARITY_THRESHOLD: f32 = 0.75;
        const MAX_LINKS: usize = 5;

        // Search for similar memories
        let similar = match self
            .qdrant
            .search(project_slug, embedding.to_vec(), MAX_LINKS + 1, None)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                warn!(error = %e, "Failed to search for similar memories during auto-linking");
                return;
            }
        };

        let mut links_created = 0;
        for result in similar {
            // Skip self and low-similarity matches
            if result.id == memory_id || result.score < SIMILARITY_THRESHOLD {
                continue;
            }

            // Create bidirectional "related" link
            let link_result = sqlx::query(
                r#"
                INSERT OR IGNORE INTO memory_links (
                    id, project_id, source_id, target_id, link_type,
                    created_by, confidence, context, created_at
                ) VALUES (?, ?, ?, ?, 'related', 'system', ?, 'Auto-linked by vector similarity', datetime('now'))
                "#,
            )
            .bind(crate::models::new_id())
            .bind(project_id)
            .bind(memory_id)
            .bind(&result.id)
            .bind(result.score)
            .execute(&self.db)
            .await;

            match link_result {
                Ok(r) if r.rows_affected() > 0 => {
                    links_created += 1;
                    debug!(
                        source = %memory_id,
                        target = %result.id,
                        score = %result.score,
                        "Auto-linked similar memory"
                    );
                }
                Ok(_) => {
                    // Link already exists, that's fine
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create auto-link");
                }
            }
        }

        if links_created > 0 {
            info!(
                memory_id = %memory_id,
                links_created = %links_created,
                "Auto-linked to similar memories"
            );
        }
    }

    /// Insert memory into SQLite
    async fn insert_memory(&self, memory: &Memory) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO memories (
                id, project_id, type, content, title, author,
                keywords, tags, context, file_path, language,
                line_start, line_end, status, assignee, metadata,
                created_at, updated_at, retrieval_count, last_accessed
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
            )
            "#,
        )
        .bind(&memory.id)
        .bind(&memory.project_id)
        .bind(&memory.memory_type)
        .bind(&memory.content)
        .bind(&memory.title)
        .bind(&memory.author)
        .bind(&memory.keywords)
        .bind(&memory.tags)
        .bind(&memory.context)
        .bind(&memory.file_path)
        .bind(&memory.language)
        .bind(memory.line_start)
        .bind(memory.line_end)
        .bind(&memory.status)
        .bind(&memory.assignee)
        .bind(&memory.metadata)
        .bind(memory.created_at)
        .bind(memory.updated_at)
        .bind(memory.retrieval_count)
        .bind(memory.last_accessed)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

/// Context gathered for a task
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextResult {
    pub task: String,
    pub code: Vec<ContextItem>,
    pub specs: Vec<ContextItem>,
    pub decisions: Vec<ContextItem>,
    pub sessions: Vec<ContextItem>,
    pub other: Vec<ContextItem>,
}

/// A single context item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextItem {
    pub id: String,
    pub title: Option<String>,
    pub content: String,
    pub score: f32,
    pub file_path: Option<String>,
    pub author: Option<String>,
}
