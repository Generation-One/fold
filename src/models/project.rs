//! Project model for organizing memories.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

// ============================================================================
// Project Member Access Control
// ============================================================================

/// Role for project-level access control.
/// - Owner: Full access (set via project.owner field, not this table)
/// - Member: Read + Write access
/// - Viewer: Read-only access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    Member,
    #[default]
    Viewer,
}

impl ProjectRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectRole::Member => "member",
            ProjectRole::Viewer => "viewer",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "member" => Some(ProjectRole::Member),
            "viewer" => Some(ProjectRole::Viewer),
            _ => None,
        }
    }

    /// Check if this role can write (create/update/delete memories)
    pub fn can_write(&self) -> bool {
        matches!(self, ProjectRole::Member)
    }

    /// Check if this role can read
    pub fn can_read(&self) -> bool {
        true // Both roles can read
    }
}

/// A user's membership in a project with their role.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct ProjectMember {
    pub user_id: String,
    pub project_id: String,
    pub role: String,
    pub added_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ProjectMember {
    pub fn new(
        user_id: String,
        project_id: String,
        role: ProjectRole,
        added_by: Option<String>,
    ) -> Self {
        Self {
            user_id,
            project_id,
            role: role.as_str().to_string(),
            added_by,
            created_at: Utc::now(),
        }
    }

    pub fn get_role(&self) -> Option<ProjectRole> {
        ProjectRole::from_str(&self.role)
    }

    pub fn can_write(&self) -> bool {
        self.get_role().map(|r| r.can_write()).unwrap_or(false)
    }

    pub fn can_read(&self) -> bool {
        true
    }
}

// ============================================================================
// Git Provider
// ============================================================================

/// Git provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitProvider {
    GitHub,
    GitLab,
}

impl GitProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            GitProvider::GitHub => "github",
            GitProvider::GitLab => "gitlab",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "github" => Some(GitProvider::GitHub),
            "gitlab" => Some(GitProvider::GitLab),
            _ => None,
        }
    }
}

// ============================================================================
// Meta Storage Type
// ============================================================================

/// Meta storage location type.
/// Currently only Internal is supported - content is stored in the fold/ directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetaStorageType {
    /// Store in fold/ directory (default, always enabled)
    #[default]
    Internal,
    // External storage is disabled for now
    // External,
}

impl MetaStorageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetaStorageType::Internal => "internal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "internal" => Some(MetaStorageType::Internal),
            // "external" is no longer supported
            _ => Some(MetaStorageType::Internal), // Default to internal
        }
    }
}

/// Request model for creating a project
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectCreate {
    pub name: String,
    pub description: Option<String>,

    #[serde(default = "default_index_patterns")]
    pub index_patterns: Vec<String>,
    #[serde(default = "default_ignore_patterns")]
    pub ignore_patterns: Vec<String>,

    // Team settings
    #[serde(default)]
    pub team_members: Vec<String>,

    // Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    // Meta storage configuration
    #[serde(default)]
    pub meta_storage_type: MetaStorageType,
    pub meta_path: Option<String>,
    pub meta_source_id: Option<String>,
    pub meta_source_config: Option<serde_json::Value>,
}

fn default_index_patterns() -> Vec<String> {
    vec![
        "**/*.py".to_string(),
        "**/*.ts".to_string(),
        "**/*.js".to_string(),
        "**/*.tsx".to_string(),
        "**/*.jsx".to_string(),
        "**/*.cs".to_string(),
        "**/*.java".to_string(),
        "**/*.go".to_string(),
        "**/*.rs".to_string(),
        "**/*.rb".to_string(),
        "**/*.swift".to_string(),
        "**/*.kt".to_string(),
        "**/*.c".to_string(),
        "**/*.cpp".to_string(),
        "**/*.h".to_string(),
        "**/*.hpp".to_string(),
        "**/*.md".to_string(),
    ]
}

fn default_ignore_patterns() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/.git/**".to_string(),
        "**/dist/**".to_string(),
        "**/build/**".to_string(),
        "**/__pycache__/**".to_string(),
        "**/bin/**".to_string(),
        "**/obj/**".to_string(),
        "**/target/**".to_string(),
        "**/vendor/**".to_string(),
        "**/*.min.js".to_string(),
        "**/*.min.css".to_string(),
        "**/*.generated.*".to_string(),
        "**/*.g.cs".to_string(),
        "**/*.designer.cs".to_string(),
        // Fold metadata directory - excluded to prevent infinite loops
        // when metadata is synced back to the repository
        "fold/**".to_string(),
        ".fold/**".to_string(),
    ]
}

