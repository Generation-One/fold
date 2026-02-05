//! LLM service with multi-provider fallback.
//!
//! Supports Gemini, Anthropic (Claude), OpenRouter, and OpenAI with automatic fallback
//! when rate limits are hit or providers fail.
//!
//! Providers are loaded from the database with fallback to environment variables
//! for initial seeding.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::ClaudeCodeService;
use crate::config::LlmConfig;
use crate::db::{
    list_enabled_llm_providers, seed_claudecode_provider_async, seed_llm_providers_from_env,
    update_llm_provider_last_used, DbPool, LlmProviderRow,
};
use crate::error::{Error, Result};
use crate::models::{CodeSummary, CommitInfo, Memory, SuggestedLink};

/// Maximum retries per provider before fallback
const MAX_RETRIES: u32 = 2;

/// Delay between retries (doubles each time)
const RETRY_DELAY_MS: u64 = 500;

/// Minimum interval between health checks (to avoid costs)
const HEALTH_CHECK_INTERVAL_SECS: u64 = 60;

/// Number of consecutive errors before marking unavailable
const ERROR_THRESHOLD: u32 = 3;

/// Runtime provider configuration (loaded from database)
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

impl From<LlmProviderRow> for RuntimeLlmProvider {
    fn from(row: LlmProviderRow) -> Self {
        let config = row.config_json().unwrap_or(json!({}));

        Self {
            id: row.id,
            name: row.name.clone(),
            base_url: config
                .get("endpoint")
                .and_then(|e| e.as_str())
                .map(String::from)
                .unwrap_or_else(|| default_endpoint(&row.name)),
            model: config
                .get("model")
                .and_then(|m| m.as_str())
                .map(String::from)
                .unwrap_or_else(|| default_model(&row.name)),
            api_key: row.api_key,
            oauth_access_token: row.oauth_access_token,
            priority: row.priority,
        }
    }
}

