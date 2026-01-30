//! Project database queries.
//!
//! Projects are the top-level organizational unit in Fold.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// Types
// ============================================================================

/// Metadata repository sync mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataRepoMode {
    /// Sync to a separate repository
    Separate,
    /// Sync to a path within the source repository
    InRepo,
}

impl MetadataRepoMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Separate => "separate",
            Self::InRepo => "in_repo",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "separate" => Some(Self::Separate),
            "in_repo" => Some(Self::InRepo),
            _ => None,
        }
    }
}

/// Git provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitProvider {
    GitHub,
    GitLab,
}

impl GitProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            _ => None,
        }
    }
}

/// Project record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,

    // Metadata repo sync config
    pub metadata_repo_enabled: i32,
    pub metadata_repo_mode: Option<String>,
    pub metadata_repo_provider: Option<String>,
    pub metadata_repo_owner: Option<String>,
    pub metadata_repo_name: Option<String>,
    pub metadata_repo_branch: Option<String>,
    pub metadata_repo_token: Option<String>,
    pub metadata_repo_source_id: Option<String>,
    pub metadata_repo_path_prefix: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

impl Project {
    /// Check if metadata repo sync is enabled.
    pub fn is_metadata_sync_enabled(&self) -> bool {
        self.metadata_repo_enabled != 0
    }

    /// Get the metadata repo mode.
    pub fn metadata_mode(&self) -> Option<MetadataRepoMode> {
        self.metadata_repo_mode.as_ref().and_then(|m| MetadataRepoMode::from_str(m))
    }

    /// Get the metadata repo provider.
    pub fn metadata_provider(&self) -> Option<GitProvider> {
        self.metadata_repo_provider.as_ref().and_then(|p| GitProvider::from_str(p))
    }
}

/// Input for creating a new project.
#[derive(Debug, Clone)]
pub struct CreateProject {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
}

/// Input for updating a project.
#[derive(Debug, Clone, Default)]
pub struct UpdateProject {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Metadata repository configuration.
#[derive(Debug, Clone)]
pub struct MetadataRepoConfig {
    pub enabled: bool,
    pub mode: MetadataRepoMode,
    pub provider: GitProvider,
    pub owner: String,
    pub name: String,
    pub branch: String,
    pub token: String,
    pub source_id: Option<String>,
    pub path_prefix: String,
}

// ============================================================================
// Queries
// ============================================================================

/// Create a new project.
pub async fn create_project(pool: &DbPool, input: CreateProject) -> Result<Project> {
    sqlx::query_as::<_, Project>(
        r#"
        INSERT INTO projects (id, slug, name, description)
        VALUES (?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.slug)
    .bind(&input.name)
    .bind(&input.description)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            Error::AlreadyExists(format!("Project with slug '{}' already exists", input.slug))
        }
        _ => Error::Database(e),
    })
}

/// Get a project by ID.
pub async fn get_project(pool: &DbPool, id: &str) -> Result<Project> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Project not found: {}", id)))
}

/// Get a project by slug.
/// Uses idx_projects_slug index.
pub async fn get_project_by_slug(pool: &DbPool, slug: &str) -> Result<Option<Project>> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// Get a project by ID or slug.
pub async fn get_project_by_id_or_slug(pool: &DbPool, id_or_slug: &str) -> Result<Project> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT * FROM projects
        WHERE id = ? OR slug = ?
        "#,
    )
    .bind(id_or_slug)
    .bind(id_or_slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", id_or_slug)))
}

