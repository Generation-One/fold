//! Project service for project management.
//!
//! Handles CRUD operations for projects, including initialization
//! of Qdrant collections.

use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, info};

use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::models::{Project, ProjectCreate, ProjectStats};

use super::qdrant::QdrantService;
use super::EmbeddingService;

/// Service for managing projects.
#[derive(Clone)]
pub struct ProjectService {
    db: DbPool,
    qdrant: Arc<QdrantService>,
    embeddings: Arc<EmbeddingService>,
}

impl ProjectService {
    /// Create a new project service.
    pub fn new(
        db: DbPool,
        qdrant: Arc<QdrantService>,
        embeddings: Arc<EmbeddingService>,
    ) -> Self {
        Self { db, qdrant, embeddings }
    }

    /// Create a new project.
    pub async fn create(&self, data: ProjectCreate, owner: Option<String>) -> Result<Project> {
        let mut project = Project::from_create(data, owner);

        // Check if slug already exists, if so, make it unique
        let existing = self.get_by_slug(&project.slug).await?;
        if existing.is_some() {
            let base_slug = project.slug.clone();
            let mut counter = 1;
            loop {
                project.slug = format!("{}-{}", base_slug, counter);
                if self.get_by_slug(&project.slug).await?.is_none() {
                    break;
                }
                counter += 1;
            }
        }

        // Insert into database
        sqlx::query(
            r#"
            INSERT INTO projects (
                id, slug, name, description,
                index_patterns, ignore_patterns, team_members, owner,
                metadata, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&project.id)
        .bind(&project.slug)
        .bind(&project.name)
        .bind(&project.description)
        .bind(&project.index_patterns)
        .bind(&project.ignore_patterns)
        .bind(&project.team_members)
        .bind(&project.owner)
        .bind(&project.metadata)
        .bind(project.created_at)
        .bind(project.updated_at)
        .execute(&self.db)
        .await?;

        // Create Qdrant collection
        self.qdrant
            .create_collection(&project.slug, self.embeddings.dimension().await)
            .await?;

        info!(id = %project.id, slug = %project.slug, "Created project");

        Ok(project)
    }

    /// Get a project by ID.
    pub async fn get(&self, id: &str) -> Result<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT * FROM projects WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?;

        Ok(project)
    }

