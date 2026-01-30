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
use uuid::Uuid;

use crate::{AppState, Error, Result};

/// Build project routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_projects).post(create_project))
        .route(
            "/:id",
            get(get_project).put(update_project).delete(delete_project),
        )
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Query parameters for listing projects.
#[derive(Debug, Deserialize, Default)]
pub struct ListProjectsQuery {
    /// Filter by name (partial match)
    pub name: Option<String>,
    /// Pagination offset
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit (default 20, max 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Sort field
    #[serde(default)]
    pub sort_by: ProjectSortField,
    /// Sort direction
    #[serde(default)]
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
    /// Local path to the codebase (for indexing)
    pub root_path: Option<String>,
    /// Git repository URL
    pub repo_url: Option<String>,
}

/// Request to update a project.
#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    /// Human-readable name
    pub name: Option<String>,
    /// Project description
    pub description: Option<String>,
    /// Local path to the codebase
    pub root_path: Option<String>,
    /// Git repository URL
    pub repo_url: Option<String>,
}

/// Project response.
#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub root_path: Option<String>,
    pub repo_url: Option<String>,
    pub memory_count: u32,
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
#[axum::debug_handler]
async fn list_projects(
    State(_state): State<AppState>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Json<ListProjectsResponse>> {
    let limit = query.limit.min(100);

    // TODO: Fetch projects from database with filters

    Ok(Json(ListProjectsResponse {
        projects: vec![],
        total: 0,
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
    State(_state): State<AppState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>> {
    // Validate slug format
    if !is_valid_slug(&request.slug) {
        return Err(Error::Validation(
            "Slug must be lowercase alphanumeric with hyphens only".into(),
        ));
    }

    // TODO: Check if slug already exists
    // TODO: Create project in database
    // TODO: Initialize Qdrant collection for project

    let now = Utc::now();
    let project = ProjectResponse {
        id: Uuid::new_v4(),
        slug: request.slug,
        name: request.name,
        description: request.description,
        root_path: request.root_path,
        repo_url: request.repo_url,
        memory_count: 0,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(project))
}

/// Get a project by ID or slug.
///
/// GET /projects/:id
///
/// Returns the full details for a single project.
#[axum::debug_handler]
async fn get_project(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectResponse>> {
    // Try to parse as UUID, otherwise treat as slug
    let _project_id = parse_project_id(&id)?;

    // TODO: Fetch project from database

    Err(Error::NotFound(format!("Project: {}", id)))
}

/// Update a project.
///
/// PUT /projects/:id
///
/// Updates the project with the given ID.
#[axum::debug_handler]
async fn update_project(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>> {
    let _project_id = parse_project_id(&id)?;

    // TODO: Fetch existing project
    // TODO: Apply updates
    // TODO: Save to database

    Err(Error::NotFound(format!("Project: {}", id)))
}

/// Delete a project.
///
/// DELETE /projects/:id
///
/// Deletes the project and all associated data (memories, attachments, etc.).
/// This action is irreversible.
#[axum::debug_handler]
async fn delete_project(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let _project_id = parse_project_id(&id)?;

    // TODO: Delete project and all associated data
    // TODO: Delete Qdrant collection

    Err(Error::NotFound(format!("Project: {}", id)))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a project ID from either UUID or slug format.
fn parse_project_id(id: &str) -> Result<ProjectIdent> {
    if let Ok(uuid) = Uuid::parse_str(id) {
        Ok(ProjectIdent::Id(uuid))
    } else {
        Ok(ProjectIdent::Slug(id.to_string()))
    }
}

/// Project identifier - either UUID or slug.
#[derive(Debug)]
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
