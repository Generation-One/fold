---
id: 9e1a2ba0-27f3-b6f8-f0ef-8cb4da3bd74f
title: Configuration Management for Fold Application
author: system
tags:
- utility
- configuration
- auth
- caching
- validation
file_path: src/config.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:33:26.281437600Z
updated_at: 2026-02-03T08:33:26.281437600Z
---

This file implements centralized configuration management for the Fold application, loading settings from environment variables with support for multiple authentication providers (OIDC, GitHub, GitLab), LLM providers with fallback priority, and various service connections (database, vector store, embeddings). It uses a singleton pattern via OnceLock to provide thread-safe global access to configuration throughout the application, with structured config types for server, database, authentication, LLM, embeddings, sessions, and storage components.