/// Update a project.
pub async fn update_project(pool: &DbPool, id: &str, input: UpdateProject) -> Result<Project> {
    let mut updates = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(slug) = input.slug {
        updates.push("slug = ?");
        bindings.push(slug);
    }
    if let Some(name) = input.name {
        updates.push("name = ?");
        bindings.push(name);
    }
    if let Some(description) = input.description {
        updates.push("description = ?");
        bindings.push(description);
    }

    if updates.is_empty() {
        return get_project(pool, id).await;
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE projects SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, Project>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Project not found: {}", id)))
}

/// Configure metadata repository sync.
pub async fn configure_metadata_repo(
    pool: &DbPool,
    project_id: &str,
    config: MetadataRepoConfig,
) -> Result<Project> {
    sqlx::query_as::<_, Project>(
        r#"
        UPDATE projects SET
            metadata_repo_enabled = ?,
            metadata_repo_mode = ?,
            metadata_repo_provider = ?,
            metadata_repo_owner = ?,
            metadata_repo_name = ?,
            metadata_repo_branch = ?,
            metadata_repo_token = ?,
            metadata_repo_source_id = ?,
            metadata_repo_path_prefix = ?,
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(if config.enabled { 1 } else { 0 })
    .bind(config.mode.as_str())
    .bind(config.provider.as_str())
    .bind(&config.owner)
    .bind(&config.name)
    .bind(&config.branch)
    .bind(&config.token)
    .bind(&config.source_id)
    .bind(&config.path_prefix)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", project_id)))
}

/// Disable metadata repository sync.
pub async fn disable_metadata_repo(pool: &DbPool, project_id: &str) -> Result<Project> {
    sqlx::query_as::<_, Project>(
        r#"
        UPDATE projects SET
            metadata_repo_enabled = 0,
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", project_id)))
}

/// Delete a project and cascade to all related entities.
pub async fn delete_project(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Project not found: {}", id)));
    }

    Ok(())
}

/// List all projects.
pub async fn list_projects(pool: &DbPool) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        "SELECT * FROM projects ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List projects with pagination.
pub async fn list_projects_paginated(
    pool: &DbPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT * FROM projects
        ORDER BY name ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Count total projects.
pub async fn count_projects(pool: &DbPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Search projects by name or description.
pub async fn search_projects(pool: &DbPool, query: &str) -> Result<Vec<Project>> {
    let pattern = format!("%{}%", query);
    sqlx::query_as::<_, Project>(
        r#"
        SELECT * FROM projects
        WHERE name LIKE ? OR description LIKE ? OR slug LIKE ?
        ORDER BY name ASC
        "#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Check if a project slug is available.
pub async fn is_slug_available(pool: &DbPool, slug: &str) -> Result<bool> {
    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    Ok(exists.is_none())
}

/// Get projects with metadata sync enabled.
pub async fn list_projects_with_metadata_sync(pool: &DbPool) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT * FROM projects
        WHERE metadata_repo_enabled = 1
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_pool, migrate};

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_create_and_get_project() {
        let pool = setup_test_db().await;

        let project = create_project(&pool, CreateProject {
            id: "proj-1".to_string(),
            slug: "test-project".to_string(),
            name: "Test Project".to_string(),
            description: Some("A test project".to_string()),
        }).await.unwrap();

        assert_eq!(project.id, "proj-1");
        assert_eq!(project.slug, "test-project");

        let fetched = get_project(&pool, "proj-1").await.unwrap();
        assert_eq!(fetched.name, "Test Project");
    }

    #[tokio::test]
    async fn test_get_project_by_slug() {
        let pool = setup_test_db().await;

        create_project(&pool, CreateProject {
            id: "proj-1".to_string(),
            slug: "my-project".to_string(),
            name: "My Project".to_string(),
            description: None,
        }).await.unwrap();

        let project = get_project_by_slug(&pool, "my-project").await.unwrap();
        assert!(project.is_some());
        assert_eq!(project.unwrap().id, "proj-1");
    }

    #[tokio::test]
    async fn test_duplicate_slug_error() {
        let pool = setup_test_db().await;

        create_project(&pool, CreateProject {
            id: "proj-1".to_string(),
            slug: "unique-slug".to_string(),
            name: "Project 1".to_string(),
            description: None,
        }).await.unwrap();

        let result = create_project(&pool, CreateProject {
            id: "proj-2".to_string(),
            slug: "unique-slug".to_string(),
            name: "Project 2".to_string(),
            description: None,
        }).await;

        assert!(matches!(result, Err(Error::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_list_projects() {
        let pool = setup_test_db().await;

        for i in 1..=3 {
            create_project(&pool, CreateProject {
                id: format!("proj-{}", i),
                slug: format!("project-{}", i),
                name: format!("Project {}", i),
                description: None,
            }).await.unwrap();
        }

        let projects = list_projects(&pool).await.unwrap();
        assert_eq!(projects.len(), 3);
    }
}