    /// Get a project by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT * FROM projects WHERE slug = ?
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.db)
        .await?;

        Ok(project)
    }

    /// Get a project by ID or slug.
    pub async fn get_by_id_or_slug(&self, id_or_slug: &str) -> Result<Option<Project>> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT * FROM projects WHERE id = ? OR slug = ?
            "#,
        )
        .bind(id_or_slug)
        .bind(id_or_slug)
        .fetch_optional(&self.db)
        .await?;

        Ok(project)
    }

    /// List all projects.
    pub async fn list(&self) -> Result<Vec<Project>> {
        let projects = sqlx::query_as::<_, Project>(
            r#"
            SELECT * FROM projects ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        Ok(projects)
    }

    /// List projects for a user (owner or team member).
    pub async fn list_for_user(&self, username: &str) -> Result<Vec<Project>> {
        // This query checks if user is owner or in team_members JSON array
        let projects = sqlx::query_as::<_, Project>(
            r#"
            SELECT * FROM projects
            WHERE owner = ?
               OR team_members LIKE ?
            ORDER BY name ASC
            "#,
        )
        .bind(username)
        .bind(format!("%\"{}%", username))
        .fetch_all(&self.db)
        .await?;

        Ok(projects)
    }

    /// Update a project.
    pub async fn update(&self, id: &str, data: ProjectUpdate) -> Result<Project> {
        let existing = self
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Project {}", id)))?;

        let now = Utc::now();

        // Build update with existing values as defaults
        let name = data.name.unwrap_or(existing.name);
        let description = data.description.or(existing.description);
        let index_patterns = data
            .index_patterns
            .map(|p| serde_json::to_string(&p).unwrap())
            .or(existing.index_patterns);
        let ignore_patterns = data
            .ignore_patterns
            .map(|p| serde_json::to_string(&p).unwrap())
            .or(existing.ignore_patterns);
        let team_members = data
            .team_members
            .map(|m| serde_json::to_string(&m).unwrap())
            .or(existing.team_members);

        sqlx::query(
            r#"
            UPDATE projects
            SET name = ?,
                description = ?,
                index_patterns = ?,
                ignore_patterns = ?,
                team_members = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&name)
        .bind(&description)
        .bind(&index_patterns)
        .bind(&ignore_patterns)
        .bind(&team_members)
        .bind(now)
        .bind(id)
        .execute(&self.db)
        .await?;

        let updated = Project {
            name,
            description,
            index_patterns,
            ignore_patterns,
            team_members,
            updated_at: now,
            ..existing
        };

        debug!(id = %id, "Updated project");

        Ok(updated)
    }

    /// Delete a project and all its memories.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let project = self
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Project {}", id)))?;

        // Delete memories
        sqlx::query(
            r#"
            DELETE FROM memories WHERE project_id = ?
            "#,
        )
        .bind(id)
        .execute(&self.db)
        .await?;

        // Delete memory links
        sqlx::query(
            r#"
            DELETE FROM memory_links WHERE project_id = ?
            "#,
        )
        .bind(id)
        .execute(&self.db)
        .await?;

        // Delete repositories
        sqlx::query(
            r#"
            DELETE FROM repositories WHERE project_id = ?
            "#,
        )
        .bind(id)
        .execute(&self.db)
        .await?;

        // Delete team status
        sqlx::query(
            r#"
            DELETE FROM team_status WHERE project_id = ?
            "#,
        )
        .bind(id)
        .execute(&self.db)
        .await?;

        // Delete the project
        sqlx::query(
            r#"
            DELETE FROM projects WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(&self.db)
        .await?;

        // Delete Qdrant collection
        self.qdrant.delete_collection(&project.slug).await?;

        info!(id = %id, slug = %project.slug, "Deleted project");

        Ok(())
    }

    /// Get project statistics.
    pub async fn stats(&self, id: &str) -> Result<ProjectStats> {
        let _project = self
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Project {}", id)))?;

        // Get total memories and by type
        let type_counts: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT type, COUNT(*) as count
            FROM memories
            WHERE project_id = ?
            GROUP BY type
            "#,
        )
        .bind(id)
        .fetch_all(&self.db)
        .await?;

        let total_memories: i64 = type_counts.iter().map(|(_, c)| c).sum();
        let memories_by_type = type_counts.into_iter().collect();

        // Get indexed files count
        let indexed_files: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM memories
            WHERE project_id = ? AND type = 'codebase'
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await?;

        // Get last indexed time
        let last_indexed: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            r#"
            SELECT MAX(created_at) FROM memories
            WHERE project_id = ? AND type = 'codebase'
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?
        .flatten();

        // Get active team members
        let active_team_members: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(DISTINCT username) FROM team_status
            WHERE project_id = ? AND status = 'active'
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await?;

        // Get last activity
        let last_activity: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            r#"
            SELECT MAX(updated_at) FROM memories WHERE project_id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?
        .flatten();

        // Get total commits
        let total_commits: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM git_commits gc
            JOIN repositories r ON gc.repository_id = r.id
            WHERE r.project_id = ?
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(0);

        // Get total links
        let total_links: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM memory_links WHERE project_id = ?
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(0);

        Ok(ProjectStats {
            total_memories,
            memories_by_type,
            indexed_files,
            last_indexed,
            active_team_members,
            last_activity,
            total_commits,
            total_links,
        })
    }

    /// Add a team member to a project.
    pub async fn add_team_member(&self, project_id: &str, username: &str) -> Result<()> {
        let project = self
            .get(project_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Project {}", project_id)))?;

        let mut members = project.team_members_vec();
        if !members.contains(&username.to_string()) {
            members.push(username.to_string());
            let members_json = serde_json::to_string(&members).unwrap();

            sqlx::query(
                r#"
                UPDATE projects SET team_members = ?, updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(members_json)
            .bind(project_id)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }

    /// Remove a team member from a project.
    pub async fn remove_team_member(&self, project_id: &str, username: &str) -> Result<()> {
        let project = self
            .get(project_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Project {}", project_id)))?;

        let members: Vec<String> = project
            .team_members_vec()
            .into_iter()
            .filter(|m| m != username)
            .collect();

        let members_json = if members.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&members).unwrap())
        };

        sqlx::query(
            r#"
            UPDATE projects SET team_members = ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(members_json)
        .bind(project_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

/// Update request for a project
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ProjectUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub index_patterns: Option<Vec<String>>,
    pub ignore_patterns: Option<Vec<String>>,
    pub team_members: Option<Vec<String>>,
}
