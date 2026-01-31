---
name: using-fold
description: Use this skill when querying Fold for project context, storing memories, searching codebases, or retrieving AI-ready context. Triggers on keywords like memory search, context retrieval, project knowledge, codebase search, semantic search, or any MCP tool interaction with Fold.
---

# Using Fold

Query and store project knowledge through Fold's MCP tools and REST API.

**Latest Documentation**: Always check [github.com/Generation-One/fold/wiki](https://github.com/Generation-One/fold/wiki) for current API and tool references.

# Core Operations

## 1. Searching Memories

Find relevant project knowledge using semantic search:

```bash
# Via MCP tool
mcp__fold__memory_search --query "authentication flow" --project "my-project"

# Via REST API
curl -X POST http://localhost:8765/api/memories/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "authentication flow", "project_slug": "my-project", "limit": 10}'
```

## 2. Getting Context

Retrieve AI-ready context for a specific task:

```bash
# Via MCP tool
mcp__fold__context_get --query "how does the payment service work" --project "my-project"
```

This returns semantically relevant memories, code snippets and decisions formatted for AI consumption.

## 3. Storing Memories

Add new knowledge to Fold:

```bash
# Via MCP tool
mcp__fold__memory_add \
  --project "my-project" \
  --type "decision" \
  --title "Chose Redis for session storage" \
  --content "We selected Redis over Memcached because..."

# Memory types: code, decision, session, spec, note
```

## 4. Searching Code

Find code patterns across indexed repositories:

```bash
# Via MCP tool
mcp__fold__codebase_search --query "error handling patterns" --project "my-project"
```

# MCP Tools Reference

| Tool | Purpose |
|------|---------|
| `memory_search` | Semantic search across all memories |
| `context_get` | AI-ready context retrieval |
| `memory_add` | Store new memories |
| `memory_get` | Retrieve specific memory by ID |
| `memory_update` | Update existing memory |
| `codebase_search` | Search indexed source code |
| `project_list` | List available projects |
| `graph_traverse` | Navigate knowledge graph links |

# Best Practices

**Search effectively**: Use natural language queries describing what you need, not keywords. Fold uses semantic search.

**Store context**: After significant work, store a session memory summarising decisions made and rationale.

**Link memories**: When creating memories, link them to related memories using the knowledge graph.

**Check project scope**: Always verify the correct project slug before searching or storing.

# Example Workflow

**User asks**: 'How do we handle authentication?'

1. Search for relevant context:
   ```bash
   mcp__fold__context_get --query "authentication implementation" --project "my-project"
   ```

2. Review returned memories (code, decisions, sessions)

3. Synthesise response from multiple memory types

4. If implementing changes, store a session memory afterwards:
   ```bash
   mcp__fold__memory_add \
     --project "my-project" \
     --type "session" \
     --title "Updated auth flow to use JWT" \
     --content "Changed from session-based to JWT authentication..."
   ```

# Troubleshooting

**No results returned**: Check the project slug is correct and memories exist for that project.

**Irrelevant results**: Refine your query to be more specific about the domain or feature.

**Authentication errors**: Verify your API token is valid and has access to the project.

See [Troubleshooting & FAQ](https://github.com/Generation-One/fold/wiki/Troubleshooting-FAQ) for detailed solutions.