impl Default for ProjectCreate {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            index_patterns: default_index_patterns(),
            ignore_patterns: default_ignore_patterns(),
            team_members: Vec::new(),
            metadata: HashMap::new(),
            meta_storage_type: MetaStorageType::default(),
            meta_path: None,
            meta_source_id: None,
            meta_source_config: None,
        }
    }
}

/// Statistics for a project
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectStats {
    pub total_memories: i64,
    pub memories_by_type: HashMap<String, i64>,
    pub indexed_files: i64,
    pub last_indexed: Option<DateTime<Utc>>,
    pub active_team_members: i64,
    pub last_activity: Option<DateTime<Utc>>,
    pub total_commits: i64,
    pub total_links: i64,
}

/// A project containing related memories
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct Project {
    pub id: String,
    /// URL-safe identifier
    pub slug: String,
    pub name: String,
    pub description: Option<String>,

    /// Root path for project files (derived from repository local_path, not stored)
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,

    /// JSON array of glob patterns
    pub index_patterns: Option<String>,
    /// JSON array of glob patterns
    pub ignore_patterns: Option<String>,

    // Team
    /// JSON array of usernames
    pub team_members: Option<String>,
    pub owner: Option<String>,

    // Custom metadata as JSON
    pub metadata: Option<String>,

    // Meta storage configuration (uses existing schema fields)
    /// Whether metadata repo sync is enabled
    pub metadata_repo_enabled: bool,
    /// 'separate' (external repo) or 'in_repo' (path in main repo)
    pub metadata_repo_mode: Option<String>,
    /// Provider type: 'github', 'gitlab', 'google-drive', etc.
    pub metadata_repo_provider: Option<String>,
    /// For separate mode: repo owner
    pub metadata_repo_owner: Option<String>,
    /// For separate mode: repo name
    pub metadata_repo_name: Option<String>,
    /// For separate mode: branch
    pub metadata_repo_branch: Option<String>,
    /// Encrypted access token
    pub metadata_repo_token: Option<String>,
    /// For in_repo mode: source ID reference
    pub metadata_repo_source_id: Option<String>,
    /// Path prefix within repo (default: '.fold/')
    pub metadata_repo_path_prefix: Option<String>,

    // Webhook loop prevention
    /// JSON array of author patterns to ignore during webhook processing
    pub ignored_commit_authors: Option<String>,

    // Decay algorithm configuration
    /// Weight for retrieval strength vs semantic similarity (0.0-1.0), default 0.3
    pub decay_strength_weight: Option<f64>,
    /// Half-life in days for memory decay, default 30.0
    pub decay_half_life_days: Option<f64>,

    // Git integration
    /// Whether to auto-commit fold/ changes after indexing (default: true)
    pub auto_commit_enabled: Option<bool>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    /// Create a new project with generated ID and slug
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            slug: slugify(&name),
            name,
            description: None,
            root_path: None, // Derived from repository local_path
            index_patterns: Some(serde_json::to_string(&default_index_patterns()).unwrap()),
            ignore_patterns: Some(serde_json::to_string(&default_ignore_patterns()).unwrap()),
            team_members: None,
            owner: None,
            metadata: None,
            metadata_repo_enabled: false,
            metadata_repo_mode: Some("in_repo".to_string()),
            metadata_repo_provider: None,
            metadata_repo_owner: None,
            metadata_repo_name: None,
            metadata_repo_branch: None,
            metadata_repo_token: None,
            metadata_repo_source_id: None,
            metadata_repo_path_prefix: Some(".fold/".to_string()),
            ignored_commit_authors: None,
            decay_strength_weight: None, // Use default 0.3
            decay_half_life_days: None,  // Use default 30.0
            auto_commit_enabled: None,   // Use default true
            created_at: now,
            updated_at: now,
        }
    }

    /// Create from ProjectCreate request
    pub fn from_create(data: ProjectCreate, owner: Option<String>) -> Self {
        let now = Utc::now();
        // Only internal storage is supported
        let mode = "in_repo";
        Self {
            id: super::new_id(),
            slug: slugify(&data.name),
            name: data.name,
            description: data.description,
            root_path: None, // Derived from repository local_path
            index_patterns: Some(serde_json::to_string(&data.index_patterns).unwrap()),
            ignore_patterns: Some(serde_json::to_string(&data.ignore_patterns).unwrap()),
            team_members: if data.team_members.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&data.team_members).unwrap())
            },
            owner,
            metadata: if data.metadata.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&data.metadata).unwrap())
            },
            // Metadata storage is always enabled (internal mode)
            metadata_repo_enabled: true,
            metadata_repo_mode: Some(mode.to_string()),
            metadata_repo_provider: data.meta_source_config.as_ref().and_then(|c| {
                c.get("provider")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            }),
            metadata_repo_owner: data.meta_source_config.as_ref().and_then(|c| {
                c.get("owner")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            }),
            metadata_repo_name: data.meta_source_config.as_ref().and_then(|c| {
                c.get("repo")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            }),
            metadata_repo_branch: data.meta_source_config.as_ref().and_then(|c| {
                c.get("branch")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            }),
            metadata_repo_token: None, // Set separately for security
            metadata_repo_source_id: data.meta_source_id,
            metadata_repo_path_prefix: data.meta_path.or_else(|| Some(".fold/".to_string())),
            ignored_commit_authors: None,
            decay_strength_weight: None,
            decay_half_life_days: None,
            auto_commit_enabled: None, // Default true
            created_at: now,
            updated_at: now,
        }
    }

    /// Parse index patterns from JSON string
    pub fn index_patterns_vec(&self) -> Vec<String> {
        self.index_patterns
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(default_index_patterns)
    }

    /// Parse ignore patterns from JSON string
    pub fn ignore_patterns_vec(&self) -> Vec<String> {
        self.ignore_patterns
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(default_ignore_patterns)
    }

    /// Parse team members from JSON string
    pub fn team_members_vec(&self) -> Vec<String> {
        self.team_members
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Parse metadata from JSON string
    pub fn metadata_map(&self) -> HashMap<String, serde_json::Value> {
        self.metadata
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Parse ignored commit authors from JSON string
    pub fn ignored_commit_authors_vec(&self) -> Vec<String> {
        self.ignored_commit_authors
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Get the strength weight for decay blending, with default fallback.
    pub fn get_decay_strength_weight(&self) -> f64 {
        self.decay_strength_weight.unwrap_or(0.3)
    }

    /// Get the half-life in days for decay, with default fallback.
    pub fn get_decay_half_life_days(&self) -> f64 {
        self.decay_half_life_days.unwrap_or(30.0)
    }

    /// Get the collection name for this project in Qdrant
    pub fn collection_name(&self, prefix: &str) -> String {
        format!("{}{}", prefix, self.slug)
    }

    /// Get the typed meta storage type.
    /// Currently only Internal is supported.
    pub fn get_meta_storage_type(&self) -> MetaStorageType {
        // Only internal storage is supported for now
        MetaStorageType::Internal
    }

    /// Get meta source config as JSON (reconstructed from fields)
    pub fn meta_source_config_value(&self) -> Option<serde_json::Value> {
        if self.metadata_repo_provider.is_none() {
            return None;
        }
        let mut config = serde_json::Map::new();
        if let Some(ref provider) = self.metadata_repo_provider {
            config.insert("provider".to_string(), serde_json::json!(provider));
        }
        if let Some(ref owner) = self.metadata_repo_owner {
            config.insert("owner".to_string(), serde_json::json!(owner));
        }
        if let Some(ref repo) = self.metadata_repo_name {
            config.insert("repo".to_string(), serde_json::json!(repo));
        }
        if let Some(ref branch) = self.metadata_repo_branch {
            config.insert("branch".to_string(), serde_json::json!(branch));
        }
        Some(serde_json::Value::Object(config))
    }

    /// Check if using internal meta storage (in_repo mode)
    pub fn uses_internal_meta(&self) -> bool {
        self.metadata_repo_mode.as_deref() != Some("separate")
    }

    /// Get the meta storage base path
    pub fn meta_base_path(&self) -> String {
        self.metadata_repo_path_prefix
            .clone()
            .unwrap_or_else(|| ".fold/".to_string())
    }

    /// Check if metadata repo sync is enabled
    pub fn is_meta_enabled(&self) -> bool {
        self.metadata_repo_enabled
    }

    /// Get the metadata repo provider type
    pub fn meta_provider(&self) -> Option<&str> {
        self.metadata_repo_provider.as_deref()
    }

    /// Check if auto-commit is enabled for fold/ changes.
    /// Default is true if not explicitly set.
    pub fn auto_commit_enabled(&self) -> bool {
        self.auto_commit_enabled.unwrap_or(true)
    }
}

/// Convert text to a URL-safe slug
pub fn slugify(text: &str) -> String {
    text.to_lowercase()
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("  My Project  "), "my-project");
        assert_eq!(slugify("Test--Project"), "test-project");
        assert_eq!(slugify("Special!@#$%Characters"), "specialcharacters");
        assert_eq!(slugify("Already-Slugified"), "already-slugified");
    }

    #[test]
    fn test_project_collection_name() {
        let project = Project::new("My Test Project".to_string());
        assert_eq!(project.collection_name("fold_"), "fold_my-test-project");
    }
}
