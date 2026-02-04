//! AI session and workspace database queries.
//!
//! Tracks AI agent working sessions and local workspace mappings.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::DbPool;

// ============================================================================
// AI Session Types
// ============================================================================

/// AI session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
    Blocked,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// AI session record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AiSession {
    pub id: String,
    pub project_id: String,
    pub task: String,
    pub status: String,
    pub local_root: Option<String>,
    pub repository_id: Option<String>,
    pub summary: Option<String>,
    pub next_steps: Option<String>, // JSON array
    pub agent_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub ended_at: Option<String>,
}

impl AiSession {
    /// Get status as enum.
    pub fn status_enum(&self) -> Option<SessionStatus> {
        SessionStatus::from_str(&self.status)
    }

    /// Parse next_steps JSON into a vector.
    pub fn next_steps_vec(&self) -> Vec<String> {
        self.next_steps
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// Check if session is ended.
    pub fn is_ended(&self) -> bool {
        self.ended_at.is_some()
    }
}

/// Input for creating an AI session.
#[derive(Debug, Clone)]
pub struct CreateAiSession {
    pub id: String,
    pub project_id: String,
    pub task: String,
    pub local_root: Option<String>,
    pub repository_id: Option<String>,
    pub agent_type: Option<String>,
}

/// Input for updating an AI session.
#[derive(Debug, Clone, Default)]
pub struct UpdateAiSession {
    pub status: Option<SessionStatus>,
    pub summary: Option<String>,
    pub next_steps: Option<Vec<String>>,
}

// ============================================================================
// Session Note Types
// ============================================================================

/// Session note type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteType {
    Decision,
    Blocker,
    Question,
    Progress,
    Finding,
}

impl NoteType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Blocker => "blocker",
            Self::Question => "question",
            Self::Progress => "progress",
            Self::Finding => "finding",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "decision" => Some(Self::Decision),
            "blocker" => Some(Self::Blocker),
            "question" => Some(Self::Question),
            "progress" => Some(Self::Progress),
            "finding" => Some(Self::Finding),
            _ => None,
        }
    }
}

/// Session note record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AiSessionNote {
    pub id: String,
    pub session_id: String,
    #[sqlx(rename = "type")]
    pub note_type: String,
    pub content: String,
    pub created_at: String,
}

impl AiSessionNote {
    /// Get note type as enum.
    pub fn note_type_enum(&self) -> Option<NoteType> {
        NoteType::from_str(&self.note_type)
    }
}

/// Input for creating a session note.
#[derive(Debug, Clone)]
pub struct CreateSessionNote {
    pub id: String,
    pub session_id: String,
    pub note_type: NoteType,
    pub content: String,
}

// ============================================================================
// Workspace Types
// ============================================================================

/// Workspace mapping record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub project_id: String,
    pub token_id: String,
    pub local_root: String,
    pub repository_id: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

impl Workspace {
    /// Check if workspace is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ref expires) = self.expires_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(expires) {
                return dt < chrono::Utc::now();
            }
        }
        false
    }
}

/// Input for creating a workspace.
#[derive(Debug, Clone)]
pub struct CreateWorkspace {
    pub id: String,
    pub project_id: String,
    pub token_id: String,
    pub local_root: String,
    pub repository_id: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ============================================================================
// Team Status Types
// ============================================================================

/// Team member status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamMemberStatus {
    Active,
    Idle,
    Away,
}

impl TeamMemberStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Away => "away",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "away" => Self::Away,
            _ => Self::Idle,
        }
    }
}

/// Team status record from the database.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TeamStatus {
    pub id: String,
    pub project_id: String,
    pub username: String,
    pub status: String,
    pub current_task: Option<String>,
    pub current_files: Option<String>, // JSON array
    pub last_seen: String,
    pub session_start: Option<String>,
}

impl TeamStatus {
    /// Get status as enum.
    pub fn status_enum(&self) -> TeamMemberStatus {
        TeamMemberStatus::from_str(&self.status)
    }

    /// Parse current_files JSON into a vector.
    pub fn current_files_vec(&self) -> Vec<String> {
        self.current_files
            .as_ref()
            .and_then(|f| serde_json::from_str(f).ok())
            .unwrap_or_default()
    }
}

/// Input for updating team status.
#[derive(Debug, Clone)]
pub struct UpdateTeamStatus {
    pub status: TeamMemberStatus,
    pub current_task: Option<String>,
    pub current_files: Option<Vec<String>>,
}

// ============================================================================
// AI Session Queries
// ============================================================================

