//! LLM service with multi-provider fallback.
//!
//! Supports Gemini, Anthropic (Claude), OpenRouter, and OpenAI with automatic fallback
//! when rate limits are hit or providers fail.
//!
//! This crate provides a standalone LLM service that can be used independently
//! of the rest of the Fold system.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Maximum retries per provider before fallback
const MAX_RETRIES: u32 = 2;

/// Delay between retries (doubles each time)
const RETRY_DELAY_MS: u64 = 500;

/// Minimum interval between health checks (to avoid costs)
const HEALTH_CHECK_INTERVAL_SECS: u64 = 60;

/// Number of consecutive errors before marking unavailable
const ERROR_THRESHOLD: u32 = 3;

/// Error types for the LLM service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("No providers configured")]
    NoProviders,

    #[error("Request failed: {0}")]
    Request(String),
}

/// Result type for LLM operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Configuration for an LLM provider.
#[derive(Debug, Clone)]
pub struct LlmProviderConfig {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub priority: u8,
}

/// Configuration for the LLM service.
#[derive(Debug, Clone, Default)]
pub struct LlmConfig {
    pub providers: Vec<LlmProviderConfig>,
}

/// Runtime provider configuration.
#[derive(Debug, Clone)]
pub struct RuntimeLlmProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub oauth_access_token: Option<String>,
    pub priority: i32,
}

impl RuntimeLlmProvider {
    /// Get the authentication token (prefers OAuth, falls back to API key)
    pub fn auth_token(&self) -> Option<&str> {
        self.oauth_access_token
            .as_deref()
            .or(self.api_key.as_deref())
    }

    /// Check if provider has valid credentials
    pub fn has_credentials(&self) -> bool {
        self.api_key.is_some() || self.oauth_access_token.is_some()
    }
}

impl From<&LlmProviderConfig> for RuntimeLlmProvider {
    fn from(config: &LlmProviderConfig) -> Self {
        Self {
            id: String::new(),
            name: config.name.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: Some(config.api_key.clone()),
            oauth_access_token: None,
            priority: config.priority as i32,
        }
    }
}

