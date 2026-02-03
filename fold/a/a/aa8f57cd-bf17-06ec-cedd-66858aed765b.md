---
id: aa8f57cd-bf17-06ec-cedd-66858aed765b
title: Database Layer Module - SQLite Connection Pool & Schema Management
author: system
tags:
- database
- async
- utility
- service
- connection-pooling
- sqlite
file_path: src/db/mod.rs
language: rust
memory_type: codebase
created_at: 2026-02-03T08:34:28.230293800Z
updated_at: 2026-02-03T08:34:28.230293800Z
---

This file serves as the central database layer for the Fold application, providing SQLite connection pooling initialization and schema management. It aggregates query modules for all domain entities (users, projects, memories, jobs, etc.) and exposes them through a unified public interface. The module implements connection pool configuration with performance optimizations including WAL mode, memory-mapped I/O, and foreign key constraints, while also handling schema initialization from an embedded SQL file with support for idempotent execution.