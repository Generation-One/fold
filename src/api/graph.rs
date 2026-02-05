//! Graph Routes
//!
//! Knowledge graph exploration and traversal endpoints.
//!
//! Routes:
//! - GET /projects/:project_id/graph - Get graph for project
//! - GET /projects/:project_id/graph/stats - Get graph statistics
//! - GET /projects/:project_id/graph/neighbors/:id - Get neighboring nodes
//! - POST /projects/:project_id/graph/cluster - Find memory clusters
//! - POST /projects/:project_id/graph/path - Find path between nodes
//! - GET /projects/:project_id/graph/context/:id - Get context for a memory
//! - POST /projects/:project_id/graph/impact - Analyze impact of changes

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::memories::MemoryType;
use crate::models::LinkType;
use crate::services::{GraphResult, GraphStats, ImpactAnalysis, MemoryContext};
use crate::{db, AppState, Error, Result};

/// Build graph routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_graph))
        .route("/stats", get(get_stats))
        .route("/neighbors/:node_id", get(get_neighbors))
        .route("/context/:node_id", get(get_context))
        .route("/cluster", post(find_clusters))
        .route("/path", post(find_path))
        .route("/impact", post(analyze_impact))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Query parameters for neighbor retrieval.
#[derive(Debug, Deserialize, Default)]
pub struct NeighborsQuery {
    /// Maximum depth of traversal (default 1)
    #[serde(default = "default_depth")]
    pub depth: u32,
    /// Maximum neighbors to return per level
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Filter by edge types
    #[serde(default)]
    pub edge_types: Vec<EdgeType>,
    /// Filter by node types
    #[serde(default)]
    pub node_types: Vec<MemoryType>,
}

fn default_depth() -> u32 {
    1
}

fn default_limit() -> u32 {
    20
}

/// Types of relationships between nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Explicit reference/link
    References,
    /// Semantic similarity
    Similar,
    /// Same file or code block
    SameFile,
    /// Same author
    SameAuthor,
    /// Temporal proximity (created close together)
    Temporal,
    /// Part of same session
    SameSession,
    /// Implementation relationship
    Implements,
    /// Derived from another memory
    DerivedFrom,
}

/// Request for cluster finding.
#[derive(Debug, Deserialize)]
pub struct ClusterRequest {
    /// Minimum cluster size
    #[serde(default = "default_min_cluster_size")]
    pub min_size: u32,
    /// Maximum number of clusters to return
    #[serde(default = "default_max_clusters")]
    pub max_clusters: u32,
    /// Filter by memory types
    #[serde(default)]
    pub types: Vec<MemoryType>,
    /// Algorithm to use
    #[serde(default)]
    pub algorithm: ClusterAlgorithm,
}

fn default_min_cluster_size() -> u32 {
    3
}

