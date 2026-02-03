# AGENTS.md

This file helps AI coding assistants understand and work with Fold.

## What is Fold?

Fold is a semantic memory system for coding projects. It stores decisions, sessions, code patterns and team context in a vector database, making project knowledge searchable and accessible via MCP tools or REST API.

## Skills

Before working with Fold, read the appropriate skill:

| Task | Skill |
|------|-------|
| **Deploy Fold** (new installation) | [skills/deploying-fold/SKILL.md](./skills/deploying-fold/SKILL.md) |
| **Use Fold** (query, store, search) | [skills/using-fold/SKILL.md](./skills/using-fold/SKILL.md) |
| **Manage Fold** (operations, troubleshooting) | [skills/managing-fold/SKILL.md](./skills/managing-fold/SKILL.md) |

## Quick Reference

### API Endpoints

- `GET /health` — Health check
- `GET /projects` — List projects
- `POST /api/context` — Get AI-ready context for a task
- `POST /mcp` — MCP JSON-RPC endpoint

### Common Commands

```bash
# Check health
curl http://localhost:8765/health

# List projects (with auth)
curl -H "Authorization: Bearer TOKEN" http://localhost:8765/projects

# Get context for coding task
curl -X POST http://localhost:8765/api/context \
  -H "Authorization: Bearer TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"project_id": "xxx", "task": "implement feature X"}'
```

### Docker Services

| Service | Port | Purpose |
|---------|------|---------|
| fold | 8765 | API server |
| qdrant | 6333, 6334 | Vector database |
| fold-ui | 5174 | Web UI (optional) |

### Environment Variables

Key variables in `.env`:

| Variable | Purpose |
|----------|---------|
| `PUBLIC_URL` | External URL for OAuth callbacks |
| `SESSION_SECRET` | Session encryption key |
| `ADMIN_BOOTSTRAP_TOKEN` | Initial admin setup token |
| `OPENROUTER_API_KEY` | LLM provider for summaries |
| `GOOGLE_API_KEY` | Alternative LLM (Gemini) |

## Project Structure

```
fold/
├── src/                    # Rust source code
│   ├── api/               # HTTP handlers
│   ├── services/          # Business logic
│   └── models/            # Data models
├── skills/                # AI assistant skills
│   ├── deploying-fold/    # Deployment guide
│   ├── using-fold/        # Usage guide
│   └── managing-fold/     # Operations guide
├── docker-compose.yml     # Container orchestration
└── .env.example          # Configuration template
```

## Contributing

When modifying Fold:

1. The backend is Rust — use `cargo build` and `cargo test`
2. Database migrations are in `src/db/migrations/`
3. API routes are defined in `src/api/`
4. Update relevant skills if behavior changes
