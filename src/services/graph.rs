//! Graph service for relationship queries.
//!
//! Provides traversal and analysis of the memory knowledge graph,
//! including context gathering and impact analysis.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::db::DbPool;
use crate::error::Result;
use crate::models::{LinkType, Memory, MemoryLink};

/// Service for querying the memory knowledge graph.
#[derive(Clone)]
pub struct GraphService {
    db: DbPool,
}

/// A node in the graph result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub memory_type: String,
    pub title: Option<String>,
    pub content_preview: String,
    pub file_path: Option<String>,
    pub depth: usize,
}

/// An edge in the graph result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source_id: String,
    pub target_id: String,
    pub link_type: String,
    pub confidence: Option<f64>,
    pub context: Option<String>,
}

/// Graph traversal result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResult {
    pub root_id: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Impact analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub source_id: String,
    pub directly_affected: Vec<AffectedMemory>,
    pub indirectly_affected: Vec<AffectedMemory>,
    pub total_impact_score: f64,
}

/// An affected memory in impact analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedMemory {
    pub id: String,
    pub memory_type: String,
    pub title: Option<String>,
    pub file_path: Option<String>,
    pub impact_type: String,
    pub depth: usize,
}

/// Context for a memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryContext {
    pub memory_id: String,
    pub related: Vec<RelatedMemory>,
    pub parents: Vec<RelatedMemory>,
    pub children: Vec<RelatedMemory>,
    pub commits: Vec<RelatedMemory>,
    pub specs: Vec<RelatedMemory>,
    pub decisions: Vec<RelatedMemory>,
}

/// A related memory with relationship info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedMemory {
    pub id: String,
    pub memory_type: String,
    pub title: Option<String>,
    pub content_preview: String,
    pub link_type: String,
    pub confidence: Option<f64>,
}

impl GraphService {
    /// Create a new graph service.
    pub fn new(db: DbPool) -> Self {
        Self { db }
    }