fn default_max_clusters() -> u32 {
    10
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClusterAlgorithm {
    #[default]
    KMeans,
    DBSCAN,
    Hierarchical,
}

/// Request for path finding.
#[derive(Debug, Deserialize)]
pub struct PathRequest {
    /// Source node ID
    pub from: Uuid,
    /// Target node ID
    pub to: Uuid,
    /// Maximum path length
    #[serde(default = "default_max_path_length")]
    pub max_length: u32,
    /// Edge types to traverse
    #[serde(default)]
    pub edge_types: Vec<EdgeType>,
}

fn default_max_path_length() -> u32 {
    5
}

/// Query parameters for history retrieval.
#[derive(Debug, Deserialize, Default)]
pub struct HistoryQuery {
    /// Maximum history items
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Include diffs
    #[serde(default)]
    pub include_diffs: bool,
}

/// Request for impact analysis.
#[derive(Debug, Deserialize)]
pub struct ImpactRequest {
    /// Node IDs to analyze impact for
    pub nodes: Vec<Uuid>,
    /// Maximum depth for impact traversal
    #[serde(default = "default_impact_depth")]
    pub depth: u32,
    /// Include code dependencies
    #[serde(default = "default_true")]
    pub include_code: bool,
    /// Include semantic relationships
    #[serde(default = "default_true")]
    pub include_semantic: bool,
}

fn default_impact_depth() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

/// Graph node representation.
#[derive(Debug, Serialize)]
pub struct GraphNode {
    pub id: Uuid,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub node_type: MemoryType,
    pub preview: String,
    pub metadata: NodeMetadata,
}

#[derive(Debug, Serialize)]
pub struct NodeMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Graph edge representation.
#[derive(Debug, Serialize)]
pub struct GraphEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub edge_type: EdgeType,
    pub weight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Neighbors response.
#[derive(Debug, Serialize)]
pub struct NeighborsResponse {
    pub center: GraphNode,
    pub neighbors: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub depth: u32,
}

/// Cluster information.
#[derive(Debug, Serialize)]
pub struct Cluster {
    pub id: u32,
    pub label: Option<String>,
    pub nodes: Vec<GraphNode>,
    pub centroid: Option<Uuid>,
    pub cohesion: f32,
    pub keywords: Vec<String>,
}

/// Cluster response.
#[derive(Debug, Serialize)]
pub struct ClusterResponse {
    pub clusters: Vec<Cluster>,
    pub unclustered: u32,
}

/// Path between nodes.
#[derive(Debug, Serialize)]
pub struct GraphPath {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total_weight: f32,
}

/// Path response.
#[derive(Debug, Serialize)]
pub struct PathResponse {
    pub from: Uuid,
    pub to: Uuid,
    pub paths: Vec<GraphPath>,
    pub found: bool,
}

/// History entry.
#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub action: HistoryAction,
    pub timestamp: DateTime<Utc>,
    pub author: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    Created,
    Updated,
    Linked,
    Unlinked,
    Tagged,
    Deleted,
}

/// History response.
#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub node_id: Uuid,
    pub entries: Vec<HistoryEntry>,
}

/// Impact analysis result.
#[derive(Debug, Serialize)]
pub struct ImpactItem {
    pub node: GraphNode,
    pub impact_type: ImpactType,
    pub distance: u32,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactType {
    Direct,
    Indirect,
    Potential,
}

/// Impact response.
#[derive(Debug, Serialize)]
pub struct ImpactResponse {
    pub source_nodes: Vec<Uuid>,
    pub impacted: Vec<ImpactItem>,
    pub summary: String,
}

// ============================================================================
// Path Extractors
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProjectPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct NodePath {
    pub project_id: String,
    pub node_id: Uuid,
}

// ============================================================================
// Handlers
// ============================================================================

/// Query parameters for graph traversal.
#[derive(Debug, Deserialize, Default)]
pub struct GraphQuery {
    /// Starting memory ID (optional - if not provided, returns stats only)
    pub start_id: Option<String>,
    /// Maximum depth of traversal (default 2)
    #[serde(default = "default_graph_depth")]
    pub depth: usize,
    /// Filter by link types (comma-separated)
    pub link_types: Option<String>,
}

fn default_graph_depth() -> usize {
    2
}

/// Get graph traversal from a starting point.
///
/// GET /projects/:project_id/graph
#[axum::debug_handler]
async fn get_graph(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<GraphResult>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    let start_id = query.start_id.ok_or_else(|| {
        Error::Validation("start_id query parameter is required".into())
    })?;

    // Parse link types filter
    let link_types = query.link_types.as_ref().map(|lt| {
        lt.split(',')
            .filter_map(|s| LinkType::from_str(s.trim()))
            .collect::<Vec<_>>()
    });

    let depth = query.depth.min(5); // Cap at 5 levels

    let result = state
        .graph
        .traverse(&project.id, &start_id, depth, link_types)
        .await?;

    Ok(Json(result))
}

/// Get graph statistics for a project.
///
/// GET /projects/:project_id/graph/stats
#[axum::debug_handler]
async fn get_stats(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
) -> Result<Json<GraphStats>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    let stats = state.graph.get_stats(&project.id).await?;

    Ok(Json(stats))
}

