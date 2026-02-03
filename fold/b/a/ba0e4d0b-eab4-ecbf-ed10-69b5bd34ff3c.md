---
id: ba0e4d0b-eab4-ecbf-ed10-69b5bd34ff3c
title: AI Session and Workspace Database Queries
author: system
tags:
- database
- model
- service
- async
- domain-model
file_path: src/db/sessions.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:35:37.670454700Z
updated_at: 2026-02-03T08:35:37.670454700Z
---

This file defines database models and operations for managing AI agent working sessions and workspace mappings in a Rust application. It provides type definitions for AI sessions, session notes, and workspace configurations, along with serialization/deserialization support for database persistence. The module implements a domain-driven design approach with separate input/output types and enum-based status tracking, serving as the data access layer for AI session management functionality.