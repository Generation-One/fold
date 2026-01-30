//! Project model for organizing memories.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

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

/// Request model for creating a project
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectCreate {
    pub name: String,
    pub description: Option<String>,

    // Codebase settings
    pub repo_url: Option<String>,
    pub root_path: Option<String>,

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
    ]
}

impl Default for ProjectCreate {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            repo_url: None,
            root_path: None,
            index_patterns: default_index_patterns(),
            ignore_patterns: default_ignore_patterns(),
            team_members: Vec::new(),
            metadata: HashMap::new(),
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

    // Codebase settings
    pub repo_url: Option<String>,
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
            repo_url: None,
            root_path: None,
            index_patterns: Some(serde_json::to_string(&default_index_patterns()).unwrap()),
            ignore_patterns: Some(serde_json::to_string(&default_ignore_patterns()).unwrap()),
            team_members: None,
            owner: None,
            metadata: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create from ProjectCreate request
    pub fn from_create(data: ProjectCreate, owner: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: super::new_id(),
            slug: slugify(&data.name),
            name: data.name,
            description: data.description,
            repo_url: data.repo_url,
            root_path: data.root_path,
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

    /// Get the collection name for this project in Qdrant
    pub fn collection_name(&self, prefix: &str) -> String {
        format!("{}{}", prefix, self.slug)
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
