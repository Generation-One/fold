//! Projects Routes
//!
//! CRUD operations for projects in the Fold system.
//!
//! Routes:
//! - GET /projects - List all projects
//! - POST /projects - Create a new project
//! - GET /projects/:id - Get project details
//! - PUT /projects/:id - Update project
//! - DELETE /projects/:id - Delete project

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build project routes.
pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_projects).post(create_project))
        .route(
            "/:id",
            get(get_project).put(update_project).delete(delete_project),
        )
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::middleware::require_auth,
        ))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Query parameters for listing projects.
#[derive(Debug, Deserialize, Default)]
pub struct ListProjectsQuery {
    /// Filter by name (partial match)
    #[allow(dead_code)]
    pub name: Option<String>,
    /// Pagination offset
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit (default 20, max 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Sort field
    #[serde(default)]
    #[allow(dead_code)]
    pub sort_by: ProjectSortField,
    /// Sort direction
    #[serde(default)]
    #[allow(dead_code)]
    pub sort_dir: SortDirection,
}

fn default_limit() -> u32 {
    20
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSortField {
    #[default]
    Name,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Request to create a new project.
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    /// Unique slug for the project (URL-friendly)
    pub slug: String,
    /// Human-readable name
    pub name: String,
    /// Project description
    pub description: Option<String>,
}

/// Request to update a project.
#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    /// Human-readable name
    pub name: Option<String>,
    /// Project description
    pub description: Option<String>,
    /// Author patterns to ignore during webhook processing (prevents loops)
    pub ignored_commit_authors: Option<Vec<String>>,
}

/// Project response.
#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub memory_count: u32,
    /// Author patterns to ignore during webhook processing
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_commit_authors: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List of projects response.
#[derive(Debug, Serialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectResponse>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

// ============================================================================
// Handlers
// ============================================================================

/// List all projects.
///
/// GET /projects
///
/// Returns a paginated list of projects the user has access to.
/// Users see only projects they have direct or group membership in.
/// Admins see all projects.
#[axum::debug_handler]
async fn list_projects(
    State(state): State<AppState>,
    Query(query): Query<ListProjectsQuery>,
    axum::extract::Extension(auth): axum::extract::Extension<crate::middleware::AuthUser>,
) -> Result<Json<ListProjectsResponse>> {
    let limit = query.limit.min(100);

    // Admin users see all projects
    if auth.is_admin() {
        // Fetch all projects from database
        let projects =
            crate::db::list_projects_paginated(&state.db, limit as i64, query.offset as i64)
                .await?;

        // Get total count
        let total = crate::db::count_projects(&state.db).await.unwrap_or(0) as u32;

        let project_responses: Vec<ProjectResponse> = projects
            .into_iter()
            .map(|p| {
                let ignored_authors = p
                    .ignored_commit_authors
                    .as_ref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                ProjectResponse {
                    id: p.id.parse().unwrap_or_default(),
                    slug: p.slug,
                    name: p.name,
                    description: p.description,
                    memory_count: 0,
                    ignored_commit_authors: ignored_authors,
                    created_at: p.created_at.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: p.updated_at.parse().unwrap_or_else(|_| Utc::now()),
                }
            })
            .collect();

        return Ok(Json(ListProjectsResponse {
            projects: project_responses,
            total,
            offset: query.offset,
            limit,
        }));
    }

    // Non-admin users: get projects they have access to
    let perm_service = crate::services::PermissionService::new(state.db.clone());
    let accessible_projects = perm_service
        .get_accessible_projects(&auth.user_id, &auth.role)
        .await?;

    // Fetch all projects and filter to accessible ones
    let all_projects = crate::db::list_projects_paginated(
        &state.db, 10000, // Large number to get all
        0,
    )
    .await?;

    let accessible_project_ids: std::collections::HashSet<String> =
        accessible_projects.into_iter().collect();

    let filtered_projects: Vec<_> = all_projects
        .into_iter()
        .filter(|p| accessible_project_ids.contains(&p.id))
        .collect();

    // Apply pagination to filtered results
    let total = filtered_projects.len() as u32;
    let paginated = filtered_projects
        .into_iter()
        .skip(query.offset as usize)
        .take(limit as usize)
        .collect::<Vec<_>>();

    let project_responses: Vec<ProjectResponse> = paginated
        .into_iter()
        .map(|p| {
            let ignored_authors = p
                .ignored_commit_authors
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            ProjectResponse {
                id: p.id.parse().unwrap_or_default(),
                slug: p.slug,
                name: p.name,
                description: p.description,
                memory_count: 0,
                ignored_commit_authors: ignored_authors,
                created_at: p.created_at.parse().unwrap_or_else(|_| Utc::now()),
                updated_at: p.updated_at.parse().unwrap_or_else(|_| Utc::now()),
            }
        })
        .collect();

    Ok(Json(ListProjectsResponse {
        projects: project_responses,
        total,
        offset: query.offset,
        limit,
    }))
}

/// Create a new project.
///
/// POST /projects
///
/// Creates a new project with the given details.
#[axum::debug_handler]
async fn create_project(
    State(state): State<AppState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>> {
    // Validate slug format
    if !is_valid_slug(&request.slug) {
        return Err(Error::Validation(
            "Slug must be lowercase alphanumeric with hyphens only".into(),
        ));
    }

    // Create project in database
    let input = crate::db::CreateProject {
        id: Uuid::new_v4().to_string(),
        slug: request.slug,
        name: request.name,
        description: request.description,
    };

    let project = crate::db::create_project(&state.db, input).await?;

    // Initialize Qdrant collection for project (non-blocking)
    match state
        .qdrant
        .create_collection(&project.slug, state.embeddings.dimension().await)
        .await
    {
        Ok(()) => info!(slug = %project.slug, "Created Qdrant collection"),
        Err(e) => {
            warn!(error = %e, slug = %project.slug, "Failed to create Qdrant collection, search unavailable")
        }
    }

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(ProjectResponse {
        id: project.id.parse().unwrap_or_default(),
        slug: project.slug,
        name: project.name,
        description: project.description,
        memory_count: 0,
        ignored_commit_authors: ignored_authors,
        created_at: project.created_at.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: project.updated_at.parse().unwrap_or_else(|_| Utc::now()),
    }))
}

/// Get a project by ID or slug.
///
/// GET /projects/:id
///
/// Returns the full details for a single project.
#[axum::debug_handler]
async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(ProjectResponse {
        id: project.id.parse().unwrap_or_default(),
        slug: project.slug,
        name: project.name,
        description: project.description,
        memory_count: 0,
        ignored_commit_authors: ignored_authors,
        created_at: project.created_at.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: project.updated_at.parse().unwrap_or_else(|_| Utc::now()),
    }))
}