/// Get default endpoint for a provider
pub fn default_endpoint(name: &str) -> String {
    match name {
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        "anthropic" | "claudecode" => "https://api.anthropic.com/v1".to_string(),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        "openai" => "https://api.openai.com/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

/// Get default model for a provider
pub fn default_model(name: &str) -> String {
    match name {
        "gemini" => "gemini-1.5-flash".to_string(),
        "anthropic" | "claudecode" => "claude-3-5-haiku-20241022".to_string(),
        "openrouter" => "meta-llama/llama-3-8b-instruct:free".to_string(),
        "openai" => "gpt-4o-mini".to_string(),
        _ => "gpt-4o-mini".to_string(),
    }
}

/// Service for LLM operations with multi-provider fallback.
///
/// Tries providers in priority order, automatically falling back
/// on rate limits or failures.
#[derive(Clone)]
pub struct LlmService {
    inner: Arc<LlmServiceInner>,
}

struct LlmServiceInner {
    providers: RwLock<Vec<RuntimeLlmProvider>>,
    client: Client,
    /// Last error message from LLM call
    last_error: RwLock<Option<String>>,
    /// When the last error occurred
    #[allow(dead_code)]
    last_error_at: RwLock<Option<Instant>>,
    /// Consecutive error count
    error_count: AtomicU32,
    /// When we last checked health
    last_health_check: RwLock<Option<Instant>>,
    /// Callback for when a provider is used (for tracking)
    on_provider_used: RwLock<Option<Box<dyn Fn(&str) + Send + Sync>>>,
}

/// Response from LLM API
#[derive(Debug, Deserialize)]
struct LlmResponse {
    choices: Option<Vec<Choice>>,
    candidates: Option<Vec<Candidate>>,     // Gemini format
    content: Option<Vec<AnthropicContent>>, // Anthropic format
    error: Option<LlmError>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Option<Message>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: String,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: CandidateContent,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
struct Part {
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    content_type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct LlmError {
    message: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: Option<String>,
    #[allow(dead_code)]
    code: Option<String>,
}

impl LlmService {
    /// Create LLM service from config.
    pub fn new(config: &LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        let providers: Vec<RuntimeLlmProvider> =
            config.providers.iter().map(RuntimeLlmProvider::from).collect();

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "LLM service initialized from config"
        );

        Self {
            inner: Arc::new(LlmServiceInner {
                providers: RwLock::new(providers),
                client,
                last_error: RwLock::new(None),
                last_error_at: RwLock::new(None),
                error_count: AtomicU32::new(0),
                last_health_check: RwLock::new(None),
                on_provider_used: RwLock::new(None),
            }),
        }
    }

    /// Create LLM service with pre-configured runtime providers.
    pub fn with_providers(providers: Vec<RuntimeLlmProvider>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "LLM service initialized with providers"
        );

        Self {
            inner: Arc::new(LlmServiceInner {
                providers: RwLock::new(providers),
                client,
                last_error: RwLock::new(None),
                last_error_at: RwLock::new(None),
                error_count: AtomicU32::new(0),
                last_health_check: RwLock::new(None),
                on_provider_used: RwLock::new(None),
            }),
        }
    }

    /// Set a callback to be called when a provider is successfully used.
    pub async fn set_on_provider_used<F>(&self, callback: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let mut guard = self.inner.on_provider_used.write().await;
        *guard = Some(Box::new(callback));
    }

    /// Update providers at runtime.
    pub async fn set_providers(&self, providers: Vec<RuntimeLlmProvider>) {
        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "Updating LLM providers"
        );

        let mut guard = self.inner.providers.write().await;
        *guard = providers;
    }

    /// Check if LLM service is available.
    /// Returns false if no providers configured OR if in error state.
    /// Rate-limits health checks to avoid costs (max once per 60s).
    pub async fn is_available(&self) -> bool {
        let guard = self.inner.providers.read().await;
        if guard.is_empty() {
            return false;
        }
        drop(guard);

        // Check if we're in error state
        let error_count = self.inner.error_count.load(Ordering::Relaxed);
        if error_count >= ERROR_THRESHOLD {
            // Check if enough time has passed for a health check
            let last_check = self.inner.last_health_check.read().await;
            if let Some(last) = *last_check {
                if last.elapsed().as_secs() < HEALTH_CHECK_INTERVAL_SECS {
                    // Too soon to re-check, still unavailable
                    return false;
                }
            }
            // Don't reset here - let the next successful call reset it
        }

        true
    }

    /// Get error info for status endpoint
    pub async fn get_error_info(&self) -> Option<(String, u32)> {
        let error = self.inner.last_error.read().await;
        if let Some(ref msg) = *error {
            let count = self.inner.error_count.load(Ordering::Relaxed);
            Some((msg.clone(), count))
        } else {
            None
        }
    }

    /// Record an error from LLM call
    async fn record_error(&self, error: &str) {
        let mut last_error = self.inner.last_error.write().await;
        *last_error = Some(error.to_string());
        drop(last_error);

        let mut last_error_at = self.inner.last_error_at.write().await;
        *last_error_at = Some(Instant::now());
        drop(last_error_at);

        self.inner.error_count.fetch_add(1, Ordering::Relaxed);

        let mut last_check = self.inner.last_health_check.write().await;
        *last_check = Some(Instant::now());
    }

    /// Clear error state after successful call
    async fn clear_error(&self) {
        let mut last_error = self.inner.last_error.write().await;
        *last_error = None;
        drop(last_error);

        self.inner.error_count.store(0, Ordering::Relaxed);
    }

    /// Get provider names in priority order
    pub async fn providers(&self) -> Vec<String> {
        let guard = self.inner.providers.read().await;
        guard.iter().map(|p| p.name.clone()).collect()
    }

    /// Complete a prompt with automatic provider fallback.
    pub async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        let providers = {
            let guard = self.inner.providers.read().await;
            guard.clone()
        };

        if providers.is_empty() {
            return Err(Error::NoProviders);
        }

        let mut last_error = None;

        for provider in &providers {
            if !provider.has_credentials() {
                debug!(provider = %provider.name, "Skipping provider without credentials");
                continue;
            }

            match self.try_provider(provider, prompt, max_tokens).await {
                Ok(response) => {
                    // Notify callback if set
                    let callback = self.inner.on_provider_used.read().await;
                    if let Some(ref cb) = *callback {
                        cb(&provider.id);
                    }
                    // Clear error state on success
                    self.clear_error().await;
                    return Ok(response);
                }
                Err(e) => {
                    warn!(
                        provider = %provider.name,
                        error = %e,
                        "Provider failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        // Record error when all providers fail
        let error_msg = last_error
            .as_ref()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "All providers failed".to_string());
        self.record_error(&error_msg).await;

        Err(last_error.unwrap_or(Error::Llm("All providers failed".to_string())))
    }

    /// Try a specific provider with retries.
    async fn try_provider(
        &self,
        provider: &RuntimeLlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<String> {
        let mut delay = Duration::from_millis(RETRY_DELAY_MS);

        for attempt in 0..MAX_RETRIES {
            match self.call_provider(provider, prompt, max_tokens).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if Self::is_retryable(&e) && attempt < MAX_RETRIES - 1 {
                        debug!(
                            provider = %provider.name,
                            attempt,
                            delay_ms = delay.as_millis(),
                            "Retrying after error"
                        );
                        sleep(delay).await;
                        delay *= 2;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(Error::Llm(format!(
            "Provider {} failed after {} retries",
            provider.name, MAX_RETRIES
        )))
    }

    /// Check if an error is retryable
    fn is_retryable(error: &Error) -> bool {
        matches!(error, Error::RateLimitExceeded)
            || error.to_string().contains("rate limit")
            || error.to_string().contains("429")
            || error.to_string().contains("503")
            || error.to_string().contains("timeout")
    }

    /// Make the actual API call to a provider.
    async fn call_provider(
        &self,
        provider: &RuntimeLlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<String> {
        let auth_token = provider
            .auth_token()
            .ok_or_else(|| Error::Llm(format!("No credentials for provider {}", provider.name)))?;

        debug!(
            provider = %provider.name,
            model = %provider.model,
            "Calling LLM provider"
        );

        let (url, body) = match provider.name.as_str() {
            "gemini" => self.build_gemini_request(provider, prompt, max_tokens),
            "anthropic" | "claudecode" => {
                self.build_anthropic_request(provider, prompt, max_tokens)
            }
            _ => self.build_openai_request(provider, prompt, max_tokens),
        };

        let mut request = self
            .inner
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        // Add authentication header based on provider
        request = match provider.name.as_str() {
            "anthropic" => {
                // Check if using OAuth token (requires beta header)
                if provider.oauth_access_token.is_some() {
                    request
                        .header("Authorization", format!("Bearer {}", auth_token))
                        .header("anthropic-version", "2023-06-01")
                        .header(
                            "anthropic-beta",
                            "oauth-2025-04-20, claude-code-20250219, interleaved-thinking-2025-05-14",
                        )
                } else {
                    request
                        .header("x-api-key", auth_token)
                        .header("anthropic-version", "2023-06-01")
                }
            }
            "claudecode" => {
                // Claude Code always uses OAuth tokens from ~/.claude
                request
                    .header("Authorization", format!("Bearer {}", auth_token))
                    .header("anthropic-version", "2023-06-01")
                    .header(
                        "anthropic-beta",
                        "oauth-2025-04-20, claude-code-20250219, interleaved-thinking-2025-05-14",
                    )
            }
            _ => request.header("Authorization", format!("Bearer {}", auth_token)),
        };

        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Request(format!("Request failed: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Error::Request(format!("Failed to read response: {}", e)))?;

        if status.as_u16() == 429 {
            return Err(Error::RateLimitExceeded);
        }

        if !status.is_success() {
            return Err(Error::Llm(format!(
                "Provider returned {}: {}",
                status, text
            )));
        }

        self.parse_response(&provider.name, &text)
    }

    /// Build request for Gemini API
    fn build_gemini_request(
        &self,
        provider: &RuntimeLlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> (String, Value) {
        let auth_token = provider.auth_token().unwrap_or("");
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            provider.base_url, provider.model, auth_token
        );

        let body = json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }],
            "generationConfig": {
                "maxOutputTokens": max_tokens,
                "temperature": 0.3
            }
        });

        (url, body)
    }

    /// Build request for OpenAI-compatible APIs (OpenAI, OpenRouter)
    fn build_openai_request(
        &self,
        provider: &RuntimeLlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> (String, Value) {
        let url = format!("{}/chat/completions", provider.base_url);

        let mut body = json!({
            "model": provider.model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "max_tokens": max_tokens,
            "temperature": 0.3
        });

        // Add OpenRouter-specific headers
        if provider.name == "openrouter" {
            body["http_referer"] = json!("https://fold.dev");
            body["x_title"] = json!("Fold Memory System");
        }

        (url, body)
    }

    /// Build request for Anthropic Claude API
    fn build_anthropic_request(
        &self,
        provider: &RuntimeLlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> (String, Value) {
        let url = format!("{}/messages", provider.base_url);

        let body = json!({
            "model": provider.model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "max_tokens": max_tokens,
            "temperature": 0.3
        });

        (url, body)
    }

    /// Parse response from different API formats
    fn parse_response(&self, provider: &str, text: &str) -> Result<String> {
        let response: LlmResponse = serde_json::from_str(text)
            .map_err(|e| Error::Llm(format!("Failed to parse response: {}", e)))?;

        if let Some(error) = response.error {
            return Err(Error::Llm(error.message));
        }

        // Try Anthropic format first
        if let Some(content) = response.content {
            if let Some(content_block) = content.first() {
                return Ok(content_block.text.clone());
            }
        }

        // Try Gemini format
        if let Some(candidates) = response.candidates {
            if let Some(candidate) = candidates.first() {
                if let Some(part) = candidate.content.parts.first() {
                    return Ok(part.text.clone());
                }
            }
        }

        // Try OpenAI format
        if let Some(choices) = response.choices {
            if let Some(choice) = choices.first() {
                if let Some(message) = &choice.message {
                    return Ok(message.content.clone());
                }
                if let Some(text) = &choice.text {
                    return Ok(text.clone());
                }
            }
        }

        Err(Error::Llm(format!("No content in {} response", provider)))
    }

    /// Extract JSON from LLM response text
    pub fn extract_json(&self, text: &str) -> Option<Value> {
        // Try to find JSON in code blocks
        if let Some(start) = text.find("```json") {
            let start = start + 7;
            if let Some(end) = text[start..].find("```") {
                if let Ok(json) = serde_json::from_str(&text[start..start + end]) {
                    return Some(json);
                }
            }
        }

        // Try to find JSON in generic code blocks
        if let Some(start) = text.find("```") {
            let start = start + 3;
            // Skip language identifier if present
            let start = text[start..]
                .find('\n')
                .map(|i| start + i + 1)
                .unwrap_or(start);
            if let Some(end) = text[start..].find("```") {
                if let Ok(json) = serde_json::from_str(&text[start..start + end]) {
                    return Some(json);
                }
            }
        }

        // Try to find raw JSON object
        if let Some(start) = text.find('{') {
            // Find matching closing brace
            let mut depth = 0;
            let mut end = start;
            for (i, c) in text[start..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > start {
                if let Ok(json) = serde_json::from_str(&text[start..end]) {
                    return Some(json);
                }
            }
        }

        None
    }
}

/// Metadata generated by LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedMetadata {
    pub title: String,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub context: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json() {
        let service = LlmService::new(&LlmConfig { providers: vec![] });

        // Test JSON in code block
        let text = r#"Here's the result:
```json
{"title": "Test", "value": 42}
```"#;
        let json = service.extract_json(text);
        assert!(json.is_some());
        assert_eq!(json.unwrap()["title"], "Test");

        // Test raw JSON
        let text = r#"The result is {"title": "Raw", "count": 5} and more text"#;
        let json = service.extract_json(text);
        assert!(json.is_some());
        assert_eq!(json.unwrap()["title"], "Raw");
    }

    #[test]
    fn test_default_endpoints() {
        assert_eq!(
            default_endpoint("gemini"),
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(
            default_endpoint("anthropic"),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(default_endpoint("openai"), "https://api.openai.com/v1");
    }

    #[test]
    fn test_default_models() {
        assert_eq!(default_model("gemini"), "gemini-1.5-flash");
        assert_eq!(default_model("anthropic"), "claude-3-5-haiku-20241022");
        assert_eq!(default_model("openai"), "gpt-4o-mini");
    }
}
