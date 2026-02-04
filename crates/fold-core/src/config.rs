//! Configuration management for Fold.
//!
//! Loads configuration from environment variables with support for:
//! - Multiple OIDC auth providers via AUTH_PROVIDER_{NAME}_{FIELD} pattern
//! - Multiple LLM providers with fallback priority
//! - Database and vector store connections

use std::collections::HashMap;
use std::env;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Global configuration instance
static CONFIG: OnceLock<Config> = OnceLock::new();

/// Get the global configuration
pub fn config() -> &'static Config {
    CONFIG.get_or_init(Config::from_env)
}

/// Initialize configuration (call once at startup)
pub fn init() -> &'static Config {
    config()
}

#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub qdrant: QdrantConfig,
    pub embedding: EmbeddingConfig,
    pub auth: AuthConfig,
    pub llm: LlmConfig,
    pub session: SessionConfig,
    pub storage: StorageConfig,
    pub indexing: IndexingConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub public_url: String,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct QdrantConfig {
    pub url: String,
    pub collection_prefix: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub providers: Vec<EmbeddingProvider>,
    pub dimension: usize,
}

#[derive(Debug, Clone)]
pub struct EmbeddingProvider {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub priority: u8,
    pub search_priority: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub providers: HashMap<String, AuthProvider>,
    pub bootstrap_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProvider {
    pub id: String,
    pub provider_type: AuthProviderType,
    pub display_name: String,
    pub issuer: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    pub icon: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthProviderType {
    Oidc,
    GitHub,
    GitLab,
}

impl std::str::FromStr for AuthProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "oidc" => Ok(Self::Oidc),
            "github" => Ok(Self::GitHub),
            "gitlab" => Ok(Self::GitLab),
            _ => Err(format!("Unknown auth provider type: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub providers: Vec<LlmProvider>,
}

#[derive(Debug, Clone)]
pub struct LlmProvider {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub priority: u8,
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub secret: String,
    pub max_age_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub attachments_path: String,
    pub summaries_path: String,
    pub max_attachment_size: usize,
    /// Base path for memory content storage (default: "fold")
    pub fold_path: String,
}

#[derive(Debug, Clone)]
pub struct IndexingConfig {
    /// Maximum number of files to index in parallel (default: 4)
    pub concurrency_limit: usize,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            server: ServerConfig {
                host: env_or("HOST", "0.0.0.0"),
                port: env_or("PORT", "8765").parse().expect("Invalid PORT"),
                public_url: env_or("PUBLIC_URL", "http://localhost:8765"),
                tls_cert_path: env::var("TLS_CERT_PATH").ok(),
                tls_key_path: env::var("TLS_KEY_PATH").ok(),
            },
            database: DatabaseConfig {
                path: env_or("DATABASE_PATH", "./data/fold.db"),
            },
            qdrant: QdrantConfig {
                url: env_or("QDRANT_URL", "http://localhost:6334"),
                collection_prefix: env_or("QDRANT_COLLECTION_PREFIX", "fold_"),
            },
            embedding: Self::parse_embedding_config(),
            auth: AuthConfig {
                providers: Self::parse_auth_providers(),
                bootstrap_token: env::var("ADMIN_BOOTSTRAP_TOKEN").ok(),
            },
            llm: LlmConfig {
                providers: Self::parse_llm_providers(),
            },
            session: SessionConfig {
                secret: env::var("SESSION_SECRET").unwrap_or_else(|_| nanoid::nanoid!(32)),
                max_age_seconds: env_or("SESSION_MAX_AGE", "604800")
                    .parse()
                    .unwrap_or(604800), // 7 days
            },
            storage: StorageConfig {
                attachments_path: env_or("ATTACHMENTS_PATH", "./data/attachments"),
                summaries_path: env_or("SUMMARIES_PATH", "./data/summaries"),
                max_attachment_size: env_or("MAX_ATTACHMENT_SIZE", "10485760")
                    .parse()
                    .unwrap_or(10 * 1024 * 1024), // 10MB
                fold_path: env_or("FOLD_PATH", "fold"),
            },
            indexing: IndexingConfig {
                concurrency_limit: env_or("INDEXING_CONCURRENCY", "4").parse().unwrap_or(4),
            },
        }
    }

    /// Parse auth providers from environment variables.
    ///
    /// Pattern: AUTH_PROVIDER_{NAME}_{FIELD}
    /// Example:
    ///   AUTH_PROVIDER_GOOGLE_TYPE=oidc
    ///   AUTH_PROVIDER_GOOGLE_DISPLAY_NAME=Google
    ///   AUTH_PROVIDER_GOOGLE_ISSUER=https://accounts.google.com
    ///   AUTH_PROVIDER_GOOGLE_CLIENT_ID=xxx
    ///   AUTH_PROVIDER_GOOGLE_CLIENT_SECRET=xxx
    fn parse_auth_providers() -> HashMap<String, AuthProvider> {
        let mut providers = HashMap::new();
        let mut provider_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // First pass: collect all provider names
        for (key, _) in env::vars() {
            if let Some(rest) = key.strip_prefix("AUTH_PROVIDER_") {
                if let Some(idx) = rest.find('_') {
                    let name = rest[..idx].to_lowercase();
                    provider_names.insert(name);
                }
            }
        }

        // Second pass: parse each provider
        for name in provider_names {
            let prefix = format!("AUTH_PROVIDER_{}_", name.to_uppercase());

            let provider_type = env::var(format!("{}TYPE", prefix))
                .ok()
                .and_then(|t| t.parse().ok());

            let Some(provider_type) = provider_type else {
                continue;
            };

            let client_id = match env::var(format!("{}CLIENT_ID", prefix)) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let client_secret = match env::var(format!("{}CLIENT_SECRET", prefix)) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let display_name =
                env::var(format!("{}DISPLAY_NAME", prefix)).unwrap_or_else(|_| name.clone());

            let issuer = env::var(format!("{}ISSUER", prefix)).ok();

            let scopes = env::var(format!("{}SCOPES", prefix))
                .unwrap_or_else(|_| "openid profile email".to_string())
                .split_whitespace()
                .map(String::from)
                .collect();

            let icon = env::var(format!("{}ICON", prefix)).ok();

            let enabled = env::var(format!("{}ENABLED", prefix))
                .map(|v| v.to_lowercase() != "false")
                .unwrap_or(true);

            providers.insert(
                name.clone(),
                AuthProvider {
                    id: name,
                    provider_type,
                    display_name,
                    issuer,
                    client_id,
                    client_secret,
                    scopes,
                    icon,
                    enabled,
                },
            );
        }

        providers
    }

    /// Parse LLM providers from environment.
    /// Supports Gemini, Anthropic, OpenRouter, and OpenAI with automatic fallback ordering.
    fn parse_llm_providers() -> Vec<LlmProvider> {
        let mut providers = Vec::new();

        // Gemini (priority 1 - free tier)
        if let Ok(api_key) = env::var("GOOGLE_API_KEY") {
            providers.push(LlmProvider {
                name: "gemini".to_string(),
                base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                model: env_or("GEMINI_MODEL", "gemini-1.5-flash"),
                api_key,
                priority: 1,
            });
        }

        // Anthropic/Claude (priority 2)
        if let Ok(api_key) = env::var("ANTHROPIC_API_KEY") {
            providers.push(LlmProvider {
                name: "anthropic".to_string(),
                base_url: "https://api.anthropic.com/v1".to_string(),
                model: env_or("ANTHROPIC_MODEL", "claude-3-5-haiku-20241022"),
                api_key,
                priority: 2,
            });
        }

        // OpenRouter (priority 3)
        if let Ok(api_key) = env::var("OPENROUTER_API_KEY") {
            providers.push(LlmProvider {
                name: "openrouter".to_string(),
                base_url: "https://openrouter.ai/api/v1".to_string(),
                model: env_or("OPENROUTER_MODEL", "meta-llama/llama-3-8b-instruct:free"),
                api_key,
                priority: 3,
            });
        }

        // OpenAI (priority 4)
        if let Ok(api_key) = env::var("OPENAI_API_KEY") {
            providers.push(LlmProvider {
                name: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                model: env_or("OPENAI_MODEL", "gpt-4o-mini"),
                api_key,
                priority: 4,
            });
        }

        // Sort by priority
        providers.sort_by_key(|p| p.priority);
        providers
    }

    /// Parse embedding providers from environment.
    /// Supports Gemini (free) and OpenAI with automatic fallback ordering.
    fn parse_embedding_config() -> EmbeddingConfig {
        let mut providers = Vec::new();

        // Gemini embeddings (priority 1 - free tier)
        if let Ok(api_key) = env::var("GOOGLE_API_KEY") {
            providers.push(EmbeddingProvider {
                name: "gemini".to_string(),
                base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                model: env_or("GEMINI_EMBEDDING_MODEL", "text-embedding-004"),
                api_key,
                priority: 1,
                search_priority: None,
            });
        }

        // OpenAI embeddings (priority 2)
        if let Ok(api_key) = env::var("OPENAI_API_KEY") {
            providers.push(EmbeddingProvider {
                name: "openai".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                model: env_or("OPENAI_EMBEDDING_MODEL", "text-embedding-3-small"),
                api_key,
                priority: 2,
                search_priority: None,
            });
        }

        // Ollama embeddings - local/self-hosted (priority 1 - if available, prefer local)
        if let Ok(ollama_url) = env::var("OLLAMA_URL") {
            let priority = env_or("OLLAMA_PRIORITY", "1")
                .parse()
                .unwrap_or(1);
            providers.push(EmbeddingProvider {
                name: "ollama".to_string(),
                base_url: ollama_url,
                model: env_or("OLLAMA_EMBEDDING_MODEL", "nomic-embed-text"),
                api_key: String::new(), // No authentication needed
                priority,
                search_priority: Some(0), // Prefer local for search
            });
        }

        // Sort by priority
        providers.sort_by_key(|p| p.priority);

        // Get dimension from env or use default based on first provider's model
        let default_dim = if providers.is_empty() {
            384 // Hash placeholder dimension
        } else {
            Self::embedding_dimension(&providers[0].model)
        };

        let dimension = env_or("EMBEDDING_DIMENSION", &default_dim.to_string())
            .parse()
            .unwrap_or(default_dim);

        EmbeddingConfig {
            providers,
            dimension,
        }
    }

    /// Get embedding dimension for known models
    /// All providers standardized on 768 dimensions for compatibility and flexibility.
    fn embedding_dimension(model: &str) -> usize {
        // Gemini models (native 768)
        if model.contains("text-embedding-004") {
            768
        } else if model.contains("embedding-001") {
            768
        }
        // OpenAI models (standardized to 768, uses dimensions parameter in API)
        else if model.contains("text-embedding-3-small") {
            768
        } else if model.contains("text-embedding-3-large") {
            768
        } else if model.contains("text-embedding-ada-002") {
            768
        }
        // Ollama models (various, but all standardized to 768)
        else if model.contains("nomic-embed-text") {
            768
        } else if model.contains("all-minilm") {
            768
        } else if model.contains("all-mpnet") {
            768
        } else if model.contains("bge-large") || model.contains("mxbai-embed-large") {
            768
        } else if model.contains("bge-base") || model.contains("bge-small") {
            768
        } else if model.contains("jina-embeddings-v2-base") {
            768
        } else if model.contains("jina-embeddings-v2-small") {
            768
        }
        // Sentence transformers (for reference, standardized)
        else if model.contains("MiniLM-L6") {
            768
        } else if model.contains("mpnet") {
            768
        } else {
            768 // Default - universal standard
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_provider_type_parsing() {
        assert_eq!(
            "oidc".parse::<AuthProviderType>().unwrap(),
            AuthProviderType::Oidc
        );
        assert_eq!(
            "github".parse::<AuthProviderType>().unwrap(),
            AuthProviderType::GitHub
        );
        assert_eq!(
            "GITLAB".parse::<AuthProviderType>().unwrap(),
            AuthProviderType::GitLab
        );
    }
}
