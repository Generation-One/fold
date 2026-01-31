# Fold

**A semantic memory layer for teams and AI agents.**

Fold is a **holographic memory system** that captures, organizes, and retrieves project knowledge across your entire codebase and team. Think of it as a long-term memory for development — storing decisions, sessions, code patterns, and team context so you (and AI agents) can always find the right information at the right time.

Built on the principles of **holographic memory** from neuroscience, Fold lets you reconstruct full context from fragments. Ask a natural language question and get relevant code, decisions, past solutions, and team insights back instantly.

## Quick Links

**📖 Complete Documentation**: See the [Fold Wiki](https://github.com/Generation-One/fold/wiki)

- **[Overview & Concepts](https://github.com/Generation-One/fold/wiki/Overview-Concepts)** — What Fold is and why it matters
- **[Getting Started](https://github.com/Generation-One/fold/wiki/Getting-Started)** — Install and run Fold in 5 minutes
- **[Configuration](https://github.com/Generation-One/fold/wiki/Configuration)** — Set up auth, LLM providers, git integration
- **[API Reference](https://github.com/Generation-One/fold/wiki/API-Reference)** — Complete REST API documentation
- **[MCP Tools](https://github.com/Generation-One/fold/wiki/MCP-Tools-Reference)** — Use Fold with Claude Code and other AI agents
- **[Deployment & Operations](https://github.com/Generation-One/fold/wiki/Deployment-Operations)** — Production setup and scaling
- **[Troubleshooting](https://github.com/Generation-One/fold/wiki/Troubleshooting-FAQ)** — Common issues and solutions

## What is Fold?

```
Your Codebase + Team Activity + AI Context
              ↓
       Semantic Indexing (LLM-powered)
              ↓
      Vector Database + Knowledge Graph
              ↓
    Natural Language Search & Retrieval
              ↓
   Claude / Cursor / Your AI Assistant
```

Fold stores:
- **Codebase memories** — Auto-indexed source files
- **Session notes** — "We fixed the login bug and here's how"
- **Decisions** — "We chose Redis for caching because..."
- **Specs** — Feature requirements and technical specs
- **Commit summaries** — AI-generated summaries of git activity
- **Team insights** — Who changed what, when, and why

All memories are **semantically indexed** and **linked together** in a knowledge graph. This enables powerful features:

- **Holographic retrieval** — Any fragment reconstructs full context
- **Semantic search** — Find meaning, not keywords
- **AI-ready** — Claude, Cursor, and other AI agents query Fold via MCP
- **Zero friction** — Works with your existing git repos; no workflow changes
- **Automatic** — Git integration indexes your repos on push

## Use Cases

### For Developers
- "How does authentication work?" → Get code + decisions + sessions
- "Show me export patterns" → Find similar implementations across projects
- "What changed in the payment service?" → See all commits + decisions + related code

### For AI Agents (Claude, Cursor, Windsurf)
- Claude understands your architecture without reading raw files
- Implements features matching your exact patterns and conventions
- Respects architectural decisions automatically
- Makes informed cross-service changes

### For Teams
- Institutional memory survives team turnover
- Decisions are discoverable, not scattered in Slack
- Junior devs onboard faster with full context
- Multiple projects stay synchronized

## 2-Minute Setup

```bash
# Clone
git clone https://github.com/Generation-One/fold.git
cd fold

# Start (requires Docker)
docker-compose up -d

# Create admin
curl -X POST http://localhost:8765/auth/bootstrap \
  -H "Content-Type: application/json" \
  -d '{"token": "your-token"}'

# Done! Access at http://localhost:8765
```

See [Getting Started](https://github.com/Generation-One/fold/wiki/Getting-Started) for detailed instructions.

## Tech Stack

- **Rust** + Axum (web framework)
- **SQLite** (metadata storage)
- **Qdrant** (vector database)
- **fastembed** (local embeddings) or cloud LLM APIs
- **Docker** for easy deployment

## Key Features

| Feature | What It Does |
|---------|---|
| **Holographic Retrieval** | Any fragment of knowledge reconstructs full context |
| **Semantic Search** | Find meaning, not keywords |
| **Knowledge Graph** | Memories are linked by relationships (modifies, implements, decides, etc.) |
| **Git Integration** | Auto-index GitHub/GitLab; webhooks keep memories in sync |
| **AI-Ready (MCP)** | Works with Claude Code, Cursor, Windsurf, and other AI agents |
| **Multi-Provider LLM** | Gemini (free) → OpenRouter → OpenAI with automatic fallback |
| **Zero Friction** | No workflow changes; works with existing repos |
| **Self-Hosted** | Full control; no external APIs required (embeddings run locally) |

## Documentation

The complete documentation is on the [Fold Wiki](https://github.com/Generation-One/fold/wiki):

### For Operators
- [Getting Started](https://github.com/Generation-One/fold/wiki/Getting-Started)
- [Configuration](https://github.com/Generation-One/fold/wiki/Configuration)
- [Deployment & Operations](https://github.com/Generation-One/fold/wiki/Deployment-Operations)
- [Troubleshooting](https://github.com/Generation-One/fold/wiki/Troubleshooting-FAQ)

### For Developers
- [Core Concepts](https://github.com/Generation-One/fold/wiki/Core-Concepts)
- [API Reference](https://github.com/Generation-One/fold/wiki/API-Reference)
- [Advanced Topics](https://github.com/Generation-One/fold/wiki/Advanced-Topics)

### For AI Integration
- [MCP Tools Reference](https://github.com/Generation-One/fold/wiki/MCP-Tools-Reference)

### For Everyone
- [Overview & Concepts](https://github.com/Generation-One/fold/wiki/Overview-Concepts) — Start here!

## Why "Fold"?

Like a fold in spacetime, Fold brings distant but related knowledge close together. Any fragment of your project knowledge can reconstruct the whole picture — just like a hologram.

## License

MIT