/// Get neighboring nodes in the graph.
///
/// GET /projects/:project_id/graph/neighbors/:node_id
#[axum::debug_handler]
async fn get_neighbors(
    State(state): State<AppState>,
    Path(path): Path<NodePath>,
    Query(query): Query<NeighborsQuery>,
) -> Result<Json<NeighborsResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;
    let node_id = path.node_id.to_string();
    let depth = query.depth.min(3) as usize;

    // Get the center memory with content resolved
    let center_memory = state
        .memory
        .get(&project.id, &node_id)
        .await?
        .ok_or_else(|| Error::NotFound("Memory not found".into()))?;

    // Traverse to find neighbors
    let graph_result = state
        .graph
        .traverse(&project.id, &node_id, depth, None)
        .await?;

    // Convert to response format
    let created_at = center_memory.created_at;

    let center = GraphNode {
        id: Uuid::parse_str(&center_memory.id).unwrap_or_else(|_| Uuid::new_v4()),
        title: center_memory.title.clone(),
        node_type: parse_memory_type(&center_memory.memory_type),
        preview: center_memory.content.as_deref().unwrap_or("").chars().take(200).collect(),
        metadata: NodeMetadata {
            file_path: center_memory.file_path.clone(),
            author: center_memory.author.clone(),
            tags: center_memory.tags_vec(),
            created_at,
        },
    };

    let neighbors: Vec<GraphNode> = graph_result
        .nodes
        .iter()
        .filter(|n| n.id != node_id)
        .map(|n| GraphNode {
            id: Uuid::parse_str(&n.id).unwrap_or_else(|_| Uuid::new_v4()),
            title: n.title.clone(),
            node_type: parse_memory_type(&n.memory_type),
            preview: n.content_preview.clone(),
            metadata: NodeMetadata {
                file_path: n.file_path.clone(),
                author: None,
                tags: vec![],
                created_at: Utc::now(), // Placeholder
            },
        })
        .collect();

    let edges: Vec<GraphEdge> = graph_result
        .edges
        .iter()
        .map(|e| GraphEdge {
            from: Uuid::parse_str(&e.source_id).unwrap_or_else(|_| Uuid::new_v4()),
            to: Uuid::parse_str(&e.target_id).unwrap_or_else(|_| Uuid::new_v4()),
            edge_type: parse_edge_type(&e.link_type),
            weight: e.confidence.unwrap_or(1.0) as f32,
            label: e.context.clone(),
        })
        .collect();

    Ok(Json(NeighborsResponse {
        center,
        neighbors,
        edges,
        depth: query.depth,
    }))
}

/// Get context for a memory node.
///
/// GET /projects/:project_id/graph/context/:node_id
#[axum::debug_handler]
async fn get_context(
    State(state): State<AppState>,
    Path(path): Path<NodePath>,
) -> Result<Json<MemoryContext>> {
    let node_id = path.node_id.to_string();

    let context = state.graph.get_context(&node_id).await?;

    Ok(Json(context))
}

/// Find clusters of related memories.
///
/// POST /projects/:project_id/graph/cluster
#[axum::debug_handler]
async fn find_clusters(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ClusterRequest>,
) -> Result<Json<ClusterResponse>> {
    let _project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    // Clustering is computationally expensive and requires Qdrant vectors
    // For now, return empty - full implementation would:
    // 1. Fetch all embeddings from Qdrant for the project
    // 2. Run k-means or DBSCAN clustering
    // 3. Label clusters using LLM

    Ok(Json(ClusterResponse {
        clusters: vec![],
        unclustered: 0,
    }))
}

/// Find path between two nodes.
///
/// POST /projects/:project_id/graph/path
#[axum::debug_handler]
async fn find_path(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<PathRequest>,
) -> Result<Json<PathResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    let from_id = request.from.to_string();
    let to_id = request.to.to_string();
    let max_depth = request.max_length.min(10) as usize;

    let paths = state
        .graph
        .find_paths(&project.id, &from_id, &to_id, max_depth)
        .await?;

    let graph_paths: Vec<GraphPath> = paths
        .into_iter()
        .map(|path_edges| {
            let total_weight: f32 = path_edges
                .iter()
                .map(|e| e.confidence.unwrap_or(1.0) as f32)
                .sum();

            let edges: Vec<GraphEdge> = path_edges
                .iter()
                .map(|e| GraphEdge {
                    from: Uuid::parse_str(&e.source_id).unwrap_or_else(|_| Uuid::new_v4()),
                    to: Uuid::parse_str(&e.target_id).unwrap_or_else(|_| Uuid::new_v4()),
                    edge_type: parse_edge_type(&e.link_type),
                    weight: e.confidence.unwrap_or(1.0) as f32,
                    label: e.context.clone(),
                })
                .collect();

            GraphPath {
                nodes: vec![], // Would need to fetch actual node data
                edges,
                total_weight,
            }
        })
        .collect();

    let found = !graph_paths.is_empty();

    Ok(Json(PathResponse {
        from: request.from,
        to: request.to,
        paths: graph_paths,
        found,
    }))
}

