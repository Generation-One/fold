//! Linker service for auto-creating links between memories.
//!
//! Analyzes memories and automatically creates relationships based on:
//! - File path relationships (same directory, imports)
//! - Semantic similarity
//! - LLM-suggested links
//! - Commit/file associations

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn, error};

use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{LinkType, Memory, MemoryLink, MemoryType};

use super::{EmbeddingService, LlmService, MemoryService, QdrantService};

/// Service for automatically creating links between memories.
#[derive(Clone)]
pub struct LinkerService {
    db: DbPool,
    memory: MemoryService,
    llm: Arc<LlmService>,
    qdrant: Arc<QdrantService>,
    embeddings: Arc<EmbeddingService>,
}

/// Link suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkSuggestion {
    pub source_id: String,
    pub target_id: String,
    pub link_type: String,
    pub confidence: f64,
    pub reason: String,
    pub source: String, // 'semantic', 'path', 'llm', 'commit'
}

/// Result of linking operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkingResult {
    pub memory_id: String,
    pub links_created: usize,
    pub suggestions: Vec<LinkSuggestion>,
}

impl LinkerService {
    /// Create a new linker service.
    pub fn new(
        db: DbPool,
        memory: MemoryService,
        llm: Arc<LlmService>,
        qdrant: Arc<QdrantService>,
        embeddings: Arc<EmbeddingService>,
    ) -> Self {
        Self {
            db,
            memory,
            llm,
            qdrant,
            embeddings,
        }
    }

