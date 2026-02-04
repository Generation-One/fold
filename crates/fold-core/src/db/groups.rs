//! Group and group membership database queries.
//!
//! Handles user groups, group memberships, and group-based project access.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// Group Types
// ============================================================================

/// Group record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_system: i32, // SQLite stores bool as 0/1
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Group {
    pub fn is_system(&self) -> bool {
        self.is_system != 0
    }
}

/// Input for creating a new group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroup {
    pub name: String,
    pub description: Option<String>,
}

/// Input for updating a group.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateGroup {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl From<crate::api::groups::UpdateGroupRequest> for UpdateGroup {
    fn from(req: crate::api::groups::UpdateGroupRequest) -> Self {
        UpdateGroup {
            name: req.name,
            description: req.description,
        }
    }
}

// ============================================================================
// Group Member Types
// ============================================================================

/// Group membership record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GroupMember {
    pub group_id: String,
    pub user_id: String,
    pub added_by: Option<String>,
    pub created_at: String,
}

// ============================================================================
// Group Queries
// ============================================================================

/// Create a new group.
pub async fn create_group(
    pool: &DbPool,
    id: &str,
    name: &str,
    description: Option<&str>,
    created_by: Option<&str>,
) -> Result<Group> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"
        INSERT INTO groups (id, name, description, created_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(created_by)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get_group(pool, id).await
}

/// Get a group by ID.
pub async fn get_group(pool: &DbPool, id: &str) -> Result<Group> {
    sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Group not found: {}", id)))
}

/// Get a group by name.
pub async fn get_group_by_name(pool: &DbPool, name: &str) -> Result<Option<Group>> {
    sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(Error::Database)
}

/// List all groups.
pub async fn list_groups(pool: &DbPool) -> Result<Vec<Group>> {
    sqlx::query_as::<_, Group>("SELECT * FROM groups ORDER BY name")
        .fetch_all(pool)
        .await
        .map_err(Error::Database)
}

/// Update a group.
pub async fn update_group(pool: &DbPool, id: &str, input: UpdateGroup) -> Result<Group> {
    let mut updates = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(name) = input.name {
        updates.push("name = ?");
        bindings.push(name);
    }
    if let Some(description) = input.description {
        updates.push("description = ?");
        bindings.push(description);
    }

    if updates.is_empty() {
        return get_group(pool, id).await;
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE groups SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, Group>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Group not found: {}", id)))
}

/// Delete a group (cannot delete system groups).
pub async fn delete_group(pool: &DbPool, id: &str) -> Result<()> {
    // Check if it's a system group
    let group = get_group(pool, id).await?;
    if group.is_system() {
        return Err(Error::Forbidden);
    }

    let result = sqlx::query("DELETE FROM groups WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Group not found: {}", id)));
    }

    Ok(())
}

// ============================================================================
// Group Member Queries
// ============================================================================

/// Add a user to a group.
pub async fn add_group_member(
    pool: &DbPool,
    group_id: &str,
    user_id: &str,
    added_by: Option<&str>,
) -> Result<GroupMember> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO group_members (group_id, user_id, added_by, created_at)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .bind(added_by)
    .bind(&now)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, GroupMember>(
        "SELECT * FROM group_members WHERE group_id = ? AND user_id = ?",
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Remove a user from a group.
pub async fn remove_group_member(pool: &DbPool, group_id: &str, user_id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
        .bind(group_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// List all members of a group.
pub async fn list_group_members(pool: &DbPool, group_id: &str) -> Result<Vec<GroupMember>> {
    sqlx::query_as::<_, GroupMember>(
        r#"
        SELECT * FROM group_members
        WHERE group_id = ?
        ORDER BY created_at
        "#,
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List all groups a user belongs to.
pub async fn list_user_groups(pool: &DbPool, user_id: &str) -> Result<Vec<Group>> {
    sqlx::query_as::<_, Group>(
        r#"
        SELECT g.* FROM groups g
        INNER JOIN group_members gm ON g.id = gm.group_id
        WHERE gm.user_id = ?
        ORDER BY g.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Check if a user is in a group.
pub async fn is_user_in_group(pool: &DbPool, group_id: &str, user_id: &str) -> Result<bool> {
    let result =
        sqlx::query("SELECT 1 FROM group_members WHERE group_id = ? AND user_id = ? LIMIT 1")
            .bind(group_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    Ok(result.is_some())
}

/// Get count of admin users in a group (assumes "admin" group ID is "group_admin").
pub async fn count_admin_users(pool: &DbPool, group_id: &str) -> Result<i64> {
    let result: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM group_members WHERE group_id = ?
        "#,
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;

    Ok(result.0)
}

/// Ensure the admin group exists (idempotent).
pub async fn ensure_admin_group(pool: &DbPool) -> Result<Group> {
    // Try to get it first
    if let Ok(group) = get_group(pool, "group_admin").await {
        return Ok(group);
    }

    // Create it if it doesn't exist
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO groups (id, name, description, is_system, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("group_admin")
    .bind("Admins")
    .bind("System administrators with global access")
    .bind(1)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get_group(pool, "group_admin").await
}
