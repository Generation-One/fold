---
id: 2bfd040d-d5d5-fb45-bfcc-1f521ec0fedf
title: Search and Context Retrieval API Routes
author: system
tags:
- api
- http
- search
- async
file_path: src/api/search.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:32:50.731563500Z
updated_at: 2026-02-03T08:32:50.731563500Z
---

This file implements unified search and context retrieval endpoints for a project-based memory system. It provides two main HTTP POST routes: semantic search across memories with filtering capabilities, and context retrieval for tasks. The module defines request/response types with comprehensive filtering options (by source, tags, author, date range, file patterns) and handles semantic similarity scoring, making it a critical component for querying and retrieving contextual information from the application's memory storage.