/// Create a new AI session.
pub async fn create_ai_session(pool: &DbPool, input: CreateAiSession) -> Result<AiSession> {
    sqlx::query_as::<_, AiSession>(
        r#"
        INSERT INTO ai_sessions (id, project_id, task, local_root, repository_id, agent_type)
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(&input.task)
    .bind(&input.local_root)
    .bind(&input.repository_id)
    .bind(&input.agent_type)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get an AI session by ID.
pub async fn get_ai_session(pool: &DbPool, id: &str) -> Result<AiSession> {
    sqlx::query_as::<_, AiSession>("SELECT * FROM ai_sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("AI session not found: {}", id)))
}

/// Update an AI session.
pub async fn update_ai_session(
    pool: &DbPool,
    id: &str,
    input: UpdateAiSession,
) -> Result<AiSession> {
    let mut updates = Vec::new();
    let mut bindings: Vec<Option<String>> = Vec::new();

    if let Some(status) = input.status {
        updates.push("status = ?");
        bindings.push(Some(status.as_str().to_string()));
    }
    if let Some(summary) = input.summary {
        updates.push("summary = ?");
        bindings.push(Some(summary));
    }
    if let Some(next_steps) = input.next_steps {
        updates.push("next_steps = ?");
        bindings.push(Some(serde_json::to_string(&next_steps)?));
    }

    if updates.is_empty() {
        return get_ai_session(pool, id).await;
    }

    updates.push("updated_at = datetime('now')");

    let query = format!(
        "UPDATE ai_sessions SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut q = sqlx::query_as::<_, AiSession>(&query);
    for binding in &bindings {
        q = q.bind(binding);
    }
    q = q.bind(id);

    q.fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("AI session not found: {}", id)))
}

/// End an AI session.
pub async fn end_ai_session(pool: &DbPool, id: &str, summary: Option<&str>) -> Result<AiSession> {
    sqlx::query_as::<_, AiSession>(
        r#"
        UPDATE ai_sessions SET
            status = 'completed',
            summary = COALESCE(?, summary),
            ended_at = datetime('now'),
            updated_at = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
    )
    .bind(summary)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::NotFound(format!("AI session not found: {}", id)))
}

/// Delete an AI session.
pub async fn delete_ai_session(pool: &DbPool, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM ai_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("AI session not found: {}", id)));
    }

    Ok(())
}

