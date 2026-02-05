//! Memory link database queries.
//!
//! Links form edges in the knowledge graph, connecting memories
//! with typed relationships.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// Types
// ============================================================================

/// Link type enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    Modifies,
    Contains,
    Affects,
    Implements,
    References,
    DependsOn,
    Blocks,
    Related,
    Parent,
    Child,
    Custom(String),
}

impl LinkType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Modifies => "modifies",
            Self::Contains => "contains",
            Self::Affects => "affects",
            Self::Implements => "implements",
            Self::References => "references",
            Self::DependsOn => "depends_on",
            Self::Blocks => "blocks",
            Self::Related => "related",
            Self::Parent => "parent",
            Self::Child => "child",
            Self::Custom(s) => s,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "modifies" => Self::Modifies,
            "contains" => Self::Contains,
            "affects" => Self::Affects,
            "implements" => Self::Implements,
            "references" => Self::References,
            "depends_on" => Self::DependsOn,
            "blocks" => Self::Blocks,
            "related" => Self::Related,
            "parent" => Self::Parent,
            "child" => Self::Child,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Link creator type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkCreator {
    System,
    User,
    Ai,
}

impl LinkCreator {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Ai => "ai",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "system" => Self::System,
            "user" => Self::User,
            "ai" => Self::Ai,
            _ => Self::System,
        }
    }
}

/// Change type for code links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
}

impl ChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "added" => Some(Self::Added),
            "modified" => Some(Self::Modified),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

/// Memory link record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MemoryLink {
    pub id: String,
    pub project_id: String,
    pub source_id: String,
    pub target_id: String,
    pub link_type: String,
    pub created_by: String,
    pub confidence: Option<f64>,
    pub context: Option<String>,
    pub change_type: Option<String>,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
    pub created_at: String,
}

impl MemoryLink {
    /// Get the link type as enum.
    pub fn link_type_enum(&self) -> LinkType {
        LinkType::from_str(&self.link_type)
    }

    /// Get the creator as enum.
    pub fn created_by_enum(&self) -> LinkCreator {
        LinkCreator::from_str(&self.created_by)
    }

    /// Get the change type as enum.
    pub fn change_type_enum(&self) -> Option<ChangeType> {
        self.change_type.as_ref().and_then(|ct| ChangeType::from_str(ct))
    }
}

/// Input for creating a new link.
#[derive(Debug, Clone)]
pub struct CreateLink {
    pub id: String,
    pub project_id: String,
    pub source_id: String,
    pub target_id: String,
    pub link_type: LinkType,
    pub created_by: LinkCreator,
    pub confidence: Option<f64>,
    pub context: Option<String>,
    pub change_type: Option<ChangeType>,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
}

/// Node in a graph traversal result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub memory_id: String,
    pub depth: i32,
    pub path: Vec<String>,
}

// ============================================================================
// Queries
// ============================================================================

/// Create a new memory link.
pub async fn create_link(pool: &DbPool, input: CreateLink) -> Result<MemoryLink> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        INSERT INTO memory_links (
            id, project_id, source_id, target_id, link_type, created_by,
            confidence, context, change_type, additions, deletions
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(&input.source_id)
    .bind(&input.target_id)
    .bind(input.link_type.as_str())
    .bind(input.created_by.as_str())
    .bind(input.confidence)
    .bind(&input.context)
    .bind(input.change_type.map(|ct| ct.as_str().to_string()))
    .bind(input.additions)
    .bind(input.deletions)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!(
                "Link already exists: {} -> {} ({})",
                input.source_id, input.target_id, input.link_type.as_str()
            ))
        }
        _ => Error::Database(e),
    })
}

/// Get a link by ID.
pub async fn get_link(pool: &DbPool, id: &str) -> Result<MemoryLink> {
    sqlx::query_as::<_, MemoryLink>("SELECT * FROM memory_links WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Link not found: {}", id)))
}

/// Get a link by source, target, and type.
pub async fn get_link_by_endpoints(
    pool: &DbPool,
    source_id: &str,
    target_id: &str,
    link_type: &LinkType,
) -> Result<Option<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE source_id = ? AND target_id = ? AND link_type = ?
        "#,
    )
    .bind(source_id)
    .bind(target_id)
    .bind(link_type.as_str())
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Delete a link by ID.
pub async fn delete_link(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM memory_links WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Link not found: {}", id)));
    }

    Ok(())
}

