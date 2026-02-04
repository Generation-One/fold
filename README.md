# Fold

**Holographic memory for development teams and AI agents.**

Fold captures, organises, and retrieves project knowledge across your codebase. Any fragment reconstructs full context, like a hologram. Ask natural language questions and get code, decisions, and history back instantly.

> **Official UI:** [fold-ui](https://github.com/Generation-One/fold-ui) — React web interface for Fold (separate repository)

---

## The Problem

Development knowledge is scattered: architecture decisions live in old PRs, the reasoning behind code sits in Slack threads, and half the system understanding leaves when team members do. AI agents work blind, reverse-engineering patterns instead of understanding them.

## The Solution

Fold makes knowledge searchable, connected, and persistent:

```
Search: "How do we handle authentication?"

Fold returns:
  - Auth code patterns across the codebase
  - Decision: "Use JWT for stateless auth"
  - Session notes from when it was built
  - Related security specs
  - Who implemented it and when

Result: Complete understanding in seconds
```

---

## Key Features

| Feature | What It Does |
|---------|--------------|
| **Holographic retrieval** | Any fragment reconstructs full context |
| **Semantic search** | Natural language queries across all knowledge |
| **Knowledge graph** | Memories linked: code, decisions, specs, sessions |
| **Git integration** | Auto-index from GitHub/GitLab webhooks |
| **MCP protocol** | Works with Claude Code, Cursor, Windsurf |
| **ACT-R decay** | Recent and accessed memories surface first |

---

## Quick Start

```bash
# Start Qdrant
docker run -p 6333:6333 qdrant/qdrant

# Clone and run
git clone https://github.com/Generation-One/fold.git
cd fold
cargo run
```

Server starts on `http://localhost:8765`

For detailed setup, see [Getting Started](https://github.com/Generation-One/fold/wiki/Getting-Started).

---

## Connect Claude Code

```bash
claude mcp add -t http -s user fold http://localhost:8765/mcp \
  --header "Authorization: Bearer YOUR_TOKEN"
```

Claude can now query your project knowledge directly:

```
Claude: memory_search("authentication patterns")
  → Returns code, decisions, and context
  → Writes code matching your actual patterns
```

See [MCP Tools Reference](https://github.com/Generation-One/fold/wiki/MCP-Tools-Reference) for all available tools.

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│         Fold Server (Rust + Axum)               │
│                                                 │
│  ┌─────────┬──────────┬────────────────────┐   │
│  │   MCP   │   REST   │     Webhooks       │   │
│  │ (Claude)│   (API)  │  (Git Integration) │   │
│  └─────────┴──────────┴────────────────────┘   │
│                    │                            │
│       ┌────────────┼────────────┐              │
│       │            │            │              │
│    Qdrant       SQLite       fold/             │
│   (vectors)   (metadata)  (git-native)         │
│                                                 │
│  LLM: Gemini (free) → OpenRouter → OpenAI     │
└─────────────────────────────────────────────────┘
```

- **Qdrant** stores vector embeddings for semantic search
- **SQLite** stores metadata and relationships
- **fold/** stores memories as markdown files committed to git

For detailed backend documentation, see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Documentation

Full documentation on the [GitHub Wiki](https://github.com/Generation-One/fold/wiki):

| Guide | Description |
|-------|-------------|
| [Overview & Concepts](https://github.com/Generation-One/fold/wiki/Overview-Concepts) | What Fold is, why it matters, how it works |
| [Getting Started](https://github.com/Generation-One/fold/wiki/Getting-Started) | Installation and first steps |
| [Configuration](https://github.com/Generation-One/fold/wiki/Configuration) | Auth, LLM providers, git integration |
| [Core Concepts](https://github.com/Generation-One/fold/wiki/Core-Concepts) | Memories, embeddings, knowledge graph |
| [API Reference](https://github.com/Generation-One/fold/wiki/API-Reference) | REST API documentation |
| [MCP Tools Reference](https://github.com/Generation-One/fold/wiki/MCP-Tools-Reference) | AI agent integration |
| [Deployment & Operations](https://github.com/Generation-One/fold/wiki/Deployment-Operations) | Production setup, scaling, monitoring |

---

## Why "Holographic"?

In a photograph, tear off a corner and it's gone. In a hologram, any piece can reconstruct the whole image.

Fold applies this principle: search for a file path and get the commits that modified it, the decisions behind it, the sessions where it was discussed, and similar patterns elsewhere. Knowledge is distributed and interconnected, not siloed.

---

## References

### Reference Implementation

- **[A-MEM: Agentic Memory for LLM Agents](https://github.com/WujiangXu/A-mem-sys)** — Wujiang Xu et al.
  The primary codebase this project initially referenced. Implements dynamic memory organisation using Zettelkasten principles with auto-generated metadata and inter-memory linking.
  Paper: [arXiv:2502.12110](https://arxiv.org/abs/2502.12110) (NeurIPS 2025)

### Theoretical Foundations

- **Sparse Distributed Memory** — Pentti Kanerva (1988)
- **Holographic Reduced Representations** — Tony Plate (1995)
- **Vector Symbolic Architectures** — Ross Gayler (2003)
- **Zettelkasten Method** — Niklas Luhmann

### Tech Stack

- **[Rust](https://www.rust-lang.org/)** + **[Axum](https://github.com/tokio-rs/axum)** — Backend server
- **[React UI](https://github.com/Generation-One/fold-ui)** — Web interface (separate repo)
- **[Qdrant](https://qdrant.tech/)** — Vector database for semantic search
- **[SQLite](https://www.sqlite.org/)** — Metadata and relationships
- **[Gemini](https://ai.google.dev/)** / **[OpenAI](https://openai.com/)** — Embeddings and LLM

---

## Licence

MIT

---

**Questions or feedback?** [Open an issue](https://github.com/Generation-One/fold/issues)