/// List AI sessions for a project.
/// Uses idx_ai_sessions_project index.
pub async fn list_project_ai_sessions(
    pool: &DbPool,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<AiSession>> {
    sqlx::query_as::<_, AiSession>(
        r#"
        SELECT * FROM ai_sessions
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

/// List active AI sessions for a project.
/// Uses idx_ai_sessions_status index.
pub async fn list_active_ai_sessions(pool: &DbPool, project_id: &str) -> Result<Vec<AiSession>> {
    sqlx::query_as::<_, AiSession>(
        r#"
        SELECT * FROM ai_sessions
        WHERE project_id = ? AND status = 'active'
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List AI sessions by status.
pub async fn list_ai_sessions_by_status(
    pool: &DbPool,
    status: SessionStatus,
) -> Result<Vec<AiSession>> {
    sqlx::query_as::<_, AiSession>(
        r#"
        SELECT * FROM ai_sessions
        WHERE status = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(status.as_str())
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

// ============================================================================
// Session Note Queries
// ============================================================================

/// Create a session note.
pub async fn create_session_note(pool: &DbPool, input: CreateSessionNote) -> Result<AiSessionNote> {
    sqlx::query_as::<_, AiSessionNote>(
        r#"
        INSERT INTO ai_session_notes (id, session_id, type, content)
        VALUES (?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.session_id)
    .bind(input.note_type.as_str())
    .bind(&input.content)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get a session note by ID.
pub async fn get_session_note(pool: &DbPool, id: &str) -> Result<AiSessionNote> {
    sqlx::query_as::<_, AiSessionNote>("SELECT * FROM ai_session_notes WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Session note not found: {}", id)))
}

/// List notes for a session.
/// Uses idx_session_notes_session index.
pub async fn list_session_notes(pool: &DbPool, session_id: &str) -> Result<Vec<AiSessionNote>> {
    sqlx::query_as::<_, AiSessionNote>(
        r#"
        SELECT * FROM ai_session_notes
        WHERE session_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List notes by type for a session.
pub async fn list_session_notes_by_type(
    pool: &DbPool,
    session_id: &str,
    note_type: NoteType,
) -> Result<Vec<AiSessionNote>> {
    sqlx::query_as::<_, AiSessionNote>(
        r#"
        SELECT * FROM ai_session_notes
        WHERE session_id = ? AND type = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .bind(note_type.as_str())
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Delete a session note.
pub async fn delete_session_note(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM ai_session_notes WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Workspace Queries
// ============================================================================

/// Create a workspace.
pub async fn create_workspace(pool: &DbPool, input: CreateWorkspace) -> Result<Workspace> {
    let expires_at = input.expires_at.map(|dt| dt.to_rfc3339());

    sqlx::query_as::<_, Workspace>(
        r#"
        INSERT INTO workspaces (id, project_id, token_id, local_root, repository_id, expires_at)
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING *
        "#,
    )
    .bind(&input.id)
    .bind(&input.project_id)
    .bind(&input.token_id)
    .bind(&input.local_root)
    .bind(&input.repository_id)
    .bind(&expires_at)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get a workspace by ID.
pub async fn get_workspace(pool: &DbPool, id: &str) -> Result<Workspace> {
    sqlx::query_as::<_, Workspace>("SELECT * FROM workspaces WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Workspace not found: {}", id)))
}

/// Get workspaces for a project.
/// Uses idx_workspaces_project index.
pub async fn list_project_workspaces(pool: &DbPool, project_id: &str) -> Result<Vec<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        r#"
        SELECT * FROM workspaces
        WHERE project_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Get workspaces for a token.
/// Uses idx_workspaces_token index.
pub async fn list_token_workspaces(pool: &DbPool, token_id: &str) -> Result<Vec<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        r#"
        SELECT * FROM workspaces
        WHERE token_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(token_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Find workspace by local root.
pub async fn get_workspace_by_local_root(
    pool: &DbPool,
    token_id: &str,
    local_root: &str,
) -> Result<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        r#"
        SELECT * FROM workspaces
        WHERE token_id = ? AND local_root = ?
        "#,
    )
    .bind(token_id)
    .bind(local_root)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// Delete a workspace.
pub async fn delete_workspace(pool: &DbPool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete expired workspaces.
pub async fn cleanup_expired_workspaces(pool: &DbPool) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM workspaces WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// ============================================================================
// Team Status Queries
// ============================================================================

/// Upsert team status (update if exists, insert if not).
pub async fn upsert_team_status(
    pool: &DbPool,
    project_id: &str,
    username: &str,
    input: UpdateTeamStatus,
) -> Result<TeamStatus> {
    let current_files_json = input
        .current_files
        .map(|f| serde_json::to_string(&f).unwrap_or_default());
    let id = format!("{}-{}", project_id, username);

    sqlx::query_as::<_, TeamStatus>(
        r#"
        INSERT INTO team_status (id, project_id, username, status, current_task, current_files, session_start)
        VALUES (?, ?, ?, ?, ?, ?, CASE WHEN ? = 'active' THEN datetime('now') ELSE NULL END)
        ON CONFLICT(project_id, username) DO UPDATE SET
            status = excluded.status,
            current_task = excluded.current_task,
            current_files = excluded.current_files,
            last_seen = datetime('now'),
            session_start = CASE
                WHEN excluded.status = 'active' AND team_status.status != 'active'
                THEN datetime('now')
                ELSE team_status.session_start
            END
        RETURNING *
        "#,
    )
    .bind(&id)
    .bind(project_id)
    .bind(username)
    .bind(input.status.as_str())
    .bind(&input.current_task)
    .bind(&current_files_json)
    .bind(input.status.as_str())
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

/// Get team status for a user.
pub async fn get_team_status(
    pool: &DbPool,
    project_id: &str,
    username: &str,
) -> Result<Option<TeamStatus>> {
    sqlx::query_as::<_, TeamStatus>(
        r#"
        SELECT * FROM team_status
        WHERE project_id = ? AND username = ?
        "#,
    )
    .bind(project_id)
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

/// List team status for a project.
/// Uses idx_team_status_project index.
pub async fn list_team_status(pool: &DbPool, project_id: &str) -> Result<Vec<TeamStatus>> {
    sqlx::query_as::<_, TeamStatus>(
        r#"
        SELECT * FROM team_status
        WHERE project_id = ?
        ORDER BY last_seen DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// List active team members for a project.
pub async fn list_active_team_members(pool: &DbPool, project_id: &str) -> Result<Vec<TeamStatus>> {
    sqlx::query_as::<_, TeamStatus>(
        r#"
        SELECT * FROM team_status
        WHERE project_id = ? AND status = 'active'
        ORDER BY last_seen DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)
}

/// Delete team status entry.
pub async fn delete_team_status(pool: &DbPool, project_id: &str, username: &str) -> Result<()> {
    sqlx::query("DELETE FROM team_status WHERE project_id = ? AND username = ?")
        .bind(project_id)
        .bind(username)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark stale team members as idle (not seen in given minutes).
pub async fn mark_stale_members_idle(pool: &DbPool, minutes: i64) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE team_status SET status = 'idle'
        WHERE status = 'active'
        AND datetime(last_seen, '+' || ? || ' minutes') < datetime('now')
        "#,
    )
    .bind(minutes)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        create_api_token, create_project, create_user, init_pool, migrate, CreateApiToken,
        CreateProject, CreateUser, UserRole,
    };

    async fn setup_test_db() -> DbPool {
        let pool = init_pool(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();

        create_project(
            &pool,
            CreateProject {
                id: "proj-1".to_string(),
                slug: "test".to_string(),
                name: "Test".to_string(),
                description: None,
            },
        )
        .await
        .unwrap();

        create_user(
            &pool,
            CreateUser {
                id: "user-1".to_string(),
                provider: "google".to_string(),
                subject: "sub-1".to_string(),
                email: None,
                display_name: None,
                avatar_url: None,
                role: UserRole::Member,
            },
        )
        .await
        .unwrap();

        create_api_token(
            &pool,
            CreateApiToken {
                id: "token-1".to_string(),
                user_id: "user-1".to_string(),
                name: "Test Token".to_string(),
                token_hash: "hash123".to_string(),
                token_prefix: "fold_".to_string(),
                project_ids: vec!["proj-1".to_string()],
                expires_at: None,
            },
        )
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_ai_session() {
        let pool = setup_test_db().await;

        let session = create_ai_session(
            &pool,
            CreateAiSession {
                id: "sess-1".to_string(),
                project_id: "proj-1".to_string(),
                task: "Implement auth feature".to_string(),
                local_root: Some("/home/user/project".to_string()),
                repository_id: None,
                agent_type: Some("claude-code".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(session.id, "sess-1");
        assert!(session.is_active());
        assert!(!session.is_ended());

        let fetched = get_ai_session(&pool, "sess-1").await.unwrap();
        assert_eq!(fetched.task, "Implement auth feature");
    }

    #[tokio::test]
    async fn test_session_notes() {
        let pool = setup_test_db().await;

        create_ai_session(
            &pool,
            CreateAiSession {
                id: "sess-1".to_string(),
                project_id: "proj-1".to_string(),
                task: "Test task".to_string(),
                local_root: None,
                repository_id: None,
                agent_type: None,
            },
        )
        .await
        .unwrap();

        create_session_note(
            &pool,
            CreateSessionNote {
                id: "note-1".to_string(),
                session_id: "sess-1".to_string(),
                note_type: NoteType::Decision,
                content: "Decided to use REST API".to_string(),
            },
        )
        .await
        .unwrap();

        create_session_note(
            &pool,
            CreateSessionNote {
                id: "note-2".to_string(),
                session_id: "sess-1".to_string(),
                note_type: NoteType::Progress,
                content: "Completed initial setup".to_string(),
            },
        )
        .await
        .unwrap();

        let notes = list_session_notes(&pool, "sess-1").await.unwrap();
        assert_eq!(notes.len(), 2);

        let decisions = list_session_notes_by_type(&pool, "sess-1", NoteType::Decision)
            .await
            .unwrap();
        assert_eq!(decisions.len(), 1);
    }

    #[tokio::test]
    async fn test_workspace() {
        let pool = setup_test_db().await;

        let workspace = create_workspace(
            &pool,
            CreateWorkspace {
                id: "ws-1".to_string(),
                project_id: "proj-1".to_string(),
                token_id: "token-1".to_string(),
                local_root: "/home/user/project".to_string(),
                repository_id: None,
                expires_at: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(workspace.id, "ws-1");
        assert!(!workspace.is_expired());

        let fetched = get_workspace(&pool, "ws-1").await.unwrap();
        assert_eq!(fetched.local_root, "/home/user/project");

        let by_root = get_workspace_by_local_root(&pool, "token-1", "/home/user/project")
            .await
            .unwrap();
        assert!(by_root.is_some());
    }

    #[tokio::test]
    async fn test_team_status() {
        let pool = setup_test_db().await;

        let status = upsert_team_status(
            &pool,
            "proj-1",
            "alice",
            UpdateTeamStatus {
                status: TeamMemberStatus::Active,
                current_task: Some("Working on auth".to_string()),
                current_files: Some(vec!["src/auth.rs".to_string()]),
            },
        )
        .await
        .unwrap();

        assert_eq!(status.username, "alice");
        assert_eq!(status.status_enum(), TeamMemberStatus::Active);
        assert_eq!(status.current_files_vec(), vec!["src/auth.rs"]);

        let active = list_active_team_members(&pool, "proj-1").await.unwrap();
        assert_eq!(active.len(), 1);
    }
}