/// Update a project.
///
/// PUT /projects/:id
///
/// Updates the project with the given ID.
#[axum::debug_handler]
async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>> {
    // First fetch the project to get its actual ID (in case user passed a slug)
    let existing = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    // Serialize ignored_commit_authors if provided
    let ignored_authors_json = request
        .ignored_commit_authors
        .map(|authors| serde_json::to_string(&authors).ok())
        .flatten();

    let input = crate::db::UpdateProject {
        slug: None, // Don't allow slug changes via this endpoint
        name: request.name,
        description: request.description,
        ignored_commit_authors: ignored_authors_json,
    };

    let project = crate::db::update_project(&state.db, &existing.id, input).await?;

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(ProjectResponse {
        id: project.id.parse().unwrap_or_default(),
        slug: project.slug,
        name: project.name,
        description: project.description,
        memory_count: 0,
        ignored_commit_authors: ignored_authors,
        created_at: project.created_at.parse().unwrap_or_else(|_| Utc::now()),
        updated_at: project.updated_at.parse().unwrap_or_else(|_| Utc::now()),
    }))
}

/// Delete a project.
///
/// DELETE /projects/:id
///
/// Deletes the project and all associated data (memories, attachments, etc.).
/// This action is irreversible.
#[axum::debug_handler]
async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // First fetch the project to get its actual ID (in case user passed a slug)
    let existing = crate::db::get_project_by_id_or_slug(&state.db, &id).await?;

    // Delete project and all associated data (cascade handled by DB)
    crate::db::delete_project(&state.db, &existing.id).await?;

    // Delete Qdrant collection (non-blocking cleanup)
    match state.qdrant.delete_collection(&existing.slug).await {
        Ok(()) => info!(slug = %existing.slug, "Deleted Qdrant collection"),
        Err(e) => warn!(error = %e, slug = %existing.slug, "Failed to delete Qdrant collection"),
    }

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": existing.id
    })))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a project ID from either UUID or slug format.
#[allow(dead_code)]
fn parse_project_id(id: &str) -> Result<ProjectIdent> {
    if let Ok(uuid) = Uuid::parse_str(id) {
        Ok(ProjectIdent::Id(uuid))
    } else {
        Ok(ProjectIdent::Slug(id.to_string()))
    }
}

/// Project identifier - either UUID or slug.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ProjectIdent {
    Id(Uuid),
    Slug(String),
}

/// Check if a string is a valid project slug.
fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