/// Analyze impact of changes to nodes.
///
/// POST /projects/:project_id/graph/impact
#[axum::debug_handler]
async fn analyze_impact(
    State(state): State<AppState>,
    Path(path): Path<ProjectPath>,
    Json(request): Json<ImpactRequest>,
) -> Result<Json<ImpactResponse>> {
    let project = db::get_project_by_id_or_slug(&state.db, &path.project_id).await?;

    if request.nodes.is_empty() {
        return Err(Error::Validation("At least one node is required".into()));
    }

    let depth = request.depth.min(5) as usize;
    let mut all_impacted: Vec<ImpactItem> = Vec::new();

    for node_id in &request.nodes {
        let analysis = state
            .graph
            .analyze_impact(&project.id, &node_id.to_string(), depth)
            .await?;

        // Convert directly affected
        for affected in analysis.directly_affected {
            all_impacted.push(ImpactItem {
                node: GraphNode {
                    id: Uuid::parse_str(&affected.id).unwrap_or_else(|_| Uuid::new_v4()),
                    title: affected.title,
                    node_type: parse_memory_type(&affected.memory_type),
                    preview: String::new(),
                    metadata: NodeMetadata {
                        file_path: affected.file_path,
                        author: None,
                        tags: vec![],
                        created_at: Utc::now(),
                    },
                },
                impact_type: ImpactType::Direct,
                distance: affected.depth as u32,
                confidence: 0.9,
                reason: format!("Directly linked via {}", affected.impact_type),
            });
        }

        // Convert indirectly affected
        for affected in analysis.indirectly_affected {
            all_impacted.push(ImpactItem {
                node: GraphNode {
                    id: Uuid::parse_str(&affected.id).unwrap_or_else(|_| Uuid::new_v4()),
                    title: affected.title,
                    node_type: parse_memory_type(&affected.memory_type),
                    preview: String::new(),
                    metadata: NodeMetadata {
                        file_path: affected.file_path,
                        author: None,
                        tags: vec![],
                        created_at: Utc::now(),
                    },
                },
                impact_type: ImpactType::Indirect,
                distance: affected.depth as u32,
                confidence: 0.7,
                reason: format!("Indirectly linked via {}", affected.impact_type),
            });
        }
    }

    let summary = if all_impacted.is_empty() {
        "No impacted items found".to_string()
    } else {
        let direct_count = all_impacted.iter().filter(|i| i.impact_type == ImpactType::Direct).count();
        let indirect_count = all_impacted.len() - direct_count;
        format!(
            "Found {} directly affected and {} indirectly affected items",
            direct_count, indirect_count
        )
    };

    Ok(Json(ImpactResponse {
        source_nodes: request.nodes,
        impacted: all_impacted,
        summary,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse edge type from string.
fn parse_edge_type(s: &str) -> EdgeType {
    match s.to_lowercase().as_str() {
        "references" => EdgeType::References,
        "similar" => EdgeType::Similar,
        "same_file" => EdgeType::SameFile,
        "same_author" => EdgeType::SameAuthor,
        "temporal" => EdgeType::Temporal,
        "same_session" => EdgeType::SameSession,
        "implements" => EdgeType::Implements,
        "derived_from" => EdgeType::DerivedFrom,
        _ => EdgeType::References,
    }
}

/// Parse memory type from string.
fn parse_memory_type(s: &str) -> MemoryType {
    match s.to_lowercase().as_str() {
        "codebase" => MemoryType::Codebase,
        "session" => MemoryType::Session,
        "spec" => MemoryType::Spec,
        "decision" => MemoryType::Decision,
        "task" => MemoryType::Task,
        _ => MemoryType::General,
    }
}
