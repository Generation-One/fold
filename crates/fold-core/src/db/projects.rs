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
    Local,
}

impl GitProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Local => "local",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            "local" => Some(Self::Local),
            _ => None,
        }
    }
}

/// Project record from the database.
/// A project IS a repository - they are merged into a single entity.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,

    // Repository info (required)
    /// Provider type: 'github' | 'gitlab' | 'local'
    pub provider: String,
    /// Local path where fold/ directory lives
    pub root_path: String,

    // Remote repo info (for github/gitlab providers)
    pub remote_owner: Option<String>,
    pub remote_repo: Option<String>,
    pub remote_branch: Option<String>,
    pub access_token: Option<String>,
    pub webhook_id: Option<String>,
    pub webhook_secret: Option<String>,

    // Sync state
    pub last_sync: Option<String>,
    pub last_commit_sha: Option<String>,
    pub last_indexed_at: Option<String>,
    pub sync_cursor: Option<String>,

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

    // Webhook loop prevention
    pub ignored_commit_authors: Option<String>,

    // Decay algorithm configuration
    pub decay_strength_weight: Option<f64>,
    pub decay_half_life_days: Option<f64>,

    pub created_at: String,
    pub updated_at: String,
}

impl Project {
    /// Get the project's provider type.
    pub fn provider_type(&self) -> Option<GitProvider> {
        GitProvider::from_str(&self.provider)
    }

    /// Check if this is a remote repository (github/gitlab).
    pub fn is_remote(&self) -> bool {
        matches!(self.provider.as_str(), "github" | "gitlab")
    }

    /// Check if this is a local-only project.
    pub fn is_local(&self) -> bool {
        self.provider == "local"
    }

    /// Get the remote URL for display (e.g., https://github.com/owner/repo).
    pub fn remote_url(&self) -> Option<String> {
        match self.provider.as_str() {
            "github" => {
                let owner = self.remote_owner.as_ref()?;
                let repo = self.remote_repo.as_ref()?;
                Some(format!("https://github.com/{}/{}", owner, repo))
            }
            "gitlab" => {
                let owner = self.remote_owner.as_ref()?;
                let repo = self.remote_repo.as_ref()?;
                Some(format!("https://gitlab.com/{}/{}", owner, repo))
            }
            _ => None,
        }
    }

    /// Check if metadata repo sync is enabled.
    pub fn is_metadata_sync_enabled(&self) -> bool {
        self.metadata_repo_enabled != 0
    }

    /// Get the metadata repo mode.
    pub fn metadata_mode(&self) -> Option<MetadataRepoMode> {
        self.metadata_repo_mode
            .as_ref()
            .and_then(|m| MetadataRepoMode::from_str(m))
    }

    /// Get the metadata repo provider.
    pub fn metadata_provider(&self) -> Option<GitProvider> {
        self.metadata_repo_provider
            .as_ref()
            .and_then(|p| GitProvider::from_str(p))
    }

    /// Check if auto-commit is enabled for fold/ directory.
    /// Always returns true (auto-commit is enabled by default in simplified schema).
    pub fn auto_commit_enabled(&self) -> bool {
        true
    }

    /// Get the full name of the project's repository (e.g., "owner/repo" or slug for local).
    pub fn full_name(&self) -> String {
        match self.provider.as_str() {
            "github" | "gitlab" => {
                let owner = self.remote_owner.as_deref().unwrap_or("unknown");
                let repo = self.remote_repo.as_deref().unwrap_or("unknown");
                format!("{}/{}", owner, repo)
            }
            _ => self.slug.clone(),
        }
    }
}

/// Input for creating a new project.
#[derive(Debug, Clone)]
pub struct CreateProject {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    /// Provider type: 'github' | 'gitlab' | 'local'
    pub provider: String,
    /// Local path where fold/ directory lives (required)
    pub root_path: String,
    // Remote repo info (for github/gitlab)
    pub remote_owner: Option<String>,
    pub remote_repo: Option<String>,
    pub remote_branch: Option<String>,
    pub access_token: Option<String>,
}