// ============================================================================
// Project Members Routes
// ============================================================================

/// Build project members routes.
///
/// These routes use the full path pattern /:project_id/members
pub fn members_routes() -> Router<AppState> {
    Router::new()
        .route("/:project_id/members", get(list_members).post(add_member))
        .route(
            "/:project_id/members/:user_id",
            get(get_member).put(update_member).delete(remove_member),
        )
}

// ============================================================================
// Member Request/Response Types
// ============================================================================

/// Request to add a member to a project.
#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    /// User ID to add
    pub user_id: String,
    /// Role: "member" (read/write) or "viewer" (read-only)
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "viewer".to_string()
}

/// Request to update a member's role.
#[derive(Debug, Deserialize)]
pub struct UpdateMemberRequest {
    /// Role: "member" (read/write) or "viewer" (read-only)
    pub role: String,
}

/// Member response.
#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub project_id: String,
    pub role: String,
    pub can_write: bool,
    pub added_by: Option<String>,
    pub created_at: DateTime<Utc>,
    // User info (when available)
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// List members response.
#[derive(Debug, Serialize)]
pub struct ListMembersResponse {
    pub members: Vec<MemberResponse>,
    pub total: u32,
}

// ============================================================================
// Member Handlers
// ============================================================================

/// List all members of a project.
///
/// GET /projects/:project_id/members
#[axum::debug_handler]
async fn list_members(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<ListMembersResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Get members with user details
    let members = crate::db::list_project_members_with_users(&state.db, &project.id).await?;

    let member_responses: Vec<MemberResponse> = members
        .into_iter()
        .map(|m| MemberResponse {
            user_id: m.user_id,
            project_id: m.project_id,
            role: m.role.clone(),
            can_write: m.role == "member",
            added_by: m.added_by,
            created_at: m.created_at.parse().unwrap_or_else(|_| Utc::now()),
            email: m.email,
            display_name: m.display_name,
            avatar_url: m.avatar_url,
        })
        .collect();

    let total = member_responses.len() as u32;

    Ok(Json(ListMembersResponse {
        members: member_responses,
        total,
    }))
}

/// Add a member to a project.
///
/// POST /projects/:project_id/members
#[axum::debug_handler]
async fn add_member(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    axum::Extension(auth): axum::Extension<crate::middleware::AuthContext>,
    Json(request): Json<AddMemberRequest>,
) -> Result<Json<MemberResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Verify user exists
    let _ = crate::db::get_user(&state.db, &request.user_id).await?;

    // Validate role
    if request.role != "member" && request.role != "viewer" {
        return Err(Error::Validation(
            "Role must be 'member' or 'viewer'".into(),
        ));
    }

    let added_by = Some(auth.user_id.as_str());

    let member = crate::db::add_project_member(
        &state.db,
        &project.id,
        &request.user_id,
        &request.role,
        added_by,
    )
    .await?;

    Ok(Json(MemberResponse {
        user_id: member.user_id,
        project_id: member.project_id,
        role: member.role.clone(),
        can_write: member.role == "member",
        added_by: member.added_by,
        created_at: member.created_at.parse().unwrap_or_else(|_| Utc::now()),
        email: None,
        display_name: None,
        avatar_url: None,
    }))
}

/// Get a specific member of a project.
///
/// GET /projects/:project_id/members/:user_id
#[axum::debug_handler]
async fn get_member(
    State(state): State<AppState>,
    Path((project_id, user_id)): Path<(String, String)>,
) -> Result<Json<MemberResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    let member = crate::db::get_project_member(&state.db, &project.id, &user_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Member not found: {}", user_id)))?;

    Ok(Json(MemberResponse {
        user_id: member.user_id,
        project_id: member.project_id,
        role: member.role.clone(),
        can_write: member.role == "member",
        added_by: member.added_by,
        created_at: member.created_at.parse().unwrap_or_else(|_| Utc::now()),
        email: None,
        display_name: None,
        avatar_url: None,
    }))
}

/// Update a member's role.
///
/// PUT /projects/:project_id/members/:user_id
#[axum::debug_handler]
async fn update_member(
    State(state): State<AppState>,
    Path((project_id, user_id)): Path<(String, String)>,
    Json(request): Json<UpdateMemberRequest>,
) -> Result<Json<MemberResponse>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Validate role
    if request.role != "member" && request.role != "viewer" {
        return Err(Error::Validation(
            "Role must be 'member' or 'viewer'".into(),
        ));
    }

    let member =
        crate::db::update_project_member_role(&state.db, &project.id, &user_id, &request.role)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Member not found: {}", user_id)))?;

    Ok(Json(MemberResponse {
        user_id: member.user_id,
        project_id: member.project_id,
        role: member.role.clone(),
        can_write: member.role == "member",
        added_by: member.added_by,
        created_at: member.created_at.parse().unwrap_or_else(|_| Utc::now()),
        email: None,
        display_name: None,
        avatar_url: None,
    }))
}