    /// Get all links for a memory (both incoming and outgoing).
    pub async fn get_links(&self, memory_id: &str) -> Result<Vec<MemoryLink>> {
        let links = sqlx::query_as::<_, MemoryLink>(
            r#"
            SELECT * FROM memory_links
            WHERE source_id = ? OR target_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(memory_id)
        .bind(memory_id)
        .fetch_all(&self.db)
        .await?;

        Ok(links)
    }

    /// Get outgoing links from a memory.
    pub async fn get_outgoing_links(&self, memory_id: &str) -> Result<Vec<MemoryLink>> {
        let links = sqlx::query_as::<_, MemoryLink>(
            r#"
            SELECT * FROM memory_links
            WHERE source_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(memory_id)
        .fetch_all(&self.db)
        .await?;

        Ok(links)
    }

    /// Get incoming links to a memory.
    pub async fn get_incoming_links(&self, memory_id: &str) -> Result<Vec<MemoryLink>> {
        let links = sqlx::query_as::<_, MemoryLink>(
            r#"
            SELECT * FROM memory_links
            WHERE target_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(memory_id)
        .fetch_all(&self.db)
        .await?;

        Ok(links)
    }

    /// Traverse the graph from a starting memory.
    pub async fn traverse(
        &self,
        _project_id: &str,
        start_id: &str,
        max_depth: usize,
        link_types: Option<Vec<LinkType>>,
    ) -> Result<GraphResult> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut nodes: Vec<GraphNode> = Vec::new();
        let mut edges: Vec<GraphEdge> = Vec::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        queue.push_back((start_id.to_string(), 0));
        visited.insert(start_id.to_string());

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth > max_depth {
                continue;
            }

            // Get the memory
            if let Some(memory) = self.get_memory(&current_id).await? {
                nodes.push(GraphNode {
                    id: memory.id.clone(),
                    memory_type: memory.memory_type.clone(),
                    title: memory.title.clone(),
                    content_preview: memory.content.as_deref().unwrap_or("").chars().take(200).collect(),
                    file_path: memory.file_path.clone(),
                    depth,
                });
            }

            // Get links
            let links = self.get_links(&current_id).await?;

            for link in links {
                // Filter by link type if specified
                if let Some(ref types) = link_types {
                    if let Some(lt) = LinkType::from_str(&link.link_type) {
                        if !types.contains(&lt) {
                            continue;
                        }
                    }
                }

                edges.push(GraphEdge {
                    source_id: link.source_id.clone(),
                    target_id: link.target_id.clone(),
                    link_type: link.link_type.clone(),
                    confidence: link.confidence,
                    context: link.context.clone(),
                });

                // Add connected nodes to queue
                let next_id = if link.source_id == current_id {
                    &link.target_id
                } else {
                    &link.source_id
                };

                if !visited.contains(next_id) && depth < max_depth {
                    visited.insert(next_id.clone());
                    queue.push_back((next_id.clone(), depth + 1));
                }
            }
        }

        Ok(GraphResult {
            root_id: start_id.to_string(),
            nodes,
            edges,
        })
    }

    /// Get context for a memory (related items organized by type).
    pub async fn get_context(&self, memory_id: &str) -> Result<MemoryContext> {
        let links = self.get_links(memory_id).await?;

        let mut context = MemoryContext {
            memory_id: memory_id.to_string(),
            related: Vec::new(),
            parents: Vec::new(),
            children: Vec::new(),
            commits: Vec::new(),
            specs: Vec::new(),
            decisions: Vec::new(),
        };

        for link in links {
            let related_id = if link.source_id == memory_id {
                &link.target_id
            } else {
                &link.source_id
            };

            if let Some(memory) = self.get_memory(related_id).await? {
                let related = RelatedMemory {
                    id: memory.id.clone(),
                    memory_type: memory.memory_type.clone(),
                    title: memory.title.clone(),
                    content_preview: memory.content.as_deref().unwrap_or("").chars().take(200).collect(),
                    link_type: link.link_type.clone(),
                    confidence: link.confidence,
                };

                // Categorize by link type and memory type
                match link.link_type.as_str() {
                    "parent" => {
                        if link.source_id == memory_id {
                            context.children.push(related);
                        } else {
                            context.parents.push(related);
                        }
                    }
                    "contains" => {
                        if link.source_id == memory_id {
                            context.children.push(related);
                        } else {
                            context.parents.push(related);
                        }
                    }
                    _ => {
                        // Categorize by memory type
                        match memory.memory_type.as_str() {
                            "commit" => context.commits.push(related),
                            "spec" => context.specs.push(related),
                            "decision" => context.decisions.push(related),
                            _ => context.related.push(related),
                        }
                    }
                }
            }
        }

        Ok(context)
    }

    /// Analyze impact of changing a memory.
    pub async fn analyze_impact(
        &self,
        _project_id: &str,
        memory_id: &str,
        max_depth: usize,
    ) -> Result<ImpactAnalysis> {
        let mut directly_affected: Vec<AffectedMemory> = Vec::new();
        let mut indirectly_affected: Vec<AffectedMemory> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize, String)> = VecDeque::new();

        visited.insert(memory_id.to_string());

        // Get initial outgoing links (things that depend on this memory)
        let initial_links = self.get_outgoing_links(memory_id).await?;
        for link in initial_links {
            if !visited.contains(&link.target_id) {
                visited.insert(link.target_id.clone());
                queue.push_back((link.target_id.clone(), 1, link.link_type.clone()));
            }
        }

        // Also get things that reference this memory
        let incoming_links = self.get_incoming_links(memory_id).await?;
        for link in incoming_links {
            // These are affected if they depend on this memory
            if matches!(
                link.link_type.as_str(),
                "implements" | "references" | "depends_on" | "extends"
            ) {
                if !visited.contains(&link.source_id) {
                    visited.insert(link.source_id.clone());
                    queue.push_back((link.source_id.clone(), 1, link.link_type.clone()));
                }
            }
        }

        while let Some((current_id, depth, impact_type)) = queue.pop_front() {
            if depth > max_depth {
                continue;
            }

            if let Some(memory) = self.get_memory(&current_id).await? {
                let affected = AffectedMemory {
                    id: memory.id.clone(),
                    memory_type: memory.memory_type.clone(),
                    title: memory.title.clone(),
                    file_path: memory.file_path.clone(),
                    impact_type: impact_type.clone(),
                    depth,
                };

                if depth == 1 {
                    directly_affected.push(affected);
                } else {
                    indirectly_affected.push(affected);
                }

                // Continue traversal
                let links = self.get_outgoing_links(&current_id).await?;
                for link in links {
                    if !visited.contains(&link.target_id) {
                        visited.insert(link.target_id.clone());
                        queue.push_back((link.target_id.clone(), depth + 1, link.link_type.clone()));
                    }
                }
            }
        }

        // Calculate impact score (simple heuristic)
        let direct_weight = directly_affected.len() as f64 * 1.0;
        let indirect_weight = indirectly_affected.len() as f64 * 0.5;
        let total_impact_score = direct_weight + indirect_weight;

        Ok(ImpactAnalysis {
            source_id: memory_id.to_string(),
            directly_affected,
            indirectly_affected,
            total_impact_score,
        })
    }

    /// Find paths between two memories.
    pub async fn find_paths(
        &self,
        _project_id: &str,
        from_id: &str,
        to_id: &str,
        max_depth: usize,
    ) -> Result<Vec<Vec<GraphEdge>>> {
        let mut all_paths: Vec<Vec<GraphEdge>> = Vec::new();
        let mut current_path: Vec<GraphEdge> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        self.find_paths_recursive(
            from_id,
            to_id,
            max_depth,
            &mut visited,
            &mut current_path,
            &mut all_paths,
        )
        .await?;

        Ok(all_paths)
    }

