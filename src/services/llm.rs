//! LLM service with multi-provider fallback.
//!
//! Supports Gemini, OpenRouter, and OpenAI with automatic fallback
//! when rate limits are hit or providers fail.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::config::{LlmConfig, LlmProvider};
use crate::error::{Error, Result};
use crate::models::{CodeSummary, CommitInfo, Memory, SuggestedLink};

/// Maximum retries per provider before fallback
const MAX_RETRIES: u32 = 2;

/// Delay between retries (doubles each time)
const RETRY_DELAY_MS: u64 = 500;

/// Service for LLM operations with multi-provider fallback.
///
/// Tries providers in priority order (Gemini -> OpenRouter -> OpenAI),
/// automatically falling back on rate limits or failures.
#[derive(Clone)]
pub struct LlmService {
    inner: Arc<LlmServiceInner>,
}

struct LlmServiceInner {
    providers: Vec<LlmProvider>,
    client: Client,
}

/// Response from LLM API
#[derive(Debug, Deserialize)]
struct LlmResponse {
    choices: Option<Vec<Choice>>,
    candidates: Option<Vec<Candidate>>, // Gemini format
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
struct LlmError {
    message: String,
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<String>,
}

impl LlmService {
    /// Create a new LLM service with configured providers.
    pub fn new(config: &LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        info!(
            providers = ?config.providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "LLM service initialized"
        );

        Self {
            inner: Arc::new(LlmServiceInner {
                providers: config.providers.clone(),
                client,
            }),
        }
    }

    /// Check if any providers are configured
    pub fn is_available(&self) -> bool {
        !self.inner.providers.is_empty()
    }

    /// Get provider names in priority order
    pub fn providers(&self) -> Vec<&str> {
        self.inner.providers.iter().map(|p| p.name.as_str()).collect()
    }

    /// Complete a prompt with automatic provider fallback.
    pub async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        if self.inner.providers.is_empty() {
            return Err(Error::Llm("No LLM providers configured".to_string()));
        }

        let mut last_error = None;

        for provider in &self.inner.providers {
            match self.try_provider(provider, prompt, max_tokens).await {
                Ok(response) => return Ok(response),
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

        Err(last_error.unwrap_or_else(|| Error::Llm("All providers failed".to_string())))
    }

    /// Try a specific provider with retries.
    async fn try_provider(
        &self,
        provider: &LlmProvider,
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
        provider: &LlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<String> {
        debug!(
            provider = %provider.name,
            model = %provider.model,
            "Calling LLM provider"
        );

        let (url, body) = match provider.name.as_str() {
            "gemini" => self.build_gemini_request(provider, prompt, max_tokens),
            _ => self.build_openai_request(provider, prompt, max_tokens),
        };

        let response = self
            .inner
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", provider.api_key))
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
        provider: &LlmProvider,
        prompt: &str,
        max_tokens: u32,
    ) -> (String, Value) {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            provider.base_url, provider.model, provider.api_key
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
        provider: &LlmProvider,
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

    /// Parse response from different API formats
    fn parse_response(&self, provider: &str, text: &str) -> Result<String> {
        let response: LlmResponse = serde_json::from_str(text)
            .map_err(|e| Error::Llm(format!("Failed to parse response: {}", e)))?;

        if let Some(error) = response.error {
            return Err(Error::Llm(error.message));
        }

        // Try Gemini format first
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

        Err(Error::Llm(format!(
            "No content in {} response",
            provider
        )))
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
            let start = text[start..].find('\n').map(|i| start + i + 1).unwrap_or(start);
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

    /// Summarize source code and extract detailed metadata.
    pub async fn summarize_code(
        &self,
        content: &str,
        path: &str,
        language: &str,
    ) -> Result<CodeSummary> {
        let prompt = format!(
            r#"Perform a comprehensive analysis of this source code file.

File: {path}
Language: {language}
Lines of code: {lines}

```
{content}
```

Generate a detailed JSON object with:
- "title": Clear, concise description of this file's primary purpose (max 100 chars)
- "summary": Comprehensive 2-4 sentence description covering:
  * What this file does
  * Its role in the broader system
  * Key responsibilities and functionality
  * Notable patterns or architectural approach used
- "keywords": Array of important function names, class names, constants, and key variable names (max 15)
- "tags": Array of descriptive category tags like:
  * Functional role: "api", "service", "model", "controller", "utility", "middleware", "test"
  * Tech patterns: "async", "database", "http", "validation", "auth", "caching"
  * Domain: "user-management", "payments", "analytics", etc.
  (max 6 tags)
- "exports": Complete array of all exported/public functions, classes, types, or constants with their names
- "dependencies": Array of imported modules/packages with package names
- "architecture_notes": Brief note about architectural patterns used (e.g., "Uses repository pattern", "Implements Observer pattern", "RESTful API endpoint")
- "key_functions": Array of the 3-5 most important function/method names in this file

Respond with ONLY a valid JSON object, no markdown or explanation."#,
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
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            tags: json["tags"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            language: if language.is_empty() {
                None
            } else {
                Some(language.to_string())
            },
            exports: json["exports"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            dependencies: json["dependencies"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
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
                    m.content.chars().take(200).collect::<String>()
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

Respond with a JSON array of suggested links. Only include links with confidence > 0.5.
Example: [{{"target_id": "abc123", "link_type": "references", "confidence": 0.8, "reason": "Both discuss user authentication"}}]"#,
            source_id = memory.id,
            source_type = memory.memory_type,
            source_title = memory.title.as_deref().unwrap_or("Untitled"),
            source_content = memory.content.chars().take(500).collect::<String>(),
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
                        .filter(|link| link.confidence > 0.5)
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
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            tags: json["tags"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
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
}