/// Remove a member from a project.
///
/// DELETE /projects/:project_id/members/:user_id
#[axum::debug_handler]
async fn remove_member(
    State(state): State<AppState>,
    Path((project_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    // Verify project exists
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    let removed = crate::db::remove_project_member(&state.db, &project.id, &user_id).await?;

    if !removed {
        return Err(Error::NotFound(format!("Member not found: {}", user_id)));
    }

    Ok(Json(serde_json::json!({
        "removed": true,
        "user_id": user_id,
        "project_id": project.id
    })))
}

// ============================================================================
// Algorithm Configuration Routes
// ============================================================================

/// Build algorithm configuration routes.
///
/// These are mounted under /projects/:project_id/config
pub fn config_routes() -> Router<AppState> {
    Router::new().route(
        "/algorithm",
        get(get_algorithm_config).put(update_algorithm_config),
    )
}

// ============================================================================
// Algorithm Config Request/Response Types
// ============================================================================

/// Algorithm configuration response.
#[derive(Debug, Serialize)]
pub struct AlgorithmConfigResponse {
    /// Weight for retrieval strength vs semantic similarity (0.0-1.0)
    /// 0.0 = pure semantic, 1.0 = pure strength-based
    pub strength_weight: f64,
    /// Half-life in days for memory decay
    /// Shorter values favour very recent memories
    pub decay_half_life_days: f64,
    /// Author patterns to ignore during webhook processing
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_commit_authors: Vec<String>,
}

/// Request to update algorithm configuration.
#[derive(Debug, Deserialize)]
pub struct UpdateAlgorithmConfigRequest {
    /// Weight for retrieval strength vs semantic similarity (0.0-1.0)
    pub strength_weight: Option<f64>,
    /// Half-life in days for memory decay
    pub decay_half_life_days: Option<f64>,
    /// Author patterns to ignore during webhook processing
    pub ignored_commit_authors: Option<Vec<String>>,
}

// ============================================================================
// Algorithm Config Handlers
// ============================================================================

/// Get algorithm configuration for a project.
///
/// GET /projects/:project_id/config/algorithm
///
/// Returns the decay algorithm parameters and ignored commit authors.
#[axum::debug_handler]
async fn get_algorithm_config(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<AlgorithmConfigResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    let ignored_authors = project
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(AlgorithmConfigResponse {
        strength_weight: project.decay_strength_weight.unwrap_or(0.3),
        decay_half_life_days: project.decay_half_life_days.unwrap_or(30.0),
        ignored_commit_authors: ignored_authors,
    }))
}

/// Update algorithm configuration for a project.
///
/// PUT /projects/:project_id/config/algorithm
///
/// Updates the decay algorithm parameters and/or ignored commit authors.
#[axum::debug_handler]
async fn update_algorithm_config(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateAlgorithmConfigRequest>,
) -> Result<Json<AlgorithmConfigResponse>> {
    let project = crate::db::get_project_by_id_or_slug(&state.db, &project_id).await?;

    // Validate strength_weight range
    if let Some(weight) = request.strength_weight {
        if !(0.0..=1.0).contains(&weight) {
            return Err(Error::Validation(
                "strength_weight must be between 0.0 and 1.0".into(),
            ));
        }
    }

    // Validate decay_half_life_days
    if let Some(half_life) = request.decay_half_life_days {
        if half_life < 1.0 {
            return Err(Error::Validation(
                "decay_half_life_days must be at least 1.0".into(),
            ));
        }
    }

    // Build update
    let input = crate::db::UpdateAlgorithmConfig {
        decay_strength_weight: request.strength_weight,
        decay_half_life_days: request.decay_half_life_days,
        ignored_commit_authors: request
            .ignored_commit_authors
            .map(|authors| serde_json::to_string(&authors).ok())
            .flatten(),
    };

    let updated = crate::db::update_algorithm_config(&state.db, &project.id, input).await?;

    let ignored_authors = updated
        .ignored_commit_authors
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(Json(AlgorithmConfigResponse {
        strength_weight: updated.decay_strength_weight.unwrap_or(0.3),
        decay_half_life_days: updated.decay_half_life_days.unwrap_or(30.0),
        ignored_commit_authors: ignored_authors,
    }))
}