    /// Recursive path finding helper.
    async fn find_paths_recursive(
        &self,
        current: &str,
        target: &str,
        remaining_depth: usize,
        visited: &mut HashSet<String>,
        current_path: &mut Vec<GraphEdge>,
        all_paths: &mut Vec<Vec<GraphEdge>>,
    ) -> Result<()> {
        if current == target {
            all_paths.push(current_path.clone());
            return Ok(());
        }

        if remaining_depth == 0 {
            return Ok(());
        }

        visited.insert(current.to_string());

        let links = self.get_links(current).await?;
        for link in links {
            let next = if link.source_id == current {
                &link.target_id
            } else {
                &link.source_id
            };

            if !visited.contains(next) {
                let edge = GraphEdge {
                    source_id: link.source_id.clone(),
                    target_id: link.target_id.clone(),
                    link_type: link.link_type.clone(),
                    confidence: link.confidence,
                    context: link.context.clone(),
                };

                current_path.push(edge);

                Box::pin(self.find_paths_recursive(
                    next,
                    target,
                    remaining_depth - 1,
                    visited,
                    current_path,
                    all_paths,
                ))
                .await?;

                current_path.pop();
            }
        }

        visited.remove(current);
        Ok(())
    }

    /// Get project graph statistics.
    pub async fn get_stats(&self, project_id: &str) -> Result<GraphStats> {
        let total_nodes: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM memories WHERE project_id = ?"#,
        )
        .bind(project_id)
        .fetch_one(&self.db)
        .await?;

        let total_edges: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM memory_links WHERE project_id = ?"#,
        )
        .bind(project_id)
        .fetch_one(&self.db)
        .await?;

        let edge_type_counts: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT link_type, COUNT(*) as count
            FROM memory_links
            WHERE project_id = ?
            GROUP BY link_type
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        let most_connected: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT m.id, COUNT(DISTINCT ml.id) as link_count
            FROM memories m
            LEFT JOIN memory_links ml ON m.id = ml.source_id OR m.id = ml.target_id
            WHERE m.project_id = ?
            GROUP BY m.id
            ORDER BY link_count DESC
            LIMIT 10
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        Ok(GraphStats {
            total_nodes: total_nodes as usize,
            total_edges: total_edges as usize,
            edges_by_type: edge_type_counts.into_iter().collect(),
            most_connected,
            avg_connections: if total_nodes > 0 {
                (total_edges as f64 * 2.0) / total_nodes as f64
            } else {
                0.0
            },
        })
    }

    /// Get a memory by ID.
    async fn get_memory(&self, id: &str) -> Result<Option<Memory>> {
        let memory = sqlx::query_as::<_, Memory>(
            r#"SELECT * FROM memories WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?;

        Ok(memory)
    }

    /// Get all nodes and edges for a project.
    pub async fn get_all(&self, project_id: &str, limit: usize) -> Result<GraphResult> {
        // Get all memories as nodes (limited)
        let memories: Vec<Memory> = sqlx::query_as(
            r#"
            SELECT * FROM memories
            WHERE project_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(project_id)
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        let memory_ids: std::collections::HashSet<String> =
            memories.iter().map(|m| m.id.clone()).collect();

        let nodes: Vec<GraphNode> = memories
            .into_iter()
            .map(|m| GraphNode {
                id: m.id,
                memory_type: m.memory_type,
                title: m.title,
                content_preview: m.content.as_ref().map(|c| {
                    if c.len() > 100 {
                        format!("{}...", &c[..100])
                    } else {
                        c.clone()
                    }
                }).unwrap_or_default(),
                file_path: m.file_path,
                depth: 0,
            })
            .collect();

        // Get all edges between these nodes
        let all_edges: Vec<MemoryLink> = sqlx::query_as(
            r#"
            SELECT * FROM memory_links
            WHERE project_id = ?
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.db)
        .await?;

        // Filter to only edges where both nodes exist in our set
        let edges: Vec<GraphEdge> = all_edges
            .into_iter()
            .filter(|e| memory_ids.contains(&e.source_id) && memory_ids.contains(&e.target_id))
            .map(|e| GraphEdge {
                source_id: e.source_id,
                target_id: e.target_id,
                link_type: e.link_type,
                confidence: e.confidence,
                context: e.context,
            })
            .collect();

        Ok(GraphResult {
            root_id: String::new(), // Empty for full graph
            nodes,
            edges,
        })
    }
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub edges_by_type: HashMap<String, i64>,
    pub most_connected: Vec<(String, i64)>,
    pub avg_connections: f64,
}
