//! Memory links model for knowledge graph relationships.
//!
//! This module defines the link types and structures used to connect
//! memories in the knowledge graph. Links are simplified to 4 core types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ============================================================================
// Link Type
// ============================================================================

/// Type of relationship between memories.
///
/// Simplified to 4 core types from the previous 11 types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// Semantically related (auto-generated from vector similarity)
    Related,
    /// Explicit reference in content
    References,
    /// Dependency relationship
    DependsOn,
    /// For generated memories that summarise file changes
    Modifies,
}

impl LinkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkType::Related => "related",
            LinkType::References => "references",
            LinkType::DependsOn => "depends_on",
            LinkType::Modifies => "modifies",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "related" => Some(LinkType::Related),
            "references" => Some(LinkType::References),
            "depends_on" => Some(LinkType::DependsOn),
            "modifies" => Some(LinkType::Modifies),
            _ => None,
        }
    }

    /// Get all link types
    pub fn all() -> &'static [LinkType] {
        &[
            LinkType::Related,
            LinkType::References,
            LinkType::DependsOn,
            LinkType::Modifies,
        ]
    }

    /// Check if this is an auto-generated link type
    pub fn is_auto_generated(&self) -> bool {
        matches!(self, LinkType::Related)
    }
}

impl Default for LinkType {
    fn default() -> Self {
        LinkType::Related
    }
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Memory Link
// ============================================================================

/// A link between two memories in the knowledge graph.
///
/// Links are directional: source -> target. The link_type describes
/// the relationship from source's perspective.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct MemoryLink {
    pub id: String,
    pub project_id: String,
    pub source_id: String,
    pub target_id: String,
    /// 'related', 'references', 'depends_on', 'modifies'
    pub link_type: String,
    /// Why this link exists (optional context)
    pub context: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl MemoryLink {
    /// Create a new link with generated ID
    pub fn new(
        project_id: String,
        source_id: String,
        target_id: String,
        link_type: LinkType,
        context: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            project_id,
            source_id,
            target_id,
            link_type: link_type.as_str().to_string(),
            context,
            created_at: Utc::now(),
        }
    }

    /// Create a "related" link (for auto-generated similarity links)
    pub fn related(project_id: String, source_id: String, target_id: String) -> Self {
        Self::new(project_id, source_id, target_id, LinkType::Related, None)
    }

    /// Create a "references" link
    pub fn references(
        project_id: String,
        source_id: String,
        target_id: String,
        context: Option<String>,
    ) -> Self {
        Self::new(project_id, source_id, target_id, LinkType::References, context)
    }

    /// Create a "depends_on" link
    pub fn depends_on(
        project_id: String,
        source_id: String,
        target_id: String,
        context: Option<String>,
    ) -> Self {
        Self::new(project_id, source_id, target_id, LinkType::DependsOn, context)
    }

    /// Create a "modifies" link (e.g., commit summary -> modified file)
    pub fn modifies(
        project_id: String,
        source_id: String,
        target_id: String,
        context: Option<String>,
    ) -> Self {
        Self::new(project_id, source_id, target_id, LinkType::Modifies, context)
    }

    /// Get the typed link type
    pub fn get_link_type(&self) -> Option<LinkType> {
        LinkType::from_str(&self.link_type)
    }

    /// Check if this is an auto-generated link
    pub fn is_auto_generated(&self) -> bool {
        self.get_link_type()
            .map(|t| t.is_auto_generated())
            .unwrap_or(false)
    }
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request model for creating a link
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateLinkRequest {
    pub source_id: String,
    pub target_id: String,
    #[serde(default)]
    pub link_type: String,
    pub context: Option<String>,
}

impl Default for CreateLinkRequest {
    fn default() -> Self {
        Self {
            source_id: String::new(),
            target_id: String::new(),
            link_type: LinkType::Related.as_str().to_string(),
            context: None,
        }
    }
}

/// Response model for a link with memory details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LinkResponse {
    pub link: MemoryLink,
    /// Title of source memory (for display)
    pub source_title: Option<String>,
    /// Title of target memory (for display)
    pub target_title: Option<String>,
}

/// Response model for links from/to a memory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryLinksResponse {
    /// Links where this memory is the source
    pub outgoing: Vec<LinkResponse>,
    /// Links where this memory is the target
    pub incoming: Vec<LinkResponse>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_type_conversion() {
        assert_eq!(LinkType::Related.as_str(), "related");
        assert_eq!(LinkType::References.as_str(), "references");
        assert_eq!(LinkType::DependsOn.as_str(), "depends_on");
        assert_eq!(LinkType::Modifies.as_str(), "modifies");

        assert_eq!(LinkType::from_str("related"), Some(LinkType::Related));
        assert_eq!(LinkType::from_str("REFERENCES"), Some(LinkType::References));
        assert_eq!(LinkType::from_str("depends_on"), Some(LinkType::DependsOn));
        assert_eq!(LinkType::from_str("modifies"), Some(LinkType::Modifies));
        assert_eq!(LinkType::from_str("invalid"), None);
    }

    #[test]
    fn test_link_type_all() {
        let all = LinkType::all();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn test_memory_link_new() {
        let link = MemoryLink::new(
            "project-1".to_string(),
            "source-1".to_string(),
            "target-1".to_string(),
            LinkType::References,
            Some("See also".to_string()),
        );

        assert!(!link.id.is_empty());
        assert_eq!(link.project_id, "project-1");
        assert_eq!(link.source_id, "source-1");
        assert_eq!(link.target_id, "target-1");
        assert_eq!(link.link_type, "references");
        assert_eq!(link.context, Some("See also".to_string()));
    }

    #[test]
    fn test_memory_link_helpers() {
        let related = MemoryLink::related(
            "p1".to_string(),
            "s1".to_string(),
            "t1".to_string(),
        );
        assert_eq!(related.link_type, "related");
        assert!(related.is_auto_generated());

        let references = MemoryLink::references(
            "p1".to_string(),
            "s1".to_string(),
            "t1".to_string(),
            None,
        );
        assert_eq!(references.link_type, "references");
        assert!(!references.is_auto_generated());

        let depends = MemoryLink::depends_on(
            "p1".to_string(),
            "s1".to_string(),
            "t1".to_string(),
            Some("Import".to_string()),
        );
        assert_eq!(depends.link_type, "depends_on");

        let modifies = MemoryLink::modifies(
            "p1".to_string(),
            "s1".to_string(),
            "t1".to_string(),
            Some("Updated function".to_string()),
        );
        assert_eq!(modifies.link_type, "modifies");
    }

    #[test]
    fn test_is_auto_generated() {
        assert!(LinkType::Related.is_auto_generated());
        assert!(!LinkType::References.is_auto_generated());
        assert!(!LinkType::DependsOn.is_auto_generated());
        assert!(!LinkType::Modifies.is_auto_generated());
    }
}
