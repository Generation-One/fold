---
name: using-fold
description: Use this skill when querying Fold for project context, storing memories, searching codebases, or retrieving AI-ready context. Triggers on keywords like memory search, context retrieval, project knowledge, codebase search, semantic search, or any MCP tool interaction with Fold.
---

# Using Fold

Query and store project knowledge through Fold's MCP tools and REST API.

**Latest Documentation**: Always check [github.com/Generation-One/fold/wiki](https://github.com/Generation-One/fold/wiki) for current API and tool references.

# API Note

**Fold has NO `/api` prefix** — routes are directly on root (`/projects`, `/health`, `/search`, etc.).

# Core Operations

## 1. Searching Memories

Find relevant project knowledge using semantic search:

```bash
# Via REST API
curl -X POST http://localhost:8765/projects/{project_id}/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "authentication flow", "limit": 10}'
```

## 2. Getting Context

Retrieve AI-ready context for a specific task:

```bash
# Via REST API
curl -X POST http://localhost:8765/projects/{project_id}/context \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"task": "how does the payment service work", "max_tokens": 4000}'
```

This returns semantically relevant memories, code snippets and decisions formatted for AI consumption.

## 3. Triggering Reindex

Update the index after changes:

```bash
curl -X POST http://localhost:8765/projects/{project_id}/reindex \
  -H "Authorization: Bearer $TOKEN"
```

## 4. Listing Projects

```bash
curl http://localhost:8765/projects \
  -H "Authorization: Bearer $TOKEN"
```

# REST API Reference

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/health` | GET | Health check (no auth) |
| `/projects` | GET | List all projects |
| `/projects` | POST | Create project |
| `/projects/{id}` | GET | Get project details |
| `/projects/{id}` | PUT | Update project |
| `/projects/{id}` | DELETE | Delete project |
| `/projects/{id}/search` | POST | Semantic search |
| `/projects/{id}/context` | POST | AI-ready context |
| `/projects/{id}/reindex` | POST | Trigger reindex |
| `/jobs/{id}` | GET | Check job status |

# Project Types

## GitHub Provider
For repositories hosted on GitHub:
```json
{
  "slug": "my-project",
  "name": "My Project",
  "provider": "github",
  "remote_owner": "org-name",
  "remote_repo": "repo-name"
}
```

## Local Provider
For local filesystem directories:
```json
{
  "slug": "my-project",
  "name": "My Project", 
  "provider": "local",
  "root_path": "/path/to/project"
}
```

**Note**: Local paths must be mounted into the Fold container.

# Best Practices

**Search effectively**: Use natural language queries describing what you need, not keywords. Fold uses semantic search.

**Check project scope**: Always verify the correct project ID before searching.

**Reindex after changes**: If you've updated files in a local project, trigger a reindex to update the vectors.

# Example Workflow

**User asks**: 'How do we handle authentication?'

1. Search for relevant context:
   ```bash
   curl -X POST http://localhost:8765/projects/{id}/context \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"task": "explain the authentication implementation"}'
   ```

2. Review returned memories (code, decisions, sessions)

3. Synthesise response from the context

# Troubleshooting

**No results returned**: Check the project ID is correct and the project has been indexed.

**Irrelevant results**: Refine your query to be more specific about the domain or feature.

**Authentication errors**: Verify your API token is valid (format: `fold_{prefix}_{secret}`).

**Reindex fails**: Check container logs — common issues are missing local path mounts or UTF-8 truncation panics.

See the managing-fold skill for deployment and configuration issues.
