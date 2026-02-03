---
id: ddebcc8d-5b4d-0b47-bb23-c8d20cd5b55d
title: Projects API Routes - CRUD operations for project management
author: system
tags:
- api
- http
- async
- database
- validation
file_path: src/api/projects.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:32:15.016260400Z
updated_at: 2026-02-03T08:32:15.016260400Z
---

This file implements RESTful API endpoints for managing projects in the Fold system, providing complete CRUD operations (Create, Read, Update, Delete) with pagination, filtering, and sorting capabilities. It defines request/response types, query parameters, and handler functions that interact with the database layer through the AppState. The module follows Axum web framework patterns with async handlers, structured error handling, and comprehensive type safety using Rust's type system.