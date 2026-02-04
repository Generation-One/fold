//! Provider Integration Tests
//!
//! Real integration tests for LLM and embedding providers.
//!
//! These tests use actual API keys from environment variables.
//! Set the following environment variables to run tests:
//! - GEMINI_API_KEY: Google Gemini API key
//! - OPENAI_API_KEY: OpenAI API key
//! - ANTHROPIC_API_KEY: Anthropic Claude API key
//! - OPENROUTER_API_KEY: OpenRouter API key
//!
//! Run specific provider tests:
//!   cargo test provider_integration::gemini -- --ignored
//!   cargo test provider_integration::openai -- --ignored
//!   cargo test provider_integration::anthropic -- --ignored
//!   cargo test provider_integration::openrouter -- --ignored
//!
//! Run all provider tests:
//!   cargo test provider_integration -- --ignored

use reqwest::Client;
use serde_json::{json, Value};
use std::env;
use std::time::Duration;

// ============================================================================
// Test Helpers
// ============================================================================

/// Get API key from environment, panics with helpful message if not set
fn require_api_key(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| {
        panic!(
            "Environment variable {} not set. Please set it to run this test.",
            name
        )
    })
}

/// Check if API key is available without panicking
fn has_api_key(name: &str) -> bool {
    env::var(name).is_ok()
}

/// Create HTTP client with timeout
fn create_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("Failed to create HTTP client")
}

// ============================================================================
// Gemini Tests
// ============================================================================

mod gemini {
    use super::*;

