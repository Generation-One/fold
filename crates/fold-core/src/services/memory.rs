//! Agentic Memory Service.
//!
//! Implements an agentic memory system based on A-MEM principles:
//! - LLM-powered content analysis for automatic metadata extraction
//! - Memory evolution with LLM-driven linking decisions
//! - Neighbour metadata updates during evolution
//! - Link traversal for holographic context retrieval
//!
//! Content storage depends on source:
//! - Agent memories: content stored in fold/{char1}/{char2}/{hash}.md
//! - File/Git memories: content (LLM summaries) stored in SQLite

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::db::{self, DbPool};
use crate::error::{Error, Result};
use crate::models::{
    ChunkMatch, Memory, MemoryCreate, MemorySearchResult, MemorySource, MemoryType, MemoryUpdate,
};

use super::decay::{
    blend_scores, calculate_strength, DEFAULT_HALF_LIFE_DAYS, DEFAULT_STRENGTH_WEIGHT,
};
use super::fold_storage::FoldStorageService;
use super::EmbeddingService;
use fold_qdrant::{QdrantService, SearchFilter};
use super::LlmService;

/// Safe floor char boundary (stable alternative to str::floor_char_boundary)
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        s.len()
    } else {
        let mut i = index;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
}

/// Result of LLM content analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAnalysis {
    pub keywords: Vec<String>,
    pub context: String,
    pub tags: Vec<String>,
}

impl Default for ContentAnalysis {
    fn default() -> Self {
        Self {
            keywords: Vec::new(),
            context: String::new(),
            tags: Vec::new(),
        }
    }
}

/// LLM decision about memory evolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvolutionDecision {
    should_evolve: bool,
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    suggested_connections: Vec<String>,
    #[serde(default)]
    tags_to_update: Vec<String>,
    #[serde(default)]
    new_context_neighbourhood: Vec<String>,
    #[serde(default)]
    new_tags_neighbourhood: Vec<Vec<String>>,
}

impl Default for EvolutionDecision {
    fn default() -> Self {
        Self {
            should_evolve: false,
            actions: Vec::new(),
            suggested_connections: Vec::new(),
            tags_to_update: Vec::new(),
            new_context_neighbourhood: Vec::new(),
            new_tags_neighbourhood: Vec::new(),
        }
    }
}

/// Search result with optional neighbour flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticSearchResult {
    pub memory: Memory,
    pub content: String,
    pub score: f32,
    pub is_neighbour: bool,
}

/// Context response with holographic memory reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    pub memory: Memory,
    pub content: String,
    pub related: Vec<MemoryWithContent>,
    pub depth: usize,
}

/// Memory with content for context responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryWithContent {
    pub memory: Memory,
    pub content: String,
}

/// Service for managing memories with agentic evolution.
#[derive(Clone)]
pub struct MemoryService {
    db: DbPool,
    qdrant: Arc<QdrantService>,
    embeddings: Arc<EmbeddingService>,
    llm: Arc<LlmService>,
    fold_storage: Arc<FoldStorageService>,
}

impl MemoryService {
    /// Create a new memory service.
    pub fn new(
        db: DbPool,
        qdrant: Arc<QdrantService>,
        embeddings: Arc<EmbeddingService>,
        llm: Arc<LlmService>,
        fold_storage: Arc<FoldStorageService>,
    ) -> Self {
        Self {
            db,
            qdrant,
            embeddings,
            llm,
            fold_storage,
        }
    }

    // =========================================================================
    // Content Analysis (LLM-powered)
    // =========================================================================