    /// Auto-link a memory to related memories.
    pub async fn auto_link(
        &self,
        project_id: &str,
        project_slug: &str,
        memory_id: &str,
        min_confidence: f64,
    ) -> Result<LinkingResult> {
        let memory = self
            .memory
            .get(project_id, memory_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Memory {}", memory_id)))?;

        let mut all_suggestions: Vec<LinkSuggestion> = Vec::new();

        // 1. Find semantically similar memories
        let semantic_suggestions = self
            .find_semantic_links(&memory, project_id, project_slug)
            .await?;
        all_suggestions.extend(semantic_suggestions);

        // 2. Find path-based links (for codebase memories)
        if memory.memory_type == "codebase" {
            let path_suggestions = self.find_path_links(&memory, project_id).await?;
            all_suggestions.extend(path_suggestions);
        }

        // 3. Find LLM-suggested links (if LLM available)
        if self.llm.is_available().await {
            let llm_suggestions = self
                .find_llm_links(&memory, project_id, project_slug)
                .await?;
            all_suggestions.extend(llm_suggestions);
        }

        // Deduplicate and filter by confidence
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut filtered_suggestions: Vec<LinkSuggestion> = Vec::new();

        for suggestion in all_suggestions {
            let key = if suggestion.source_id < suggestion.target_id {
                (suggestion.source_id.clone(), suggestion.target_id.clone())
            } else {
                (suggestion.target_id.clone(), suggestion.source_id.clone())
            };

            if !seen.contains(&key) && suggestion.confidence >= min_confidence {
                seen.insert(key);
                filtered_suggestions.push(suggestion);
            }
        }

        // Sort by confidence
        filtered_suggestions.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Log top suggestions for debugging
        for (i, s) in filtered_suggestions.iter().take(5).enumerate() {
            info!(
                rank = i + 1,
                target = %s.target_id,
                confidence = s.confidence,
                link_type = %s.link_type,
                "Link suggestion"
            );
        }

        // Create links for suggestions above threshold
        let mut links_created = 0;
        debug!(
            suggestions_count = filtered_suggestions.len(),
            threshold = 0.3,
            "Creating links for suggestions above threshold"
        );

        for suggestion in &filtered_suggestions {
            if suggestion.confidence >= 0.3 {
                debug!(
                    source = %suggestion.source_id,
                    target = %suggestion.target_id,
                    link_type = %suggestion.link_type,
                    confidence = suggestion.confidence,
                    "Attempting to create link"
                );

                // Auto-create for moderate-to-high confidence
                match self
                    .create_link(
                        project_id,
                        &suggestion.source_id,
                        &suggestion.target_id,
                        &suggestion.link_type,
                        Some(suggestion.confidence),
                        Some(&suggestion.reason),
                        "ai",
                    )
                    .await
                {
                    Ok(link) => {
                        links_created += 1;
                        info!(
                            link_id = %link.id,
                            source = %suggestion.source_id,
                            target = %suggestion.target_id,
                            "Successfully created link"
                        );
                    }
                    Err(e) => {
                        error!(
                            source = %suggestion.source_id,
                            target = %suggestion.target_id,
                            error = %e,
                            "Failed to create link"
                        );
                    }
                }
            }
        }

        info!(
            memory_id = memory_id,
            suggestions = filtered_suggestions.len(),
            links_created = links_created,
            "Auto-linking completed"
        );

        Ok(LinkingResult {
            memory_id: memory_id.to_string(),
            links_created,
            suggestions: filtered_suggestions,
        })
    }

    /// Find semantically similar memories using chunk-level search.
    ///
    /// This enhanced version uses chunk-level similarity to find more precise links.
    /// When chunks from different memories are similar, we link the parent memories
    /// and include chunk details in the link reason.
    async fn find_semantic_links(
        &self,
        memory: &Memory,
        project_id: &str,
        project_slug: &str,
    ) -> Result<Vec<LinkSuggestion>> {
        let search_text = memory.to_search_text();
        debug!(
            memory_id = %memory.id,
            search_text_len = search_text.len(),
            project_slug = %project_slug,
            "Finding semantic links for memory (with chunks)"
        );

        // Use chunk-aware search for more precise linking
        let search_results = self
            .memory
            .search_with_chunks(project_id, project_slug, &search_text, None, 15)
            .await?;

        debug!(
            memory_id = %memory.id,
            results_count = search_results.len(),
            "Chunk-aware semantic search completed"
        );

        let mut suggestions = Vec::new();

        for result in search_results {
            // Skip self
            if result.memory.id == memory.id {
                debug!(target_id = %result.memory.id, "Skipping self");
                continue;
            }

            // Skip if already linked
            if self
                .link_exists(&memory.id, &result.memory.id)
                .await?
            {
                debug!(target_id = %result.memory.id, "Skipping - already linked");
                continue;
            }

            // Determine link type based on memory types
            let link_type = self.infer_link_type(memory, &result.memory);

            // Build reason including chunk details if available
            let (reason, confidence_boost) = if !result.matched_chunks.is_empty() {
                let chunk_count = result.matched_chunks.len();
                let top_chunks: Vec<String> = result.matched_chunks
                    .iter()
                    .take(3)
                    .map(|c| {
                        if let Some(name) = &c.node_name {
                            format!("{} '{}' (L{}-{})", c.node_type, name, c.start_line, c.end_line)
                        } else {
                            format!("{} (L{}-{})", c.node_type, c.start_line, c.end_line)
                        }
                    })
                    .collect();

                let reason = format!(
                    "Similar chunks: {} matched. Top: {}",
                    chunk_count,
                    top_chunks.join(", ")
                );

                // Boost confidence based on number of matching chunks
                let boost = (chunk_count as f64 * 0.02).min(0.1);
                (reason, boost)
            } else {
                ("Semantically similar content".to_string(), 0.0)
            };

            // Calculate final confidence with boost
            let confidence = (result.score as f64 + confidence_boost).min(1.0);

            debug!(
                source_id = %memory.id,
                target_id = %result.memory.id,
                score = result.score,
                confidence = confidence,
                matched_chunks = result.matched_chunks.len(),
                link_type = %link_type,
                target_title = result.memory.title.as_deref().unwrap_or("untitled"),
                "Found semantic link candidate"
            );

            suggestions.push(LinkSuggestion {
                source_id: memory.id.clone(),
                target_id: result.memory.id.clone(),
                link_type,
                confidence,
                reason,
                source: "semantic".to_string(),
            });
        }

        debug!(
            memory_id = %memory.id,
            suggestions_count = suggestions.len(),
            "Semantic link suggestions generated"
        );

        Ok(suggestions)
    }

    /// Find links based on file paths.
    async fn find_path_links(
        &self,
        memory: &Memory,
        project_id: &str,
    ) -> Result<Vec<LinkSuggestion>> {
        let file_path = match &memory.file_path {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let mut suggestions = Vec::new();

        // Find files in the same directory
        let parent_dir = std::path::Path::new(file_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if !parent_dir.is_empty() {
            let siblings: Vec<Memory> = sqlx::query_as(
                r#"
                SELECT * FROM memories
                WHERE project_id = ?
                  AND type = 'codebase'
                  AND file_path LIKE ?
                  AND id != ?
                "#,
            )
            .bind(project_id)
            .bind(format!("{}/%", parent_dir))
            .bind(&memory.id)
            .fetch_all(&self.db)
            .await?;

            for sibling in siblings {
                if !self.link_exists(&memory.id, &sibling.id).await? {
                    suggestions.push(LinkSuggestion {
                        source_id: memory.id.clone(),
                        target_id: sibling.id.clone(),
                        link_type: LinkType::Related.as_str().to_string(),
                        confidence: 0.5,
                        reason: format!("Same directory: {}", parent_dir),
                        source: "path".to_string(),
                    });
                }
            }
        }

        // Find test file relationships
        if file_path.contains(".test.") || file_path.contains("_test.") || file_path.contains("/tests/") {
            // Try to find the source file
            let source_path = file_path
                .replace(".test.", ".")
                .replace("_test.", ".")
                .replace("/tests/", "/src/")
                .replace("/test/", "/");

            let source_file: Option<Memory> = sqlx::query_as(
                r#"
                SELECT * FROM memories
                WHERE project_id = ?
                  AND type = 'codebase'
                  AND file_path LIKE ?
                LIMIT 1
                "#,
            )
            .bind(project_id)
            .bind(format!("%{}", source_path.split('/').last().unwrap_or("")))
            .fetch_optional(&self.db)
            .await?;

            if let Some(source) = source_file {
                if !self.link_exists(&memory.id, &source.id).await? {
                    suggestions.push(LinkSuggestion {
                        source_id: memory.id.clone(),
                        target_id: source.id.clone(),
                        link_type: LinkType::References.as_str().to_string(),
                        confidence: 0.8,
                        reason: "Test file for source".to_string(),
                        source: "path".to_string(),
                    });
                }
            }
        }

        Ok(suggestions)
    }

    /// Find LLM-suggested links.
    async fn find_llm_links(
        &self,
        memory: &Memory,
        project_id: &str,
        project_slug: &str,
    ) -> Result<Vec<LinkSuggestion>> {
        // Get candidate memories
        let candidates = self
            .memory
            .search(project_id, project_slug, &memory.to_search_text(), 15)
            .await?;

        let candidate_memories: Vec<Memory> = candidates
            .into_iter()
            .filter(|r| r.memory.id != memory.id)
            .map(|r| r.memory)
            .collect();

        if candidate_memories.is_empty() {
            return Ok(Vec::new());
        }

        // Ask LLM for link suggestions
        let llm_suggestions = self
            .llm
            .suggest_links(memory, &candidate_memories)
            .await?;

        let mut suggestions = Vec::new();

        for s in llm_suggestions {
            if !self.link_exists(&s.source_id, &s.target_id).await? {
                suggestions.push(LinkSuggestion {
                    source_id: s.source_id,
                    target_id: s.target_id,
                    link_type: s.link_type,
                    confidence: s.confidence as f64,
                    reason: s.reason,
                    source: "llm".to_string(),
                });
            }
        }

        Ok(suggestions)
    }

    /// Infer link type based on memory types.
    fn infer_link_type(&self, source: &Memory, target: &Memory) -> String {
        let source_type = MemoryType::from_str(&source.memory_type);
        let target_type = MemoryType::from_str(&target.memory_type);

        match (source_type, target_type) {
            (Some(MemoryType::Codebase), Some(MemoryType::Spec)) => {
                LinkType::Implements.as_str().to_string()
            }
            (Some(MemoryType::Codebase), Some(MemoryType::Decision)) => {
                LinkType::Implements.as_str().to_string()
            }
            (Some(MemoryType::Commit), Some(MemoryType::Codebase)) => {
                LinkType::Modifies.as_str().to_string()
            }
            (Some(MemoryType::Session), Some(MemoryType::Codebase)) => {
                LinkType::Affects.as_str().to_string()
            }
            (Some(MemoryType::Decision), Some(MemoryType::Spec)) => {
                LinkType::Decides.as_str().to_string()
            }
            _ => LinkType::Related.as_str().to_string(),
        }
    }

    /// Check if a link already exists between two memories.
    async fn link_exists(&self, source_id: &str, target_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM memory_links
            WHERE (source_id = ? AND target_id = ?)
               OR (source_id = ? AND target_id = ?)
            "#,
        )
        .bind(source_id)
        .bind(target_id)
        .bind(target_id)
        .bind(source_id)
        .fetch_one(&self.db)
        .await?;

        Ok(count > 0)
    }

    /// Create a link between two memories.
    pub async fn create_link(
        &self,
        project_id: &str,
        source_id: &str,
        target_id: &str,
        link_type: &str,
        confidence: Option<f64>,
        context: Option<&str>,
        created_by: &str,
    ) -> Result<MemoryLink> {
        let link = MemoryLink {
            id: crate::models::new_id(),
            project_id: project_id.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            link_type: link_type.to_string(),
            created_by: created_by.to_string(),
            confidence,
            context: context.map(String::from),
            change_type: None,
            additions: None,
            deletions: None,
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO memory_links (
                id, project_id, source_id, target_id, link_type,
                created_by, confidence, context, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&link.id)
        .bind(&link.project_id)
        .bind(&link.source_id)
        .bind(&link.target_id)
        .bind(&link.link_type)
        .bind(&link.created_by)
        .bind(link.confidence)
        .bind(&link.context)
        .bind(link.created_at)
        .execute(&self.db)
        .await?;

        debug!(
            source = source_id,
            target = target_id,
            link_type = link_type,
            "Created link"
        );

        Ok(link)
    }

    /// Delete a link.
    pub async fn delete_link(&self, link_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"DELETE FROM memory_links WHERE id = ?"#,
        )
        .bind(link_id)
        .execute(&self.db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("Link {}", link_id)));
        }

        Ok(())
    }

    /// Batch auto-link all memories in a project.
    pub async fn batch_auto_link(
        &self,
        project_id: &str,
        project_slug: &str,
        min_confidence: f64,
    ) -> Result<BatchLinkingResult> {
        let memories: Vec<Memory> = sqlx::query_as(
            r#"SELECT * FROM memories WHERE project_id = ?"#,
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        let mut total_links_created = 0;
        let mut total_suggestions = 0;

        for memory in &memories {
            let result = self
                .auto_link(project_id, project_slug, &memory.id, min_confidence)
                .await?;

            total_links_created += result.links_created;
            total_suggestions += result.suggestions.len();
        }

        info!(
            project_id = project_id,
            memories = memories.len(),
            links_created = total_links_created,
            suggestions = total_suggestions,
            "Batch auto-linking completed"
        );

        Ok(BatchLinkingResult {
            memories_processed: memories.len(),
            links_created: total_links_created,
            total_suggestions,
        })
    }
}

/// Result of batch linking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchLinkingResult {
    pub memories_processed: usize,
    pub links_created: usize,
    pub total_suggestions: usize,
}
