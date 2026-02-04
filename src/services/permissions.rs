//! Permission service for centralized access control.
//!
//! Handles checking if users have access to projects based on:
//! - Direct user -> project membership
//! - Group membership (user -> group -> project)
//! - Global admin role (bypass all project checks)

use tracing::{debug, warn};

use crate::db::DbPool;
use crate::error::{Error, Result};

/// Project-level access role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectAccess {
    /// No access to the project.
    NoAccess,
    /// Read-only access to the project.
    Viewer,
    /// Read and write access to the project.
    Member,
}

impl ProjectAccess {
    /// Check if this access level is greater than or equal to another.
    pub fn is_at_least(&self, other: ProjectAccess) -> bool {
        match (self, other) {
            (ProjectAccess::Member, ProjectAccess::Member) => true,
            (ProjectAccess::Member, ProjectAccess::Viewer) => true,
            (ProjectAccess::Member, ProjectAccess::NoAccess) => true,
            (ProjectAccess::Viewer, ProjectAccess::Viewer) => true,
            (ProjectAccess::Viewer, ProjectAccess::NoAccess) => true,
            (ProjectAccess::NoAccess, ProjectAccess::NoAccess) => true,
            _ => false,
        }
    }
}

/// Service for checking permissions.
#[derive(Clone)]
pub struct PermissionService {
    db: DbPool,
}

impl PermissionService {
    /// Create a new permission service.
    pub fn new(db: DbPool) -> Self {
        Self { db }
    }

    /// Check project access for a user via direct membership or group membership.
    ///
    /// Returns the highest privilege level available (member > viewer > no access).
    /// This query checks both:
    /// - Direct membership in `project_members` table
    /// - Group-based membership via `project_group_members` and `group_members`
    pub async fn check_project_access(
        &self,
        user_id: &str,
        project_id: &str,
    ) -> Result<ProjectAccess> {
        let access: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT
                CASE
                    WHEN MAX(CASE WHEN role = 'member' THEN 1 ELSE 0 END) = 1 THEN 'member'
                    WHEN COUNT(*) > 0 THEN 'viewer'
                    ELSE NULL
                END as highest_role
            FROM (
                -- Direct user membership
                SELECT role FROM project_members
                WHERE user_id = ? AND project_id = ?

                UNION ALL

                -- Group-based membership
                SELECT pgm.role
                FROM project_group_members pgm
                INNER JOIN group_members gm ON pgm.group_id = gm.group_id
                WHERE gm.user_id = ? AND pgm.project_id = ?
            ) as access_sources
            "#,
        )
        .bind(user_id)
        .bind(project_id)
        .bind(user_id)
        .bind(project_id)
        .fetch_optional(&self.db)
        .await?;

        let access = match access {
            Some((role,)) if role == "member" => ProjectAccess::Member,
            Some((_,)) => ProjectAccess::Viewer,
            _ => ProjectAccess::NoAccess,
        };

        debug!(
            user_id = %user_id,
            project_id = %project_id,
            access = ?access,
            "Checked project access"
        );

        Ok(access)
    }

    /// Check if user can read from a project.
    ///
    /// Admins have global read access. Other users need at least viewer role.
    pub async fn can_read_project(
        &self,
        user_id: &str,
        project_id: &str,
        user_role: &str,
    ) -> Result<bool> {
        // Admins have global access
        if user_role == "admin" {
            debug!(user_id = %user_id, project_id = %project_id, "Admin bypass: read access");
            return Ok(true);
        }

        let access = self.check_project_access(user_id, project_id).await?;
        let can_read = !matches!(access, ProjectAccess::NoAccess);

        debug!(
            user_id = %user_id,
            project_id = %project_id,
            can_read = can_read,
            "Checked read access"
        );

        Ok(can_read)
    }

    /// Check if user can write to a project.
    ///
    /// Admins have global write access. Other users need member role.
    pub async fn can_write_project(
        &self,
        user_id: &str,
        project_id: &str,
        user_role: &str,
    ) -> Result<bool> {
        // Admins have global access
        if user_role == "admin" {
            debug!(user_id = %user_id, project_id = %project_id, "Admin bypass: write access");
            return Ok(true);
        }

        let access = self.check_project_access(user_id, project_id).await?;
        let can_write = matches!(access, ProjectAccess::Member);

        debug!(
            user_id = %user_id,
            project_id = %project_id,
            can_write = can_write,
            "Checked write access"
        );

        Ok(can_write)
    }

    /// Check project access considering both token scoping and membership.
    ///
    /// If the token has project_ids scoping, the project_id must be in the list.
    /// Then checks membership-based access as normal.
    pub async fn check_project_access_with_token(
        &self,
        user_id: &str,
        project_id: &str,
        token_project_ids: &[String],
    ) -> Result<ProjectAccess> {
        // If token has project_ids, check if this project is in scope
        if !token_project_ids.is_empty() {
            if !token_project_ids.contains(&project_id.to_string()) {
                debug!(
                    user_id = %user_id,
                    project_id = %project_id,
                    token_project_count = token_project_ids.len(),
                    "Token project_ids restriction blocks access"
                );
                return Ok(ProjectAccess::NoAccess);
            }
        }

        // Then check membership-based access
        self.check_project_access(user_id, project_id).await
    }

    /// Get all projects accessible to a user.
    ///
    /// Returns projects where the user has direct membership, group membership,
    /// or is an admin.
    pub async fn get_accessible_projects(&self, user_id: &str, user_role: &str) -> Result<Vec<String>> {
        // Admins have access to all projects
        if user_role == "admin" {
            let projects: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM projects"
            )
            .fetch_all(&self.db)
            .await?;
            return Ok(projects.into_iter().map(|(id,)| id).collect());
        }

        // Non-admins: direct + group-based membership
        let projects: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT project_id FROM (
                -- Direct membership
                SELECT project_id FROM project_members
                WHERE user_id = ?

                UNION

                -- Group-based membership
                SELECT pgm.project_id
                FROM project_group_members pgm
                INNER JOIN group_members gm ON pgm.group_id = gm.group_id
                WHERE gm.user_id = ?
            )
            "#,
        )
        .bind(user_id)
        .bind(user_id)
        .fetch_all(&self.db)
        .await?;

        Ok(projects.into_iter().map(|(id,)| id).collect())
    }

    /// Check if user can manage project members (admin-level operation).
    ///
    /// Only global admins can manage project members.
    pub async fn can_manage_project_members(&self, user_role: &str) -> bool {
        user_role == "admin"
    }
}