/// Delete a link by endpoints.
pub async fn delete_link_by_endpoints(
    pool: &DbPool,
    source_id: &str,
    target_id: &str,
    link_type: &LinkType,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM memory_links WHERE source_id = ? AND target_id = ? AND link_type = ?",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(link_type.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete all links for a memory (as source or target).
pub async fn delete_memory_links(pool: &DbPool, memory_id: &str) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM memory_links WHERE source_id = ? OR target_id = ?",
    )
    .bind(memory_id)
    .bind(memory_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// List outgoing links from a memory.
/// Uses idx_links_source index.
pub async fn list_outgoing_links(pool: &DbPool, source_id: &str) -> Result<Vec<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE source_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List incoming links to a memory.
/// Uses idx_links_target index.
pub async fn list_incoming_links(pool: &DbPool, target_id: &str) -> Result<Vec<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE target_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(target_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List all links for a memory (both directions).
pub async fn list_memory_links(pool: &DbPool, memory_id: &str) -> Result<Vec<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE source_id = ? OR target_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(memory_id)
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List links by type for a project.
/// Uses idx_links_type index (project_id, link_type).
pub async fn list_links_by_type(
    pool: &DbPool,
    project_id: &str,
    link_type: &LinkType,
) -> Result<Vec<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE project_id = ? AND link_type = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .bind(link_type.as_str())
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List links for a project.
pub async fn list_project_links(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE project_id = ?
        ORDER BY created_at DESC
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

/// List AI-suggested links with confidence above threshold.
pub async fn list_ai_links_above_threshold(
    pool: &DbPool,
    project_id: &str,
    min_confidence: f64,
) -> Result<Vec<MemoryLink>> {
    sqlx::query_as::<_, MemoryLink>(
        r#"
        SELECT * FROM memory_links
        WHERE project_id = ? AND created_by = 'ai' AND confidence >= ?
        ORDER BY confidence DESC
        "#,
    )
    .bind(project_id)
    .bind(min_confidence)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Count links for a project.
pub async fn count_project_links(pool: &DbPool, project_id: &str) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_links WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Traverse graph from a starting node (BFS).
/// Returns all reachable memories up to max_depth.
pub async fn traverse_graph(
    pool: &DbPool,
    start_id: &str,
    max_depth: i32,
    link_types: Option<&[LinkType]>,
) -> Result<Vec<GraphNode>> {
    // SQLite doesn't have great recursive CTE support for this,
    // so we implement BFS in application code
    use std::collections::{HashSet, VecDeque};

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    let mut queue = VecDeque::new();

    queue.push_back(GraphNode {
        memory_id: start_id.to_string(),
        depth: 0,
        path: vec![start_id.to_string()],
    });
    visited.insert(start_id.to_string());

    while let Some(node) = queue.pop_front() {
        if node.depth > 0 {
            result.push(node.clone());
        }

        if node.depth >= max_depth {
            continue;
        }

        // Get outgoing links
        let links = list_outgoing_links(pool, &node.memory_id).await?;

        for link in links {
            // Filter by link type if specified
            if let Some(types) = link_types {
                let link_type_enum = link.link_type_enum();
                if !types.iter().any(|t| t.as_str() == link_type_enum.as_str()) {
                    continue;
                }
            }

            if !visited.contains(&link.target_id) {
                visited.insert(link.target_id.clone());
                let mut new_path = node.path.clone();
                new_path.push(link.target_id.clone());

                queue.push_back(GraphNode {
                    memory_id: link.target_id,
                    depth: node.depth + 1,
                    path: new_path,
                });
            }
        }
    }

    Ok(result)
}

/// Traverse graph in both directions (bidirectional BFS).
pub async fn traverse_graph_bidirectional(
    pool: &DbPool,
    start_id: &str,
    max_depth: i32,
) -> Result<Vec<GraphNode>> {
    use std::collections::{HashSet, VecDeque};

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    let mut queue = VecDeque::new();

    queue.push_back(GraphNode {
        memory_id: start_id.to_string(),
        depth: 0,
        path: vec![start_id.to_string()],
    });
    visited.insert(start_id.to_string());

    while let Some(node) = queue.pop_front() {
        if node.depth > 0 {
            result.push(node.clone());
        }

        if node.depth >= max_depth {
            continue;
        }

        // Get all links (both directions)
        let links = list_memory_links(pool, &node.memory_id).await?;

        for link in links {
            let neighbor_id = if link.source_id == node.memory_id {
                &link.target_id
            } else {
                &link.source_id
            };

            if !visited.contains(neighbor_id) {
                visited.insert(neighbor_id.clone());
                let mut new_path = node.path.clone();
                new_path.push(neighbor_id.clone());

                queue.push_back(GraphNode {
                    memory_id: neighbor_id.clone(),
                    depth: node.depth + 1,
                    path: new_path,
                });
            }
        }
    }

    Ok(result)
}

/// Find shortest path between two memories.
pub async fn find_path(
    pool: &DbPool,
    from_id: &str,
    to_id: &str,
    max_depth: i32,
) -> Result<Option<Vec<String>>> {
    use std::collections::{HashSet, VecDeque};

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(GraphNode {
        memory_id: from_id.to_string(),
        depth: 0,
        path: vec![from_id.to_string()],
    });
    visited.insert(from_id.to_string());

    while let Some(node) = queue.pop_front() {
        if node.memory_id == to_id {
            return Ok(Some(node.path));
        }

        if node.depth >= max_depth {
            continue;
        }

        let links = list_memory_links(pool, &node.memory_id).await?;

        for link in links {
            let neighbor_id = if link.source_id == node.memory_id {
                &link.target_id
            } else {
                &link.source_id
            };

            if !visited.contains(neighbor_id) {
                visited.insert(neighbor_id.clone());
                let mut new_path = node.path.clone();
                new_path.push(neighbor_id.clone());

                if neighbor_id == to_id {
                    return Ok(Some(new_path));
                }

                queue.push_back(GraphNode {
                    memory_id: neighbor_id.clone(),
                    depth: node.depth + 1,
                    path: new_path,
                });
            }
        }
    }

    Ok(None)
}

/// Simple input for creating a link (for API use).
#[derive(Debug, Clone)]
pub struct CreateMemoryLink {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub link_type: String,
    pub context: Option<String>,
}

/// Create a memory link with simplified input (API-friendly).
pub async fn create_memory_link(pool: &DbPool, input: CreateMemoryLink) -> Result<MemoryLink> {
    // Determine project from source memory
    let source_memory = crate::db::get_memory(pool, &input.source_id).await?;

    create_link(pool, CreateLink {
        id: input.id,
        project_id: source_memory.project_id,
        source_id: input.source_id,
        target_id: input.target_id,
        link_type: LinkType::from_str(&input.link_type),
        created_by: LinkCreator::User,
        confidence: None,
        context: input.context,
        change_type: None,
        additions: None,
        deletions: None,
    }).await
}

/// Get all links for a memory (outgoing only, for API use).
/// This is an alias for list_outgoing_links with a more intuitive name.
pub async fn get_memory_links(pool: &DbPool, memory_id: &str) -> Result<Vec<MemoryLink>> {
    list_outgoing_links(pool, memory_id).await
}

/// Bulk create links (for batch operations).
pub async fn create_links_batch(pool: &DbPool, links: Vec<CreateLink>) -> Result<Vec<MemoryLink>> {
    let mut results = Vec::with_capacity(links.len());

    for link in links {
        // Skip duplicates silently in batch mode
        match create_link(pool, link).await {
            Ok(created) => results.push(created),
            Err(Error::AlreadyExists(_)) => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_pool, migrate, create_project, CreateProject, create_memory, CreateMemory, MemoryType};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        create_project(&pool, CreateProject {
            id: "proj-1".to_string(),
            slug: "test".to_string(),
            name: "Test".to_string(),
            description: None,
        }).await.unwrap();

        // Create test memories
        for i in 1..=3 {
            create_memory(&pool, CreateMemory {
                id: format!("mem-{}", i),
                project_id: "proj-1".to_string(),
                repository_id: None,
                memory_type: MemoryType::General,
                source: None,
                title: Some(format!("Memory {}", i)),
                content: Some(format!("Content {}", i)),
                content_hash: None,
                content_storage: "filesystem".to_string(),
                file_path: None,
                language: None,
                git_branch: None,
                git_commit_sha: None,
                author: None,
                keywords: None,
                tags: None,
            }).await.unwrap();
        }

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_link() {
        let pool = setup_test_db().await;

        let link = create_link(&pool, CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: LinkType::References,
            created_by: LinkCreator::User,
            confidence: None,
            context: Some("Test link".to_string()),
            change_type: None,
            additions: None,
            deletions: None,
        }).await.unwrap();

        assert_eq!(link.id, "link-1");
        assert_eq!(link.link_type, "references");

        let fetched = get_link(&pool, "link-1").await.unwrap();
        assert_eq!(fetched.context, Some("Test link".to_string()));
    }

    #[tokio::test]
    async fn test_graph_traversal() {
        let pool = setup_test_db().await;

        // Create chain: mem-1 -> mem-2 -> mem-3
        create_link(&pool, CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: LinkType::References,
            created_by: LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        }).await.unwrap();

        create_link(&pool, CreateLink {
            id: "link-2".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-2".to_string(),
            target_id: "mem-3".to_string(),
            link_type: LinkType::References,
            created_by: LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        }).await.unwrap();

        let nodes = traverse_graph(&pool, "mem-1", 3, None).await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().any(|n| n.memory_id == "mem-2"));
        assert!(nodes.iter().any(|n| n.memory_id == "mem-3"));
    }

    #[tokio::test]
    async fn test_find_path() {
        let pool = setup_test_db().await;

        // Create path: mem-1 -> mem-2 -> mem-3
        create_link(&pool, CreateLink {
            id: "link-1".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-1".to_string(),
            target_id: "mem-2".to_string(),
            link_type: LinkType::References,
            created_by: LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        }).await.unwrap();

        create_link(&pool, CreateLink {
            id: "link-2".to_string(),
            project_id: "proj-1".to_string(),
            source_id: "mem-2".to_string(),
            target_id: "mem-3".to_string(),
            link_type: LinkType::References,
            created_by: LinkCreator::System,
            confidence: None,
            context: None,
            change_type: None,
            additions: None,
            deletions: None,
        }).await.unwrap();

        let path = find_path(&pool, "mem-1", "mem-3", 5).await.unwrap();
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, vec!["mem-1", "mem-2", "mem-3"]);
    }
}