/// Get default endpoint for a provider
fn default_endpoint(name: &str) -> String {
    match name {
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        "anthropic" | "claudecode" => "https://api.anthropic.com/v1".to_string(),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        "openai" => "https://api.openai.com/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

/// Get default model for a provider
fn default_model(name: &str) -> String {
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
/// on rate limits or failures. Providers are loaded from the database.
#[derive(Clone)]
pub struct LlmService {
    inner: Arc<LlmServiceInner>,
}

struct LlmServiceInner {
    db: Option<DbPool>,
    providers: RwLock<Vec<RuntimeLlmProvider>>,
    client: Client,
    /// Last error message from LLM call
    last_error: RwLock<Option<String>>,
    /// When the last error occurred
    last_error_at: RwLock<Option<Instant>>,
    /// Consecutive error count
    error_count: AtomicU32,
    /// When we last checked health
    last_health_check: RwLock<Option<Instant>>,
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
    /// Create a new LLM service with database-backed providers.
    ///
    /// On first run, seeds providers from environment variables.
    pub async fn new(db: DbPool, config: &LlmConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        // Try to load providers from database
        let db_providers = list_enabled_llm_providers(&db).await?;

        let providers: Vec<RuntimeLlmProvider> = if db_providers.is_empty() {
            // Seed from environment variables on first run
            if !config.providers.is_empty() {
                info!("Seeding LLM providers from environment variables");
                seed_llm_providers_from_env(&db, &config.providers).await?;
            }

            // Also check for Claude Code credentials
            let claudecode_service = ClaudeCodeService::new();
            if claudecode_service.is_available() {
                match claudecode_service.read_credentials() {
                    Ok(creds) => {
                        if let Some(token) = creds.access_token() {
                            info!(
                                subscription = ?creds.subscription_type(),
                                "Seeding Claude Code provider from local credentials"
                            );
                            if let Err(e) = seed_claudecode_provider_async(
                                &db,
                                token,
                                creds.subscription_type(),
                            )
                            .await
                            {
                                warn!(error = %e, "Failed to seed Claude Code provider");
                            }
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, "Could not read Claude Code credentials");
                    }
                }
            }

            let seeded = list_enabled_llm_providers(&db).await?;
            seeded.into_iter().map(RuntimeLlmProvider::from).collect()
        } else {
            // Check if claudecode provider exists but needs token refresh
            let has_claudecode = db_providers.iter().any(|p| p.name == "claudecode");
            if has_claudecode {
                let claudecode_service = ClaudeCodeService::new();
                if let Ok(creds) = claudecode_service.read_credentials() {
                    if let Some(token) = creds.access_token() {
                        // Refresh the token in case it was updated
                        let _ =
                            seed_claudecode_provider_async(&db, token, creds.subscription_type())
                                .await;
                    }
                }
            }

            db_providers
                .into_iter()
                .map(RuntimeLlmProvider::from)
                .collect()
        };

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "LLM service initialized from database"
        );

        Ok(Self {
            inner: Arc::new(LlmServiceInner {
                db: Some(db),
                providers: RwLock::new(providers),
                client,
                last_error: RwLock::new(None),
                last_error_at: RwLock::new(None),
                error_count: AtomicU32::new(0),
                last_health_check: RwLock::new(None),
            }),
        })
    }

    /// Create LLM service from config only (for backwards compatibility)
    pub fn from_config(config: &LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        let providers: Vec<RuntimeLlmProvider> = config
            .providers
            .iter()
            .map(|p| RuntimeLlmProvider {
                id: String::new(),
                name: p.name.clone(),
                base_url: p.base_url.clone(),
                model: p.model.clone(),
                api_key: Some(p.api_key.clone()),
                oauth_access_token: None,
                priority: p.priority as i32,
            })
            .collect();

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "LLM service initialized from config"
        );

        Self {
            inner: Arc::new(LlmServiceInner {
                db: None, // Not used in config-only mode
                providers: RwLock::new(providers),
                client,
                last_error: RwLock::new(None),
                last_error_at: RwLock::new(None),
                error_count: AtomicU32::new(0),
                last_health_check: RwLock::new(None),
            }),
        }
    }

    /// Reload providers from the database.
    pub async fn refresh_providers(&self) -> Result<()> {
        let Some(ref db) = self.inner.db else {
            // No database configured - config-only mode, nothing to refresh
            return Ok(());
        };

        let db_providers = list_enabled_llm_providers(db).await?;
        let providers: Vec<RuntimeLlmProvider> = db_providers
            .into_iter()
            .map(RuntimeLlmProvider::from)
            .collect();

        info!(
            providers = ?providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "Refreshed LLM providers from database"
        );

        let mut guard = self.inner.providers.write().await;
        *guard = providers;

        Ok(())
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
            return Err(Error::Llm("No LLM providers configured".to_string()));
        }

        let mut last_error = None;

        for provider in &providers {
            if !provider.has_credentials() {
                debug!(provider = %provider.name, "Skipping provider without credentials");
                continue;
            }

            match self.try_provider(provider, prompt, max_tokens).await {
                Ok(response) => {
                    // Update last used timestamp
                    if !provider.id.is_empty() {
                        if let Some(ref db) = self.inner.db {
                            let _ = update_llm_provider_last_used(db, &provider.id).await;
                        }
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

        Err(last_error.unwrap_or_else(|| Error::Llm("All providers failed".to_string())))
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
            .map_err(|e| Error::Llm(format!("Request failed: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Error::Llm(format!("Failed to read response: {}", e)))?;

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
    fn extract_json(&self, text: &str) -> Option<Value> {
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

    /// Summarize a file and extract detailed metadata.
    /// Works with any file type: source code, documentation, configuration, data, etc.
    pub async fn summarize_code(
        &self,
        content: &str,
        path: &str,
        language: &str,
    ) -> Result<CodeSummary> {
        let prompt = format!(
            r#"Analyse this file. Be brief but precise - pack maximum information into minimum words. No fluff.

File: {path}
Type: {language}
Lines: {lines}

```
{content}
```

Return JSON:
- "title": What this file does (max 80 chars, be specific)
- "summary": 1-2 dense sentences. Include specific details that matter (names, values, patterns). Omit obvious/generic statements. Adapt to file type - code: what it does and how; docs: key points; config: what it controls; data: structure and contents.
- "keywords": Important identifiers from the file (max 12). Function/class names, key terms, settings, fields.
- "tags": Category tags (max 5). File type + role + domain.
- "exports": What this defines (functions, sections, settings, columns)
- "dependencies": What this requires (imports, external refs)
- "architecture_notes": One line on structure/patterns if notable, else empty
- "key_functions": Main elements (max 5)
- "created_date": Earliest date found (YYYY-MM-DD) or null

ONLY valid JSON, no markdown."#,
            path = path,
            language = if language.is_empty() { "unknown" } else { language },
            lines = content.lines().count(),
            content = &content[..content.len().min(4000)]
        );

        let response = self.complete(&prompt, 800).await?;

        let json = self.extract_json(&response).unwrap_or_else(|| json!({}));

        Ok(CodeSummary {
            title: json["title"].as_str().unwrap_or("").to_string(),
            summary: json["summary"].as_str().unwrap_or("").to_string(),
            keywords: json["keywords"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            tags: json["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            language: if language.is_empty() {
                None
            } else {
                Some(language.to_string())
            },
            exports: json["exports"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            dependencies: json["dependencies"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            created_date: json["created_date"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from),
        })
    }

    /// Summarize a git commit with detailed analysis.
    pub async fn summarize_commit(&self, commit: &CommitInfo) -> Result<String> {
        let files_summary = commit
            .files
            .iter()
            .take(30)
            .map(|f| format!("  - {} ({})", f.path, f.status))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Provide a comprehensive technical summary of this git commit for a development team.

Commit: {sha}
Author: {author}
Message: {message}

Statistics: +{insertions} lines, -{deletions} lines
Files changed: {file_count}

Detailed file changes:
{files}

Generate a detailed summary with the following sections:

## Overview
Brief description of what this commit accomplishes (2-3 sentences).

## Changes Made
Detailed breakdown of specific changes:
- List key modifications by file or functional area
- Include technical details about what was added, removed, or modified
- Note any important algorithmic or architectural changes

## Impact Analysis
- Which parts of the system are affected?
- Are there any breaking changes or API modifications?
- What functionality is newly enabled or deprecated?

## Context
- Why were these changes made? (based on commit message and file patterns)
- Any notable patterns in the changes?
- Relationship to broader features or refactoring efforts

Provide detailed, technical analysis suitable for code review and project history. Use plain text without markdown formatting."#,
            sha = &commit.sha[..7.min(commit.sha.len())],
            author = commit.author.as_deref().unwrap_or("unknown"),
            message = commit.message,
            insertions = commit.insertions,
            deletions = commit.deletions,
            file_count = commit.files.len(),
            files = files_summary
        );

        self.complete(&prompt, 800).await
    }

    /// Summarize a development session from notes.
    pub async fn summarize_session(&self, notes: &str) -> Result<String> {
        let prompt = format!(
            r#"Summarize this development session based on the notes recorded during the session.

## Session Notes
{notes}

## Instructions
Create a concise summary that captures:
1. What was accomplished during the session
2. Key decisions made
3. Problems encountered and solutions found
4. Next steps or remaining work

Write in past tense, 2-3 paragraphs. Be specific about technical details."#,
            notes = notes
        );

        self.complete(&prompt, 400).await
    }

    /// Summarize a PR file diff for impact analysis.
    ///
    /// Used for hybrid diff indexing - only called on top impactful files.
    pub async fn summarize_pr_diff(
        &self,
        pr_title: &str,
        file_path: &str,
        status: &str,
        additions: i32,
        deletions: i32,
        patch: Option<&str>,
    ) -> Result<String> {
        let patch_preview = patch
            .map(|p| p.chars().take(2000).collect::<String>())
            .unwrap_or_else(|| "(patch not available)".to_string());

        let prompt = format!(
            r#"Analyze this file change from a pull request.

PR: {pr_title}
File: {file_path}
Status: {status}
Changes: +{additions} -{deletions} lines

Diff:
```
{patch}
```

Provide a concise technical analysis (3-5 sentences):
1. What was changed in this file?
2. Why might this change have been made? (infer from context)
3. What areas of the codebase might be affected?

Focus on actionable insights for developers reviewing or searching this change later."#,
            pr_title = pr_title,
            file_path = file_path,
            status = status,
            additions = additions,
            deletions = deletions,
            patch = patch_preview
        );

        self.complete(&prompt, 300).await
    }

    /// Suggest links between a memory and candidate memories.
    pub async fn suggest_links(
        &self,
        memory: &Memory,
        candidates: &[Memory],
    ) -> Result<Vec<SuggestedLink>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let candidates_text = candidates
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, m)| {
                format!(
                    "{}. [{}] {}: {}",
                    i,
                    m.id,
                    m.title.as_deref().unwrap_or("Untitled"),
                    m.content
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(200)
                        .collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            r#"Analyze relationships between a source memory and candidate memories.

Source Memory:
ID: {source_id}
Type: {source_type}
Title: {source_title}
Content: {source_content}

Candidate Memories:
{candidates}

For each related candidate, suggest a link with:
- "target_id": The candidate's ID
- "link_type": One of "references", "implements", "extends", "relates_to", "depends_on", "deprecates"
- "confidence": 0.0-1.0 how confident the relationship exists
- "reason": Brief explanation of the relationship

Respond with a JSON array of suggested links. Only include links with confidence > 0.6.
Example: [{{"target_id": "abc123", "link_type": "references", "confidence": 0.8, "reason": "Both discuss user authentication"}}]"#,
            source_id = memory.id,
            source_type = memory.memory_type,
            source_title = memory.title.as_deref().unwrap_or("Untitled"),
            source_content = memory
                .content
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(500)
                .collect::<String>(),
            candidates = candidates_text
        );

        let response = self.complete(&prompt, 800).await?;

        let json = self.extract_json(&response);

        let links = json
            .and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            Some(SuggestedLink {
                                source_id: memory.id.clone(),
                                target_id: item["target_id"].as_str()?.to_string(),
                                link_type: item["link_type"]
                                    .as_str()
                                    .unwrap_or("relates_to")
                                    .to_string(),
                                confidence: item["confidence"].as_f64().unwrap_or(0.5) as f32,
                                reason: item["reason"].as_str().unwrap_or("").to_string(),
                            })
                        })
                        .filter(|link| link.confidence > 0.6)
                        .collect()
                })
            })
            .unwrap_or_default();

        Ok(links)
    }

    /// Generate metadata for a memory.
    pub async fn generate_metadata(
        &self,
        content: &str,
        memory_type: &str,
    ) -> Result<GeneratedMetadata> {
        let prompt = format!(
            r#"Analyze this content and generate metadata for a knowledge base.

Content type: {memory_type}
Content: {content}

Generate a JSON object with:
- "title": A short descriptive title (max 100 chars)
- "keywords": Array of 3-7 key terms/concepts
- "tags": Array of 2-4 broad category tags
- "context": A one-sentence description of the broader context

Respond with ONLY a valid JSON object, no markdown or explanation."#,
            memory_type = memory_type,
            content = &content[..content.len().min(2000)]
        );

        let response = self.complete(&prompt, 500).await?;

        let json = self.extract_json(&response).unwrap_or_else(|| json!({}));

        Ok(GeneratedMetadata {
            title: json["title"].as_str().unwrap_or("").to_string(),
            keywords: json["keywords"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            tags: json["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            context: json["context"].as_str().unwrap_or("").to_string(),
        })
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
    use crate::config::LlmConfig;

    #[test]
    fn test_extract_json() {
        let service = LlmService::from_config(&LlmConfig { providers: vec![] });

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
