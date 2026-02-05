//! Provider registry for dynamic provider lookup.
//!
//! The registry allows runtime lookup of file source providers by type,
//! enabling dynamic configuration of which providers are available.

use std::collections::HashMap;
use std::sync::Arc;

use super::{FileSourceProvider, GitHubFileSource, GoogleDriveFileSource, LocalFileSource};

/// Registry of available file source providers.
///
/// Allows dynamic lookup of providers by their type identifier.
pub struct ProviderRegistry {
    providers: HashMap<&'static str, Arc<dyn FileSourceProvider>>,
}

impl ProviderRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Create a registry with default providers.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register GitHub provider
        registry.register(Arc::new(GitHubFileSource::new()));

        // Register Google Drive provider
        registry.register(Arc::new(GoogleDriveFileSource::new()));

        // Register Local filesystem provider
        registry.register(Arc::new(LocalFileSource::new()));

        // Future: register GitLab, OneDrive, etc.

        registry
    }

    /// Register a provider.
    pub fn register(&mut self, provider: Arc<dyn FileSourceProvider>) {
        self.providers.insert(provider.provider_type(), provider);
    }

    /// Get a provider by type.
    pub fn get(&self, provider_type: &str) -> Option<Arc<dyn FileSourceProvider>> {
        self.providers.get(provider_type).cloned()
    }

    /// Check if a provider type is available.
    pub fn has(&self, provider_type: &str) -> bool {
        self.providers.contains_key(provider_type)
    }

    /// List all registered provider types.
    pub fn provider_types(&self) -> Vec<&'static str> {
        self.providers.keys().copied().collect()
    }

    /// List all registered providers with their display names.
    pub fn providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|p| ProviderInfo {
                provider_type: p.provider_type(),
                display_name: p.display_name(),
                supports_webhooks: p.supports_webhooks(),
                requires_polling: p.requires_polling(),
            })
            .collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Information about a registered provider.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// Provider type identifier.
    pub provider_type: &'static str,
    /// Human-readable name.
    pub display_name: &'static str,
    /// Whether webhooks are supported.
    pub supports_webhooks: bool,
    /// Whether polling is required.
    pub requires_polling: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_defaults() {
        let registry = ProviderRegistry::with_defaults();

        assert!(registry.has("github"));
        assert!(registry.has("google-drive"));

        let github = registry.get("github").unwrap();
        assert_eq!(github.provider_type(), "github");
        assert_eq!(github.display_name(), "GitHub");
        assert!(github.supports_webhooks());

        let gdrive = registry.get("google-drive").unwrap();
        assert_eq!(gdrive.provider_type(), "google-drive");
        assert_eq!(gdrive.display_name(), "Google Drive");
        assert!(!gdrive.supports_webhooks());
        assert!(gdrive.requires_polling());
    }

    #[test]
    fn test_provider_list() {
        let registry = ProviderRegistry::with_defaults();

        let types = registry.provider_types();
        assert!(types.contains(&"github"));

        let providers = registry.providers();
        assert!(!providers.is_empty());

        let github_info = providers.iter().find(|p| p.provider_type == "github").unwrap();
        assert_eq!(github_info.display_name, "GitHub");
        assert!(github_info.supports_webhooks);
        assert!(!github_info.requires_polling);
    }
}