    /// Analyse content using LLM to extract semantic metadata.
    ///
    /// Extracts:
    /// - Keywords: Key terms and concepts (nouns, verbs, important terminology)
    /// - Context: One sentence summarising the domain, purpose, and key points
    /// - Tags: Broad categories for classification
    pub async fn analyse_content(&self, content: &str) -> Result<ContentAnalysis> {
        if !self.llm.is_available().await {
            debug!("LLM not available, returning empty analysis");
            return Ok(ContentAnalysis::default());
        }

        let prompt = format!(
            r#"Analyse this content and extract semantic metadata for a knowledge base.

1. **Keywords**: Key terms and concepts - function names, class names, technical terms, domain vocabulary (max 15)
2. **Context**: A detailed 3-5 sentence summary covering:
   - What this content does and its primary purpose
   - Its role in the broader system or domain
   - Key responsibilities, patterns, or architectural approach
   - Important relationships to other components
   - Any notable design decisions or constraints
3. **Tags**: Broad categories for classification (max 6)

Content:
{}

Return JSON:
{{
    "keywords": ["term1", "term2", ...],
    "context": "Detailed multi-sentence context paragraph...",
    "tags": ["category1", "category2", ...]
}}"#,
            &content[..floor_char_boundary(&content, content.len().min(4000))]
        );

        match self.llm.complete(&prompt, 800).await {
            Ok(response) => {
                // Try to extract JSON from response
                if let Some(json) = self.extract_json(&response) {
                    let keywords = json["keywords"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    let context = json["context"].as_str().unwrap_or("").to_string();

                    let tags = json["tags"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    Ok(ContentAnalysis {
                        keywords,
                        context,
                        tags,
                    })
                } else {
                    warn!("Failed to parse content analysis response as JSON");
                    Ok(ContentAnalysis::default())
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to analyse content");
                Ok(ContentAnalysis::default())
            }
        }
    }

    /// Generate a short title for content using LLM.
    ///
    /// Returns a concise, descriptive title (max 60 chars) suitable for
    /// use in slugs and file names.
    pub async fn generate_title(&self, content: &str) -> Result<String> {
        if !self.llm.is_available().await {
            debug!("LLM not available for title generation");
            return Err(Error::Internal("LLM not available".to_string()));
        }

        let truncated = &content[..floor_char_boundary(&content, content.len().min(2000))];
        let prompt = format!(
            r#"Generate a short, descriptive title for this content.

Requirements:
- Maximum 60 characters
- Use sentence case (capitalise first word only, except proper nouns)
- Be specific and descriptive
- No quotes or special punctuation
- Should work as a file name

Content:
{}

Return only the title, nothing else."#,
            truncated
        );

        match self.llm.complete(&prompt, 80).await {
            Ok(response) => {
                let title = response.trim().trim_matches('"').trim();
                // Ensure it's not too long
                let title = if title.len() > 60 {
                    title.chars().take(60).collect::<String>().trim_end().to_string()
                } else {
                    title.to_string()
                };
                Ok(title)
            }
            Err(e) => {
                warn!(error = %e, "Failed to generate title");
                Err(Error::Internal(format!("Title generation failed: {}", e)))
            }
        }
    }

    /// Extract JSON from LLM response text.
    fn extract_json(&self, text: &str) -> Option<Value> {
        // Try to find JSON in code blocks
        if let Some(start) = text.find("```json") {
            let start = start + 7;
            if let Some(end) = text[start..].find("```") {
                if let Ok(json) = serde_json::from_str(&text[start..start + end]) {
                    return Some(json);
                }
            }
        }

        // Try to find JSON in generic code blocks
        if let Some(start) = text.find("```") {
            let start = start + 3;
            let start = text[start..]
                .find('\n')
                .map(|i| start + i + 1)
                .unwrap_or(start);
            if let Some(end) = text[start..].find("```") {
                if let Ok(json) = serde_json::from_str(&text[start..start + end]) {
                    return Some(json);
                }
            }
        }

        // Try to find raw JSON object
        if let Some(start) = text.find('{') {
            let mut depth = 0;
            let mut end = start;
            for (i, c) in text[start..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > start {
                if let Ok(json) = serde_json::from_str(&text[start..end]) {
                    return Some(json);
                }
            }
        }

        None
    }

    // =========================================================================
    // Memory Evolution (LLM-driven)
    // =========================================================================

    /// Get LLM decision on memory evolution.
    async fn get_evolution_decision(
        &self,
        memory: &Memory,
        content: &str,
        neighbours_text: &str,
        neighbour_count: usize,
    ) -> Result<EvolutionDecision> {
        if !self.llm.is_available().await || neighbour_count == 0 {
            return Ok(EvolutionDecision::default());
        }

        let prompt = format!(
            r#"You are a memory evolution agent. Analyse the new memory and its neighbours to decide on evolution.

New memory:
- Title: {}
- Content: {}
- Context: {}
- Keywords: {:?}
- Tags: {:?}

Nearest neighbour memories:
{}

Number of neighbours: {}

Decide:
1. Should this memory be evolved (linked/organised)?
2. Actions to take:
   - "strengthen": Link to similar memories, update tags
   - "update_neighbor": Update context/tags of related memories

Return JSON:
{{
    "should_evolve": true/false,
    "actions": ["strengthen", "update_neighbor"],
    "suggested_connections": ["memory_id_1", "memory_id_2"],
    "tags_to_update": ["tag1", "tag2"],
    "new_context_neighbourhood": ["new context for neighbour 1", ...],
    "new_tags_neighbourhood": [["tag1", "tag2"], ...]
}}"#,
            memory.title.as_deref().unwrap_or(""),
            &content[..floor_char_boundary(&content, content.len().min(1000))],
            memory.context.as_deref().unwrap_or(""),
            memory.keywords_vec(),
            memory.tags_vec(),
            neighbours_text,
            neighbour_count,
        );

        match self.llm.complete(&prompt, 800).await {
            Ok(response) => {
                if let Some(json) = self.extract_json(&response) {
                    let decision: EvolutionDecision =
                        serde_json::from_value(json).unwrap_or_default();
                    Ok(decision)
                } else {
                    warn!("Failed to parse evolution decision response as JSON");
                    Ok(EvolutionDecision::default())
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to get evolution decision");
                Ok(EvolutionDecision::default())
            }
        }
    }

    /// Process memory evolution - decide if/how to link and update neighbours.
    async fn process_memory_evolution(
        &self,
        memory_id: &str,
        project_id: &str,
        project_slug: &str,
        project_root: &Path,
        embedding: &[f32],
        content: &str,
    ) -> Result<()> {
        // Find nearest neighbours
        let neighbours = self
            .qdrant
            .search(project_slug, embedding.to_vec(), 5, None)
            .await?;

        if neighbours.is_empty() {
            return Ok(()); // First memory, no evolution needed
        }

        // Build neighbour context for LLM
        let mut neighbour_text = String::new();
        let mut neighbour_ids = Vec::new();

        for result in &neighbours {
            if result.id == memory_id {
                continue; // Skip self
            }

            if let Ok(Some(neighbour)) = self.get_without_tracking(project_id, &result.id).await {
                neighbour_ids.push(result.id.clone());

                // Try to get content from fold storage
                let neighbour_content = if let Ok((_, nc)) = self
                    .fold_storage
                    .read_memory(project_root, &result.id)
                    .await
                {
                    nc[..floor_char_boundary(&nc, nc.len().min(200))].to_string()
                } else {
                    String::new()
                };

                neighbour_text.push_str(&format!(
                    "memory_id:{}\tcontent:{}\tcontext:{}\tkeywords:{:?}\ttags:{:?}\n",
                    result.id,
                    neighbour_content,
                    neighbour.context.as_deref().unwrap_or(""),
                    neighbour.keywords_vec(),
                    neighbour.tags_vec(),
                ));
            }
        }

        if neighbour_ids.is_empty() {
            return Ok(());
        }

        // Get the memory for evolution
        let memory = match self.get_without_tracking(project_id, memory_id).await? {
            Some(m) => m,
            None => return Ok(()),
        };

        // Ask LLM for evolution decision
        let decision = self
            .get_evolution_decision(&memory, content, &neighbour_text, neighbour_ids.len())
            .await?;

        if !decision.should_evolve {
            return Ok(());
        }

        // Collect all links created for updating fold files
        let mut created_links: Vec<String> = Vec::new();

        // Apply evolution actions
        for action in &decision.actions {
            match action.as_str() {
                "strengthen" => {
                    // Create links to suggested connections
                    for target_id in &decision.suggested_connections {
                        // Create bidirectional link
                        let _ = sqlx::query(
                            r#"
                            INSERT OR IGNORE INTO memory_links (
                                id, project_id, source_id, target_id, link_type,
                                created_by, confidence, context, created_at
                            ) VALUES (?, ?, ?, ?, 'related', 'evolution', 0.8, 'Auto-linked by memory evolution', datetime('now'))
                            "#,
                        )
                        .bind(crate::models::new_id())
                        .bind(project_id)
                        .bind(memory_id)
                        .bind(target_id)
                        .execute(&self.db)
                        .await;

                        created_links.push(target_id.clone());

                        debug!(
                            source = %memory_id,
                            target = %target_id,
                            "Linked memories via evolution"
                        );
                    }

                    // Update memory tags if provided
                    if !decision.tags_to_update.is_empty() {
                        let tags_json =
                            serde_json::to_string(&decision.tags_to_update).unwrap_or_default();
                        let _ = sqlx::query(
                            r#"UPDATE memories SET tags = ?, updated_at = datetime('now') WHERE id = ?"#,
                        )
                        .bind(&tags_json)
                        .bind(memory_id)
                        .execute(&self.db)
                        .await;
                    }
                }
                "update_neighbor" => {
                    // Update neighbour memories' metadata
                    for (i, neighbour_id) in neighbour_ids.iter().enumerate() {
                        let mut updated = false;

                        // Update context if provided
                        if i < decision.new_context_neighbourhood.len() {
                            let new_ctx = &decision.new_context_neighbourhood[i];
                            if !new_ctx.is_empty() {
                                let _ = sqlx::query(
                                    r#"UPDATE memories SET context = ?, updated_at = datetime('now') WHERE id = ?"#,
                                )
                                .bind(new_ctx)
                                .bind(neighbour_id)
                                .execute(&self.db)
                                .await;
                                updated = true;
                            }
                        }

                        // Update tags if provided
                        if i < decision.new_tags_neighbourhood.len() {
                            let new_tags = &decision.new_tags_neighbourhood[i];
                            if !new_tags.is_empty() {
                                let tags_json = serde_json::to_string(new_tags).unwrap_or_default();
                                let _ = sqlx::query(
                                    r#"UPDATE memories SET tags = ?, updated_at = datetime('now') WHERE id = ?"#,
                                )
                                .bind(&tags_json)
                                .bind(neighbour_id)
                                .execute(&self.db)
                                .await;
                                updated = true;
                            }
                        }

                        if updated {
                            debug!(neighbour_id = %neighbour_id, "Updated neighbour metadata via evolution");
                        }
                    }
                }
                _ => {}
            }
        }

        // Update fold file with wiki-style links if any were created
        if !created_links.is_empty() {
            if let Err(e) = self
                .fold_storage
                .update_memory_links(project_root, memory_id, &created_links)
                .await
            {
                warn!(error = %e, memory_id = %memory_id, "Failed to update fold file with links");
            }
        }

        Ok(())
    }

    // =========================================================================
    // Build Embedding Text
    // =========================================================================

    /// Build text for embedding that includes content + metadata.
    fn build_embedding_text(&self, memory: &Memory, content: &str) -> String {
        let mut parts = vec![content.to_string()];

        if let Some(ctx) = &memory.context {
            parts.push(ctx.clone());
        }

        let keywords = memory.keywords_vec();
        if !keywords.is_empty() {
            parts.push(keywords.join(" "));
        }

        let tags = memory.tags_vec();
        if !tags.is_empty() {
            parts.push(tags.join(" "));
        }

        if let Some(title) = &memory.title {
            parts.push(title.clone());
        }

        parts.join("\n")
    }

    // =========================================================================
    // CRUD Operations
    // =========================================================================

    /// Add a memory with automatic analysis and evolution.
    ///
    /// Content storage depends on source:
    /// - Agent (or None): content stored in fold/ directory, SQLite content is NULL
    /// - File/Git: content (LLM summary) stored in SQLite, nothing written to fold/
    pub async fn add(
        &self,
        project_id: &str,
        project_slug: &str,
        data: MemoryCreate,
        auto_metadata: bool,
    ) -> Result<Memory> {
        // Get project to find root path
        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

        // Auto-analyse if metadata not provided
        let (keywords, context, tags) =
            if auto_metadata && (data.keywords.is_empty() || data.tags.is_empty()) {
                let analysis = self.analyse_content(&data.content).await?;
                (
                    if data.keywords.is_empty() {
                        analysis.keywords
                    } else {
                        data.keywords.clone()
                    },
                    if data.context.is_none() {
                        Some(analysis.context).filter(|s| !s.is_empty())
                    } else {
                        data.context.clone()
                    },
                    if data.tags.is_empty() {
                        analysis.tags
                    } else {
                        data.tags.clone()
                    },
                )
            } else {
                (
                    data.keywords.clone(),
                    data.context.clone(),
                    data.tags.clone(),
                )
            };

        // Create memory object
        let now = Utc::now();

        // Compute content hash for deduplication and change detection
        let content_hash = {
            let mut hasher = Sha256::new();
            hasher.update(data.content.as_bytes());
            hex::encode(hasher.finalize())
        };

        // Determine if this is an agent memory (writes to fold/) or indexed memory (stores in SQLite)
        let is_agent_memory = match data.source {
            Some(MemorySource::File) | Some(MemorySource::Git) => false,
            Some(MemorySource::Agent) | None => true,
        };

        // Generate title if not provided (for agent memories, use LLM)
        let title = if data.title.is_some() {
            data.title.clone()
        } else if is_agent_memory && auto_metadata {
            // Generate a title using LLM
            let generated = self.generate_title(&data.content).await.ok();
            generated.or_else(|| {
                // Fallback: use first line or truncated content
                let first_line = data.content.lines().next().unwrap_or("Untitled");
                Some(first_line.chars().take(50).collect())
            })
        } else {
            None
        };

        // Generate ID from slug if provided, otherwise use random UUID
        // When a slug is provided, the ID is deterministically derived from it via SHA256
        // This means the same slug always produces the same ID, enabling upsert behaviour
        let (memory_id, memory_slug) = if let Some(slug) = data.slug.clone() {
            let id = fold_storage::slug_to_id(&slug);
            (id, Some(slug))
        } else if let Some(id) = data.id.clone() {
            (id, None)
        } else {
            (crate::models::new_id(), None)
        };

        let memory = Memory {
            id: memory_id,
            project_id: project_id.to_string(),
            slug: memory_slug,
            memory_type: data.memory_type.as_str().to_string(),
            source: data.source.map(|s| s.as_str().to_string()),
            // For agent memories: content stored in fold/, SQLite content is NULL
            // For file/git memories: content (summary) stored in SQLite
            content: if is_agent_memory {
                None
            } else {
                Some(data.content.clone())
            },
            content_hash: Some(content_hash),
            // Deprecated field - kept for backwards compatibility
            content_storage: if is_agent_memory {
                Some("fold".to_string())
            } else {
                None
            },
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

        // Only write to fold/ directory for agent memories
        if is_agent_memory {
            // Ensure fold directory is initialised
            if !self.fold_storage.is_initialised(&project_root).await {
                info!(
                    project_id = %project_id,
                    project_slug = %project_slug,
                    "Initialising fold directory for project"
                );
                self.fold_storage
                    .init_fold_directory(&project_root, project_id, project_slug, &project.name)
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to init fold directory: {}", e)))?;
            }

            self.fold_storage
                .write_memory(&project_root, &memory, &data.content)
                .await?;
        }

        // Insert metadata into SQLite (content included for file/git memories)
        self.insert_memory(&memory).await?;

        // Generate embedding
        let embed_text = self.build_embedding_text(&memory, &data.content);
        let embedding = self.embeddings.embed_single(&embed_text).await?;

        // Ensure Qdrant collection exists
        self.qdrant
            .create_collection(project_slug, self.embeddings.dimension().await)
            .await?;

        // Build Qdrant payload
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
        payload.insert(
            "created_at".to_string(),
            json!(memory.created_at.to_rfc3339()),
        );

        // Store in Qdrant
        self.qdrant
            .upsert(project_slug, &memory.id, embedding.clone(), payload)
            .await?;

        // Process memory evolution (agentic linking) - only for agent memories
        // File/git memories don't need evolution as they're indexed content
        if is_agent_memory {
            self.process_memory_evolution(
                &memory.id,
                project_id,
                project_slug,
                &project_root,
                &embedding,
                &data.content,
            )
            .await?;
        }

        let source_str = if is_agent_memory { "agent" } else { "indexed" };
        info!(id = %memory.id, memory_type = %memory.memory_type, source = %source_str, "Added memory");

        // Return memory with content populated
        let mut result = memory;
        result.content = Some(data.content);
        Ok(result)
    }

    /// Get a memory by ID with content resolved based on source.
    ///
    /// Content resolution:
    /// - Agent memories: read from fold/ directory
    /// - File/Git memories: content already in SQLite (from indexing)
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

        let mut memory = match memory {
            Some(m) => m,
            None => return Ok(None),
        };

        // Update access tracking
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

        // Resolve content based on source
        // - File/Git memories: content already in SQLite (summary from LLM)
        // - Agent memories: content stored in fold/ directory
        let source = memory.source.as_deref().unwrap_or("agent");
        let needs_fold_content = match source {
            "file" | "git" => {
                // Content should be in SQLite, but check if it's missing (legacy records)
                memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty())
            }
            _ => true, // Agent memories always read from fold/
        };

        if needs_fold_content {
            if let Ok(project) = crate::db::get_project(&self.db, project_id).await {
                let project_root = std::path::PathBuf::from(&project.root_path);

                match self
                    .fold_storage
                    .read_memory(&project_root, memory_id)
                    .await
                {
                    Ok((_, content)) => {
                        memory.content = Some(content);
                    }
                    Err(e) => {
                        debug!(
                            memory_id = %memory_id,
                            source = %source,
                            project_root = %project_root.display(),
                            error = %e,
                            "Failed to read memory content from fold/"
                        );
                    }
                }
            }
        }

        Ok(Some(memory))
    }

    /// Get a memory without updating access tracking (for internal use).
    async fn get_without_tracking(
        &self,
        project_id: &str,
        memory_id: &str,
    ) -> Result<Option<Memory>> {
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

    /// List memories with content resolved based on source.
    ///
    /// Content resolution:
    /// - File/Git memories: content already in SQLite
    /// - Agent memories: read from fold/ directory
    pub async fn list_with_content(
        &self,
        project_id: &str,
        _project_slug: &str,
        memory_type: Option<MemoryType>,
        author: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Memory>> {
        let mut memories = self
            .list(project_id, memory_type, author, limit, offset)
            .await?;

        // Resolve content for memories that need it (agent memories without content)
        if let Ok(project) = crate::db::get_project(&self.db, project_id).await {
            let project_root = std::path::PathBuf::from(&project.root_path);

            for memory in &mut memories {
                // Only read from fold/ if content is missing
                // File/Git memories already have content in SQLite
                if memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty())
                {
                    if let Ok((_, content)) = self
                        .fold_storage
                        .read_memory(&project_root, &memory.id)
                        .await
                    {
                        memory.content = Some(content);
                    }
                }
            }
        }

        Ok(memories)
    }

    /// Update a memory.
    ///
    /// Content handling depends on source:
    /// - Agent memories: content stored in fold/ directory
    /// - File/Git memories: content stored in SQLite
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

        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

        // Determine if this is an agent memory (stored in fold/)
        let source = existing.source.as_deref().unwrap_or("agent");
        let is_agent_memory = !matches!(source, "file" | "git");

        // Get current content based on source
        let current_content = if is_agent_memory {
            // Agent memories: read from fold/
            self.fold_storage
                .read_memory(&project_root, memory_id)
                .await
                .map(|(_, c)| c)
                .unwrap_or_default()
        } else {
            // File/Git memories: content already in existing.content from SQLite
            existing.content.clone().unwrap_or_default()
        };

        // Determine new content
        let new_content = update.content.clone().unwrap_or(current_content);

        // Build update values
        let now = Utc::now();
        let title = update.title.or(existing.title.clone());
        let keywords = update
            .keywords
            .map(|k| serde_json::to_string(&k).unwrap())
            .or(existing.keywords.clone());
        let tags = update
            .tags
            .map(|t| serde_json::to_string(&t).unwrap())
            .or(existing.tags.clone());
        let context = update.context.or(existing.context.clone());
        let status = update.status.or(existing.status.clone());
        let assignee = update.assignee.or(existing.assignee.clone());
        let metadata = update
            .metadata
            .map(|m| serde_json::to_string(&m).unwrap())
            .or(existing.metadata.clone());

        // For file/git memories, also update content in SQLite
        let content_for_db = if is_agent_memory {
            None
        } else {
            Some(new_content.clone())
        };

        // Update metadata (and content for file/git) in SQLite
        sqlx::query(
            r#"
            UPDATE memories
            SET title = ?,
                keywords = ?,
                tags = ?,
                context = ?,
                status = ?,
                assignee = ?,
                metadata = ?,
                content = ?,
                updated_at = ?
            WHERE id = ? AND project_id = ?
            "#,
        )
        .bind(&title)
        .bind(&keywords)
        .bind(&tags)
        .bind(&context)
        .bind(&status)
        .bind(&assignee)
        .bind(&metadata)
        .bind(&content_for_db)
        .bind(now)
        .bind(memory_id)
        .bind(project_id)
        .execute(&self.db)
        .await?;

        // Build updated memory struct
        let updated = Memory {
            content: Some(new_content.clone()),
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

        // Only update fold/ file for agent memories
        if is_agent_memory {
            self.fold_storage
                .write_memory(&project_root, &updated, &new_content)
                .await?;
        }

        // Re-embed with new content
        let embed_text = self.build_embedding_text(&updated, &new_content);
        let embedding = self.embeddings.embed_single(&embed_text).await?;

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
        payload.insert(
            "created_at".to_string(),
            json!(updated.created_at.to_rfc3339()),
        );

        self.qdrant
            .upsert(project_slug, &updated.id, embedding, payload)
            .await?;

        debug!(id = %memory_id, "Updated memory");

        Ok(updated)
    }

    /// Delete a memory.
    pub async fn delete(
        &self,
        project_id: &str,
        project_slug: &str,
        memory_id: &str,
    ) -> Result<()> {
        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

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

        // Delete from fold/
        if let Err(e) = self
            .fold_storage
            .delete_memory(&project_root, memory_id)
            .await
        {
            warn!(error = %e, memory_id = %memory_id, "Failed to delete memory file (may not exist)");
        }

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

    // =========================================================================
    // Search Methods
    // =========================================================================

    /// Simple search without memory type filter.
    ///
    /// Convenience method for API compatibility.
    pub async fn search(
        &self,
        project_id: &str,
        project_slug: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        self.search_with_type(project_id, project_slug, query, None, limit)
            .await
    }

    /// Search memories using semantic similarity with optional type filter.
    ///
    /// Applies decay-weighted scoring: blends vector similarity with memory strength
    /// based on recency and access frequency (ACT-R inspired decay model).
    pub async fn search_with_type(
        &self,
        project_id: &str,
        project_slug: &str,
        query: &str,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        // Generate query embedding using search-priority providers
        let embedding = self.embeddings.embed_single_for_search(query).await?;

        // Build filter
        let filter = memory_type.map(|mt| SearchFilter::new().with_type(mt.as_str()));

        // Search in Qdrant - fetch more results to allow for re-ranking
        let fetch_limit = (limit * 2).min(100);
        let vector_results = self
            .qdrant
            .search(project_slug, embedding, fetch_limit, filter)
            .await?;

        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

        // Get decay config from project settings (or use defaults)
        let half_life = project
            .decay_half_life_days
            .unwrap_or(DEFAULT_HALF_LIFE_DAYS);
        let strength_weight = project
            .decay_strength_weight
            .unwrap_or(DEFAULT_STRENGTH_WEIGHT);

        let mut results = Vec::with_capacity(vector_results.len());
        for vr in vector_results {
            let mut memory = match self.get_without_tracking(project_id, &vr.id).await? {
                Some(m) => m,
                None => continue,
            };

            // Resolve content based on source
            // File/Git memories: content already in SQLite
            // Agent memories: read from fold/
            if memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty()) {
                match self.fold_storage.read_memory(&project_root, &vr.id).await {
                    Ok((_, content)) => {
                        memory.content = Some(content);
                    }
                    Err(e) => {
                        debug!(
                            memory_id = %vr.id,
                            project_root = %project_root.display(),
                            error = %e,
                            "Failed to read memory content from fold/"
                        );
                    }
                }
            }

            // Calculate decay-adjusted strength
            let strength = calculate_strength(
                memory.updated_at,
                memory.last_accessed,
                memory.retrieval_count,
                half_life,
            );

            // Blend semantic relevance with retrieval strength
            let combined_score = blend_scores(vr.score as f64, strength, strength_weight);

            results.push(MemorySearchResult::with_decay(
                memory,
                vr.score,
                strength as f32,
                combined_score as f32,
            ));
        }

        // Re-rank by combined score (decay-weighted)
        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to requested limit
        results.truncate(limit);

        // Update access tracking for returned results
        self.track_search_access(&results).await;

        Ok(results)
    }

    /// Enhanced search that queries both memories and code chunks.
    ///
    /// This method first searches memories (like search_with_type), then also
    /// searches code chunks stored in Qdrant. Matched chunks are attached to
    /// their parent memories, and memories found via chunk matches are included
    /// in results even if the memory itself didn't match directly.
    pub async fn search_with_chunks(
        &self,
        project_id: &str,
        project_slug: &str,
        query: &str,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        // Generate query embedding using search-priority providers
        let embedding = self.embeddings.embed_single_for_search(query).await?;

        // Build filter for memories
        let memory_filter = memory_type.map(|mt| SearchFilter::new().with_type(mt.as_str()));

        // Search memories - fetch more to allow for re-ranking
        let fetch_limit = (limit * 2).min(100);
        let memory_results = self
            .qdrant
            .search(project_slug, embedding.clone(), fetch_limit, memory_filter)
            .await?;

        // Search chunks - use type="chunk" filter
        let chunk_filter = Some(SearchFilter::new().with_type("chunk"));
        let chunk_results = self
            .qdrant
            .search(project_slug, embedding, fetch_limit, chunk_filter)
            .await?;

        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

        // Get decay config
        let half_life = project
            .decay_half_life_days
            .unwrap_or(DEFAULT_HALF_LIFE_DAYS);
        let strength_weight = project
            .decay_strength_weight
            .unwrap_or(DEFAULT_STRENGTH_WEIGHT);

        // Collect matched chunks by parent_memory_id
        let mut chunks_by_memory: HashMap<String, Vec<ChunkMatch>> = HashMap::new();

        for cr in &chunk_results {
            // Get parent_memory_id from payload
            let parent_id = cr
                .payload
                .get("parent_memory_id")
                .and_then(|v| v.as_str())
                .map(String::from);

            if let Some(parent_id) = parent_id {
                // Get chunk details from SQLite
                let chunk = db::get_chunk(&self.db, &cr.id).await?;

                if let Some(chunk) = chunk {
                    let snippet = if chunk.content.len() > 100 {
                        let boundary = floor_char_boundary(&chunk.content, 100);
                        Some(format!("{}...", &chunk.content[..boundary]))
                    } else {
                        Some(chunk.content.clone())
                    };

                    let chunk_match = ChunkMatch {
                        id: chunk.id,
                        node_type: chunk.node_type,
                        node_name: chunk.node_name,
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        score: cr.score,
                        snippet,
                    };

                    chunks_by_memory
                        .entry(parent_id)
                        .or_default()
                        .push(chunk_match);
                }
            }
        }

        // Sort chunks by score within each memory
        for chunks in chunks_by_memory.values_mut() {
            chunks.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Build results - start with direct memory matches
        let mut results_map: HashMap<String, MemorySearchResult> = HashMap::new();

        for vr in &memory_results {
            let mut memory = match self.get_without_tracking(project_id, &vr.id).await? {
                Some(m) => m,
                None => continue,
            };

            // Resolve content if not already in SQLite (agent memories)
            if memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty()) {
                if let Ok((_, content)) = self.fold_storage.read_memory(&project_root, &vr.id).await
                {
                    memory.content = Some(content);
                }
            }

            let strength = calculate_strength(
                memory.updated_at,
                memory.last_accessed,
                memory.retrieval_count,
                half_life,
            );

            let combined_score = blend_scores(vr.score as f64, strength, strength_weight);

            // Attach any matched chunks
            let matched_chunks = chunks_by_memory.remove(&vr.id).unwrap_or_default();

            results_map.insert(
                vr.id.clone(),
                MemorySearchResult::with_chunks(
                    memory,
                    vr.score,
                    strength as f32,
                    combined_score as f32,
                    matched_chunks,
                ),
            );
        }

        // Add memories found via chunk matches that weren't in direct results
        for (memory_id, matched_chunks) in chunks_by_memory {
            if results_map.contains_key(&memory_id) {
                continue;
            }

            let mut memory = match self.get_without_tracking(project_id, &memory_id).await? {
                Some(m) => m,
                None => continue,
            };

            // Resolve content if not already in SQLite (agent memories)
            if memory.content.is_none() || memory.content.as_ref().is_some_and(|c| c.is_empty()) {
                if let Ok((_, content)) = self
                    .fold_storage
                    .read_memory(&project_root, &memory_id)
                    .await
                {
                    memory.content = Some(content);
                }
            }

            let strength = calculate_strength(
                memory.updated_at,
                memory.last_accessed,
                memory.retrieval_count,
                half_life,
            );

            // Use best chunk score as the memory's relevance score
            let best_chunk_score = matched_chunks.first().map(|c| c.score).unwrap_or(0.0);
            let combined_score = blend_scores(best_chunk_score as f64, strength, strength_weight);

            results_map.insert(
                memory_id,
                MemorySearchResult::with_chunks(
                    memory,
                    best_chunk_score,
                    strength as f32,
                    combined_score as f32,
                    matched_chunks,
                ),
            );
        }

        // Convert to vec and sort by combined score
        let mut results: Vec<_> = results_map.into_values().collect();
        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to limit
        results.truncate(limit);

        // Update access tracking
        self.track_search_access(&results).await;

        Ok(results)
    }

    /// Agentic search with link traversal - follows relationships for holographic retrieval.
    pub async fn search_agentic(
        &self,
        project_id: &str,
        project_slug: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<AgenticSearchResult>> {
        // Generate query embedding using search-priority providers
        let embedding = self.embeddings.embed_single_for_search(query).await?;

        // Search Qdrant
        let qdrant_results = self
            .qdrant
            .search(project_slug, embedding, limit, None)
            .await?;

        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

        let mut results = Vec::new();
        let mut seen_ids = HashSet::new();

        // Process direct matches
        for result in &qdrant_results {
            if seen_ids.contains(&result.id) {
                continue;
            }

            let memory = match self.get_without_tracking(project_id, &result.id).await? {
                Some(m) => m,
                None => continue,
            };

            let content = self
                .fold_storage
                .read_memory(&project_root, &result.id)
                .await
                .map(|(_, c)| c)
                .unwrap_or_default();

            seen_ids.insert(result.id.clone());
            results.push(AgenticSearchResult {
                memory: memory.clone(),
                content,
                score: result.score,
                is_neighbour: false,
            });

            // Follow links to include neighbours (holographic property)
            let linked_ids = self.get_linked_memory_ids(&result.id).await?;
            for link_id in linked_ids {
                if seen_ids.contains(&link_id) || results.len() >= limit * 2 {
                    continue;
                }

                if let Ok(Some(neighbour)) = self.get_without_tracking(project_id, &link_id).await {
                    if let Ok((_, neighbour_content)) =
                        self.fold_storage.read_memory(&project_root, &link_id).await
                    {
                        seen_ids.insert(link_id.clone());
                        results.push(AgenticSearchResult {
                            memory: neighbour,
                            content: neighbour_content,
                            score: result.score * 0.8, // Slightly lower score for neighbours
                            is_neighbour: true,
                        });
                    }
                }
            }
        }

        // Sort by score and limit
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    /// Get linked memory IDs for a memory.
    async fn get_linked_memory_ids(&self, memory_id: &str) -> Result<Vec<String>> {
        let links: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT target_id FROM memory_links WHERE source_id = ?
            UNION
            SELECT source_id FROM memory_links WHERE target_id = ?
            "#,
        )
        .bind(memory_id)
        .bind(memory_id)
        .fetch_all(&self.db)
        .await?;

        Ok(links.into_iter().map(|(id,)| id).collect())
    }

    /// Update access tracking for memories returned in search results.
    async fn track_search_access(&self, results: &[MemorySearchResult]) {
        if results.is_empty() {
            return;
        }

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

    // =========================================================================
    // Context Reconstruction
    // =========================================================================

    /// Get holographic context around a memory.
    ///
    /// Performs BFS through links to reconstruct related context,
    /// and includes vector-similar memories not explicitly linked.
    pub async fn get_context(
        &self,
        project_id: &str,
        memory_id: &str,
        depth: usize,
    ) -> Result<ContextResponse> {
        let memory = self
            .get(project_id, memory_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Memory {}", memory_id)))?;

        let content = memory.content.clone().unwrap_or_default();

        let project = crate::db::get_project(&self.db, project_id).await?;
        let project_root = std::path::PathBuf::from(&project.root_path);

        let mut related = Vec::new();
        let mut visited = HashSet::new();
        visited.insert(memory_id.to_string());

        // BFS through links
        let initial_links = self.get_linked_memory_ids(memory_id).await?;
        let mut queue: Vec<(String, usize)> = initial_links.into_iter().map(|id| (id, 1)).collect();

        while let Some((id, current_depth)) = queue.pop() {
            if visited.contains(&id) || current_depth > depth {
                continue;
            }
            visited.insert(id.clone());

            if let Ok(Some(related_memory)) = self.get_without_tracking(project_id, &id).await {
                if let Ok((_, related_content)) =
                    self.fold_storage.read_memory(&project_root, &id).await
                {
                    // Add links to queue for next depth
                    if current_depth < depth {
                        let next_links = self.get_linked_memory_ids(&id).await?;
                        for link_id in next_links {
                            if !visited.contains(&link_id) {
                                queue.push((link_id, current_depth + 1));
                            }
                        }
                    }

                    related.push(MemoryWithContent {
                        memory: related_memory,
                        content: related_content,
                    });
                }
            }
        }

        // Also add vector-similar memories not explicitly linked
        let embedding = self.embeddings.embed_single(&content).await?;
        let similar = self
            .qdrant
            .search(&project.slug, embedding, 5, None)
            .await?;

        for result in similar {
            if !visited.contains(&result.id) {
                if let Ok(Some(sim_memory)) =
                    self.get_without_tracking(project_id, &result.id).await
                {
                    if let Ok((_, sim_content)) = self
                        .fold_storage
                        .read_memory(&project_root, &result.id)
                        .await
                    {
                        related.push(MemoryWithContent {
                            memory: sim_memory,
                            content: sim_content,
                        });
                    }
                }
            }
        }

        Ok(ContextResponse {
            memory,
            content,
            related,
            depth,
        })
    }

    // =========================================================================
    // Legacy Compatibility Methods
    // =========================================================================

    /// Get context for a task (legacy compatibility).
    pub async fn get_context_for_task(
        &self,
        project_id: &str,
        project_slug: &str,
        task: &str,
        types: Option<Vec<MemoryType>>,
        limit: usize,
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
            let results = self
                .search_with_type(
                    project_id,
                    project_slug,
                    task,
                    Some(memory_type),
                    per_type_limit,
                )
                .await?;

            for result in results {
                let item = ContextItem {
                    id: result.memory.id.clone(),
                    title: result.memory.title.clone(),
                    content: result
                        .memory
                        .content
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(500)
                        .collect(),
                    score: result.score,
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
    pub async fn delete_all_for_project(
        &self,
        project_id: &str,
        project_slug: &str,
    ) -> Result<u64> {
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

    /// Insert or update memory in SQLite.
    /// Uses upsert to handle codebase files that may be re-indexed with the same path-based ID.
    async fn insert_memory(&self, memory: &Memory) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO memories (
                id, project_id, slug, type, source, content, content_hash, content_storage,
                title, author, keywords, tags, context, file_path, language,
                line_start, line_end, status, assignee, metadata,
                created_at, updated_at, retrieval_count, last_accessed
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
            )
            ON CONFLICT(id) DO UPDATE SET
                slug = excluded.slug,
                content = excluded.content,
                content_hash = excluded.content_hash,
                title = excluded.title,
                author = excluded.author,
                keywords = excluded.keywords,
                tags = excluded.tags,
                context = excluded.context,
                metadata = excluded.metadata,
                updated_at = datetime('now')
            "#,
        )
        .bind(&memory.id)
        .bind(&memory.project_id)
        .bind(&memory.slug)
        .bind(&memory.memory_type)
        .bind(&memory.source)
        .bind(&memory.content)
        .bind(&memory.content_hash)
        .bind(&memory.content_storage)
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

    /// Resolve content for a list of memories.
    pub async fn resolve_content_for_memories(
        &self,
        mut memories: Vec<Memory>,
        _project_slug: &str,
        _project_root_path: Option<&str>,
    ) -> Result<Vec<Memory>> {
        // Get project to find root path
        if let Some(first) = memories.first() {
            if let Ok(project) = crate::db::get_project(&self.db, &first.project_id).await {
                let project_root = std::path::PathBuf::from(&project.root_path);
                for memory in &mut memories {
                    if memory.content.is_none()
                        || memory.content.as_ref().is_some_and(|c| c.is_empty())
                    {
                        if let Ok((_, content)) = self
                            .fold_storage
                            .read_memory(&project_root, &memory.id)
                            .await
                        {
                            memory.content = Some(content);
                        }
                    }
                }
            }
        }
        Ok(memories)
    }
}

/// Context gathered for a task (legacy compatibility).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextResult {
    pub task: String,
    pub code: Vec<ContextItem>,
    pub specs: Vec<ContextItem>,
    pub decisions: Vec<ContextItem>,
    pub sessions: Vec<ContextItem>,
    pub other: Vec<ContextItem>,
}

/// A single context item (legacy compatibility).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextItem {
    pub id: String,
    pub title: Option<String>,
    pub content: String,
    pub score: f32,
    pub file_path: Option<String>,
    pub author: Option<String>,
}