    /// Test Gemini embeddings API directly
    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY environment variable"]
    async fn test_gemini_embeddings() {
        let api_key = require_api_key("GEMINI_API_KEY");
        let client = create_client();

        let response = client
            .post("https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent")
            .query(&[("key", &api_key)])
            .json(&json!({
                "model": "models/text-embedding-004",
                "content": {
                    "parts": [{
                        "text": "Hello, this is a test embedding"
                    }]
                }
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "Gemini embedding failed: {} - {:?}",
            status,
            body
        );

        // Verify we got an embedding back
        let embedding = &body["embedding"]["values"];
        assert!(
            embedding.is_array(),
            "Expected embedding array, got: {:?}",
            body
        );

        let values = embedding.as_array().unwrap();
        assert!(!values.is_empty(), "Embedding should not be empty");
        println!("✓ Gemini embedding successful, dimension: {}", values.len());
    }

    /// Test Gemini LLM completion API directly
    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY environment variable"]
    async fn test_gemini_completion() {
        let api_key = require_api_key("GEMINI_API_KEY");
        let client = create_client();

        let response = client
            .post("https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent")
            .query(&[("key", &api_key)])
            .json(&json!({
                "contents": [{
                    "parts": [{
                        "text": "Say 'Hello, World!' and nothing else."
                    }]
                }],
                "generationConfig": {
                    "maxOutputTokens": 50
                }
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "Gemini completion failed: {} - {:?}",
            status,
            body
        );

        // Verify we got text back
        let text = &body["candidates"][0]["content"]["parts"][0]["text"];
        assert!(text.is_string(), "Expected text response, got: {:?}", body);
        println!("✓ Gemini completion successful: {}", text);
    }
}

// ============================================================================
// OpenAI Tests
// ============================================================================

mod openai {
    use super::*;

    /// Test OpenAI embeddings API directly
    #[tokio::test]
    #[ignore = "requires OPENAI_API_KEY environment variable"]
    async fn test_openai_embeddings() {
        let api_key = require_api_key("OPENAI_API_KEY");
        let client = create_client();

        let response = client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&api_key)
            .json(&json!({
                "model": "text-embedding-3-small",
                "input": "Hello, this is a test embedding"
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "OpenAI embedding failed: {} - {:?}",
            status,
            body
        );

        // Verify we got an embedding back
        let embedding = &body["data"][0]["embedding"];
        assert!(
            embedding.is_array(),
            "Expected embedding array, got: {:?}",
            body
        );

        let values = embedding.as_array().unwrap();
        assert!(!values.is_empty(), "Embedding should not be empty");
        println!("✓ OpenAI embedding successful, dimension: {}", values.len());
    }

    /// Test OpenAI LLM completion API directly
    #[tokio::test]
    #[ignore = "requires OPENAI_API_KEY environment variable"]
    async fn test_openai_completion() {
        let api_key = require_api_key("OPENAI_API_KEY");
        let client = create_client();

        let response = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&api_key)
            .json(&json!({
                "model": "gpt-4o-mini",
                "messages": [
                    {"role": "user", "content": "Say 'Hello, World!' and nothing else."}
                ],
                "max_tokens": 50
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "OpenAI completion failed: {} - {:?}",
            status,
            body
        );

        // Verify we got text back
        let text = &body["choices"][0]["message"]["content"];
        assert!(text.is_string(), "Expected text response, got: {:?}", body);
        println!("✓ OpenAI completion successful: {}", text);
    }
}

// ============================================================================
// Anthropic Tests
// ============================================================================

mod anthropic {
    use super::*;

    /// Test Anthropic Claude completion API directly
    #[tokio::test]
    #[ignore = "requires ANTHROPIC_API_KEY environment variable"]
    async fn test_anthropic_completion() {
        let api_key = require_api_key("ANTHROPIC_API_KEY");
        let client = create_client();

        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": "claude-3-5-haiku-20241022",
                "max_tokens": 50,
                "messages": [
                    {"role": "user", "content": "Say 'Hello, World!' and nothing else."}
                ]
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "Anthropic completion failed: {} - {:?}",
            status,
            body
        );

        // Verify we got text back
        let text = &body["content"][0]["text"];
        assert!(text.is_string(), "Expected text response, got: {:?}", body);
        println!("✓ Anthropic completion successful: {}", text);
    }

    /// Test Anthropic with OAuth token (if available)
    #[tokio::test]
    #[ignore = "requires ANTHROPIC_OAUTH_TOKEN environment variable"]
    async fn test_anthropic_oauth_completion() {
        let oauth_token = require_api_key("ANTHROPIC_OAUTH_TOKEN");
        let client = create_client();

        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .bearer_auth(&oauth_token)
            .header("anthropic-version", "2023-06-01")
            // Beta headers for OAuth
            .header(
                "anthropic-beta",
                "oauth-2025-04-20,interleaved-thinking-2025-05-14",
            )
            .json(&json!({
                "model": "claude-sonnet-4-20250514",
                "max_tokens": 50,
                "messages": [
                    {"role": "user", "content": "Say 'Hello, World!' and nothing else."}
                ]
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "Anthropic OAuth completion failed: {} - {:?}",
            status,
            body
        );

        let text = &body["content"][0]["text"];
        assert!(text.is_string(), "Expected text response, got: {:?}", body);
        println!("✓ Anthropic OAuth completion successful: {}", text);
    }
}

// ============================================================================
// OpenRouter Tests
// ============================================================================

mod openrouter {
    use super::*;

    /// Test OpenRouter completion API directly
    #[tokio::test]
    #[ignore = "requires OPENROUTER_API_KEY environment variable"]
    async fn test_openrouter_completion() {
        let api_key = require_api_key("OPENROUTER_API_KEY");
        let client = create_client();

        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(&api_key)
            .header("HTTP-Referer", "https://fold.dev")
            .header("X-Title", "Fold Integration Tests")
            .json(&json!({
                "model": "google/gemini-flash-1.5",
                "messages": [
                    {"role": "user", "content": "Say 'Hello, World!' and nothing else."}
                ],
                "max_tokens": 50
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let body: Value = response.json().await.expect("Failed to parse response");

        assert!(
            status.is_success(),
            "OpenRouter completion failed: {} - {:?}",
            status,
            body
        );

        // Verify we got text back
        let text = &body["choices"][0]["message"]["content"];
        assert!(text.is_string(), "Expected text response, got: {:?}", body);
        println!("✓ OpenRouter completion successful: {}", text);
    }
}

// ============================================================================
// Provider Status Check (Run This First)
// ============================================================================

#[test]
fn check_available_providers() {
    println!("\n=== Provider Availability Check ===\n");

    let providers = [
        ("GEMINI_API_KEY", "Gemini"),
        ("OPENAI_API_KEY", "OpenAI"),
        ("ANTHROPIC_API_KEY", "Anthropic"),
        ("OPENROUTER_API_KEY", "OpenRouter"),
    ];

    let mut available = 0;
    for (env_var, name) in providers {
        if has_api_key(env_var) {
            println!("✓ {} available ({})", name, env_var);
            available += 1;
        } else {
            println!("✗ {} not configured (set {})", name, env_var);
        }
    }

    println!("\n{}/{} providers configured", available, providers.len());
    println!("\nTo run integration tests:");
    println!("  cargo test provider_integration::gemini -- --ignored");
    println!("  cargo test provider_integration::openai -- --ignored");
    println!("  cargo test provider_integration::anthropic -- --ignored");
    println!("  cargo test provider_integration::openrouter -- --ignored");
    println!("\nOr run all: cargo test provider_integration -- --ignored\n");
}

// ============================================================================
// Database + Service Integration Tests
// ============================================================================

mod service_integration {
    use super::*;
    use fold_core::db::{
        create_embedding_provider, create_llm_provider, init_pool, migrate,
        CreateEmbeddingProvider, CreateLlmProvider,
    };
    use fold_core::services::{EmbeddingService, LlmService};
    use serde_json::json;
    use std::sync::Arc;

    /// Test full LLM service with Gemini from database
    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY environment variable"]
    async fn test_llm_service_with_gemini() {
        let api_key = require_api_key("GEMINI_API_KEY");

        // Create in-memory database
        let db = init_pool(":memory:")
            .await
            .expect("Failed to create database");
        migrate(&db).await.expect("Failed to run migrations");

        // Create Gemini provider in database
        create_llm_provider(
            &db,
            CreateLlmProvider {
                name: "gemini".to_string(),
                enabled: true,
                priority: 1,
                auth_type: "api_key".to_string(),
                api_key: Some(api_key),
                config: json!({
                    "model": "gemini-1.5-flash",
                    "endpoint": "https://generativelanguage.googleapis.com/v1beta"
                }),
            },
        )
        .await
        .expect("Failed to create provider");

        // Create LLM service from database
        let llm_config = fold::config::config().llm.clone();
        let llm_service = LlmService::new(db.clone(), &llm_config)
            .await
            .expect("Failed to create LLM service");

        // Test service is available
        assert!(
            llm_service.is_available().await,
            "LLM service should be available"
        );

        // Test actual completion through service
        let response = llm_service
            .complete("Say 'test successful'", 100)
            .await
            .expect("Failed to complete");

        println!("✓ LLM service Gemini test passed: {}", response);
    }

    /// Test full embedding service with Gemini from database
    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY environment variable"]
    async fn test_embedding_service_with_gemini() {
        let api_key = require_api_key("GEMINI_API_KEY");

        // Create in-memory database
        let db = init_pool(":memory:")
            .await
            .expect("Failed to create database");
        migrate(&db).await.expect("Failed to run migrations");

        // Create Gemini embedding provider in database
        create_embedding_provider(
            &db,
            CreateEmbeddingProvider {
                name: "gemini".to_string(),
                enabled: true,
                priority: 1,
                auth_type: "api_key".to_string(),
                api_key: Some(api_key),
                config: json!({
                    "model": "text-embedding-004",
                    "endpoint": "https://generativelanguage.googleapis.com/v1beta",
                    "dimension": 768
                }),
            },
        )
        .await
        .expect("Failed to create provider");

        // Create embedding service from database
        let embedding_config = fold::config::config().embedding.clone();
        let embedding_service = EmbeddingService::new(db.clone(), &embedding_config)
            .await
            .expect("Failed to create embedding service");

        // Test service has providers
        assert!(
            embedding_service.has_providers().await,
            "Embedding service should have providers"
        );

        // Test actual embedding through service
        let embedding = embedding_service
            .embed_single("This is a test sentence")
            .await
            .expect("Failed to embed");

        assert!(!embedding.is_empty(), "Embedding should not be empty");
        println!(
            "✓ Embedding service Gemini test passed, dimension: {}",
            embedding.len()
        );
    }

    /// Test provider fallback when first provider fails
    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY and OPENAI_API_KEY environment variables"]
    async fn test_provider_fallback() {
        let gemini_key = require_api_key("GEMINI_API_KEY");
        let openai_key = require_api_key("OPENAI_API_KEY");

        // Create in-memory database
        let db = init_pool(":memory:")
            .await
            .expect("Failed to create database");
        migrate(&db).await.expect("Failed to run migrations");

        // Create OpenAI provider with HIGHER priority (lower number = higher priority)
        create_llm_provider(
            &db,
            CreateLlmProvider {
                name: "openai".to_string(),
                enabled: true,
                priority: 1, // Higher priority
                auth_type: "api_key".to_string(),
                api_key: Some(openai_key),
                config: json!({
                    "model": "gpt-4o-mini",
                    "endpoint": "https://api.openai.com/v1"
                }),
            },
        )
        .await
        .expect("Failed to create OpenAI provider");

        // Create Gemini provider as fallback
        create_llm_provider(
            &db,
            CreateLlmProvider {
                name: "gemini".to_string(),
                enabled: true,
                priority: 2, // Lower priority = fallback
                auth_type: "api_key".to_string(),
                api_key: Some(gemini_key),
                config: json!({
                    "model": "gemini-1.5-flash",
                    "endpoint": "https://generativelanguage.googleapis.com/v1beta"
                }),
            },
        )
        .await
        .expect("Failed to create Gemini provider");

        // Create LLM service
        let llm_config = fold::config::config().llm.clone();
        let llm_service = LlmService::new(db.clone(), &llm_config)
            .await
            .expect("Failed to create LLM service");

        // Verify both providers are available
        let providers = llm_service.providers().await;
        assert!(
            providers.len() >= 2,
            "Should have at least 2 providers, got: {:?}",
            providers
        );

        // Test completion (should use highest priority first)
        let response = llm_service
            .complete("Say 'test'", 50)
            .await
            .expect("Failed to complete");

        println!("✓ Provider fallback test passed: {}", response);
    }
}