/// Input for updating a project.
#[derive(Debug, Clone, Default)]
pub struct UpdateProject {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    /// JSON string of author patterns to ignore during webhook processing
    pub ignored_commit_authors: Option<String>,
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
        INSERT INTO projects (id, slug, name, description, provider, root_path, remote_owner, remote_repo, remote_branch, access_token)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.slug)
    .bind(&input.name)
    .bind(&input.description)
    .bind(&input.provider)
    .bind(&input.root_path)
    .bind(&input.remote_owner)
    .bind(&input.remote_repo)
    .bind(&input.remote_branch)
    .bind(&input.access_token)
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
    if let Some(ignored_authors) = input.ignored_commit_authors {
        updates.push("ignored_commit_authors = ?");
        bindings.push(ignored_authors);
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

/// Input for updating algorithm configuration.
#[derive(Debug, Clone, Default)]
pub struct UpdateAlgorithmConfig {
    pub decay_strength_weight: Option<f64>,
    pub decay_half_life_days: Option<f64>,
    pub ignored_commit_authors: Option<String>,
}

/// Update algorithm configuration for a project.
pub async fn update_algorithm_config(
    pool: &DbPool,
    id: &str,
    input: UpdateAlgorithmConfig,
) -> Result<Project> {
    let mut updates = Vec::new();
    let _float_bindings: Vec<f64> = Vec::new();
    let _string_bindings: Vec<String> = Vec::new();

    if let Some(weight) = input.decay_strength_weight {
        updates.push(("decay_strength_weight = ?", true, weight, String::new()));
    }
    if let Some(half_life) = input.decay_half_life_days {
        updates.push(("decay_half_life_days = ?", true, half_life, String::new()));
    }
    if let Some(authors) = input.ignored_commit_authors {
        updates.push(("ignored_commit_authors = ?", false, 0.0, authors));
    }

    if updates.is_empty() {
        return get_project(pool, id).await;
    }

    // Build the SET clause
    let set_clause: Vec<&str> = updates.iter().map(|(sql, _, _, _)| *sql).collect();
    let query = format!(
        "UPDATE projects SET {}, updated_at = datetime('now') WHERE id = ? RETURNING *",
        set_clause.join(", ")
    );

    // SQLite doesn't support mixed bind types well, so use raw query with string formatting
    // This is safe because we control all the values
    let mut q = sqlx::query_as::<_, Project>(&query);
    for (_, is_float, float_val, string_val) in &updates {
        if *is_float {
            q = q.bind(*float_val);
        } else {
            q = q.bind(string_val);
        }
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
    sqlx::query_as::<_, Project>("SELECT * FROM projects ORDER BY name ASC")
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
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM projects WHERE slug = ?")
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

/// Get webhook secret for a project.
/// Returns None if not set.
pub async fn get_webhook_secret(pool: &DbPool, project_id: &str) -> Result<Option<String>> {
    let project = get_project(pool, project_id).await?;
    Ok(project.webhook_secret)
}

/// Update project sync state after indexing.
pub async fn update_project_indexed(
    pool: &DbPool,
    project_id: &str,
    last_commit_sha: Option<&str>,
) -> Result<Project> {
    sqlx::query_as::<_, Project>(
        r#"
        UPDATE projects SET
            last_indexed_at = datetime('now'),
            last_commit_sha = COALESCE(?, last_commit_sha),
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(last_commit_sha)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", project_id)))
}

/// List projects that need polling (remote providers with webhook not set up).
pub async fn list_polling_projects(pool: &DbPool) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT * FROM projects
        WHERE provider IN ('github', 'gitlab')
          AND (webhook_id IS NULL OR webhook_id = '')
        ORDER BY last_sync ASC NULLS FIRST
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Update project after sync.
pub async fn update_project_sync(
    pool: &DbPool,
    project_id: &str,
    last_commit_sha: Option<&str>,
    sync_cursor: Option<&str>,
) -> Result<Project> {
    sqlx::query_as::<_, Project>(
        r#"
        UPDATE projects SET
            last_sync = datetime('now'),
            last_commit_sha = COALESCE(?, last_commit_sha),
            sync_cursor = COALESCE(?, sync_cursor),
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(last_commit_sha)
    .bind(sync_cursor)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Project not found: {}", project_id)))
}

// ============================================================================
// Project Members
// ============================================================================

/// Project member record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectMember {
    pub user_id: String,
    pub project_id: String,
    pub role: String,
    pub added_by: Option<String>,
    pub created_at: String,
}

impl ProjectMember {
    /// Check if this member can write (create/update/delete).
    pub fn can_write(&self) -> bool {
        self.role == "member"
    }

    /// Check if this member can read.
    pub fn can_read(&self) -> bool {
        true // Both member and viewer can read
    }
}

/// Add a user to a project with a specific role.
pub async fn add_project_member(
    pool: &DbPool,
    project_id: &str,
    user_id: &str,
    role: &str,
    added_by: Option<&str>,
) -> Result<ProjectMember> {
    sqlx::query_as::<_, ProjectMember>(
        r#"
        INSERT INTO project_members (user_id, project_id, role, added_by)
        VALUES (?, ?, ?, ?)
        ON CONFLICT (user_id, project_id) DO UPDATE SET
            role = excluded.role,
            added_by = excluded.added_by
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(role)
    .bind(added_by)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Remove a user from a project.
pub async fn remove_project_member(pool: &DbPool, project_id: &str, user_id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM project_members WHERE project_id = ? AND user_id = ?")
        .bind(project_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Get a user's membership in a project.
pub async fn get_project_member(
    pool: &DbPool,
    project_id: &str,
    user_id: &str,
) -> Result<Option<ProjectMember>> {
    sqlx::query_as::<_, ProjectMember>(
        "SELECT * FROM project_members WHERE project_id = ? AND user_id = ?",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// List all members of a project.
pub async fn list_project_members(pool: &DbPool, project_id: &str) -> Result<Vec<ProjectMember>> {
    sqlx::query_as::<_, ProjectMember>(
        r#"
        SELECT * FROM project_members
        WHERE project_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List all projects a user has access to.
pub async fn list_user_projects(pool: &DbPool, user_id: &str) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT p.* FROM projects p
        INNER JOIN project_members pm ON p.id = pm.project_id
        WHERE pm.user_id = ?
        ORDER BY p.name ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List projects a user can write to (member role).
pub async fn list_user_writable_projects(pool: &DbPool, user_id: &str) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT p.* FROM projects p
        INNER JOIN project_members pm ON p.id = pm.project_id
        WHERE pm.user_id = ? AND pm.role = 'member'
        ORDER BY p.name ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Check if a user can access a project.
pub async fn can_user_access_project(
    pool: &DbPool,
    user_id: &str,
    project_id: &str,
) -> Result<bool> {
    let member = get_project_member(pool, project_id, user_id).await?;
    Ok(member.is_some())
}

/// Check if a user can write to a project.
pub async fn can_user_write_project(
    pool: &DbPool,
    user_id: &str,
    project_id: &str,
) -> Result<bool> {
    let member = get_project_member(pool, project_id, user_id).await?;
    Ok(member.map(|m| m.can_write()).unwrap_or(false))
}

/// Update a member's role.
pub async fn update_project_member_role(
    pool: &DbPool,
    project_id: &str,
    user_id: &str,
    role: &str,
) -> Result<Option<ProjectMember>> {
    sqlx::query_as::<_, ProjectMember>(
        r#"
        UPDATE project_members
        SET role = ?
        WHERE project_id = ? AND user_id = ?
        RETURNING *
        "#,
    )
    .bind(role)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Count members in a project.
pub async fn count_project_members(pool: &DbPool, project_id: &str) -> Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM project_members WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Extended project member info with user details.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectMemberWithUser {
    pub user_id: String,
    pub project_id: String,
    pub role: String,
    pub added_by: Option<String>,
    pub created_at: String,
    // User fields
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// List project members with user details.
pub async fn list_project_members_with_users(
    pool: &DbPool,
    project_id: &str,
) -> Result<Vec<ProjectMemberWithUser>> {
    sqlx::query_as::<_, ProjectMemberWithUser>(
        r#"
        SELECT
            pm.user_id,
            pm.project_id,
            pm.role,
            pm.added_by,
            pm.created_at,
            u.email,
            u.display_name,
            u.avatar_url
        FROM project_members pm
        INNER JOIN users u ON pm.user_id = u.id
        WHERE pm.project_id = ?
        ORDER BY pm.created_at ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// Project Group Members
// ============================================================================

/// Project group member record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectGroupMember {
    pub group_id: String,
    pub project_id: String,
    pub role: String,
    pub added_by: Option<String>,
    pub created_at: String,
}

impl ProjectGroupMember {
    /// Check if this group can write (create/update/delete).
    pub fn can_write(&self) -> bool {
        self.role == "member"
    }

    /// Check if this group can read.
    pub fn can_read(&self) -> bool {
        true // Both member and viewer can read
    }
}

/// Extended project group member info with group details.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectGroupMemberWithGroup {
    pub group_id: String,
    pub project_id: String,
    pub role: String,
    pub added_by: Option<String>,
    pub created_at: String,
    // Group fields
    pub name: String,
    pub description: Option<String>,
}

/// Add a group to a project with a specific role.
pub async fn add_project_group_member(
    pool: &DbPool,
    project_id: &str,
    group_id: &str,
    role: &str,
    added_by: Option<&str>,
) -> Result<ProjectGroupMember> {
    sqlx::query_as::<_, ProjectGroupMember>(
        r#"
        INSERT INTO project_group_members (group_id, project_id, role, added_by)
        VALUES (?, ?, ?, ?)
        ON CONFLICT (group_id, project_id) DO UPDATE SET
            role = excluded.role,
            added_by = excluded.added_by
        RETURNING *
        "#,
    )
    .bind(group_id)
    .bind(project_id)
    .bind(role)
    .bind(added_by)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Remove a group from a project.
pub async fn remove_project_group_member(
    pool: &DbPool,
    project_id: &str,
    group_id: &str,
) -> Result<bool> {
    let result =
        sqlx::query("DELETE FROM project_group_members WHERE project_id = ? AND group_id = ?")
            .bind(project_id)
            .bind(group_id)
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

/// Get a group's membership in a project.
pub async fn get_project_group_member(
    pool: &DbPool,
    project_id: &str,
    group_id: &str,
) -> Result<Option<ProjectGroupMember>> {
    sqlx::query_as::<_, ProjectGroupMember>(
        "SELECT * FROM project_group_members WHERE project_id = ? AND group_id = ?",
    )
    .bind(project_id)
    .bind(group_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// List all group members of a project.
pub async fn list_project_group_members(
    pool: &DbPool,
    project_id: &str,
) -> Result<Vec<ProjectGroupMember>> {
    sqlx::query_as::<_, ProjectGroupMember>(
        r#"
        SELECT * FROM project_group_members
        WHERE project_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List all projects a group has access to.
pub async fn list_group_projects(pool: &DbPool, group_id: &str) -> Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        r#"
        SELECT p.* FROM projects p
        INNER JOIN project_group_members pgm ON p.id = pgm.project_id
        WHERE pgm.group_id = ?
        ORDER BY p.name ASC
        "#,
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Update a group member's role in a project.
pub async fn update_project_group_member_role(
    pool: &DbPool,
    project_id: &str,
    group_id: &str,
    role: &str,
) -> Result<Option<ProjectGroupMember>> {
    sqlx::query_as::<_, ProjectGroupMember>(
        r#"
        UPDATE project_group_members
        SET role = ?
        WHERE project_id = ? AND group_id = ?
        RETURNING *
        "#,
    )
    .bind(role)
    .bind(project_id)
    .bind(group_id)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// List project group members with group details.
pub async fn list_project_group_members_with_groups(
    pool: &DbPool,
    project_id: &str,
) -> Result<Vec<ProjectGroupMemberWithGroup>> {
    sqlx::query_as::<_, ProjectGroupMemberWithGroup>(
        r#"
        SELECT
            pgm.group_id,
            pgm.project_id,
            pgm.role,
            pgm.added_by,
            pgm.created_at,
            g.name,
            g.description
        FROM project_group_members pgm
        INNER JOIN groups g ON pgm.group_id = g.id
        WHERE pgm.project_id = ?
        ORDER BY pgm.created_at ASC
        "#,
    )
    .bind(project_id)
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

    fn test_create_input(id: &str, slug: &str, name: &str) -> CreateProject {
        CreateProject {
            id: id.to_string(),
            slug: slug.to_string(),
            name: name.to_string(),
            description: None,
            provider: "local".to_string(),
            root_path: format!("/tmp/test/{}", slug),
            remote_owner: None,
            remote_repo: None,
            remote_branch: None,
            access_token: None,
        }
    }

    #[tokio::test]
    async fn test_create_and_get_project() {
        let pool = setup_test_db().await;

        let mut input = test_create_input("proj-1", "test-project", "Test Project");
        input.description = Some("A test project".to_string());

        let project = create_project(&pool, input).await.unwrap();

        assert_eq!(project.id, "proj-1");
        assert_eq!(project.slug, "test-project");
        assert_eq!(project.provider, "local");

        let fetched = get_project(&pool, "proj-1").await.unwrap();
        assert_eq!(fetched.name, "Test Project");
    }

    #[tokio::test]
    async fn test_get_project_by_slug() {
        let pool = setup_test_db().await;

        create_project(&pool, test_create_input("proj-1", "my-project", "My Project"))
            .await
            .unwrap();

        let project = get_project_by_slug(&pool, "my-project").await.unwrap();
        assert!(project.is_some());
        assert_eq!(project.unwrap().id, "proj-1");
    }

    #[tokio::test]
    async fn test_duplicate_slug_error() {
        let pool = setup_test_db().await;

        create_project(&pool, test_create_input("proj-1", "unique-slug", "Project 1"))
            .await
            .unwrap();

        let result = create_project(&pool, test_create_input("proj-2", "unique-slug", "Project 2")).await;

        assert!(matches!(result, Err(Error::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_list_projects() {
        let pool = setup_test_db().await;

        for i in 1..=3 {
            create_project(
                &pool,
                test_create_input(&format!("proj-{}", i), &format!("project-{}", i), &format!("Project {}", i)),
            )
            .await
            .unwrap();
        }

        let projects = list_projects(&pool).await.unwrap();
        assert_eq!(projects.len(), 3);
    }
}
