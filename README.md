# Fold Documentation

Complete documentation for Fold, a holographic memory system for development teams and AI agents.

## 📖 Documentation Map

**Start here:** [[01-overview|Overview & Concepts]] — Understand what Fold is and why it matters

### For Everyone

1. **[[01-overview|Overview & Concepts]]** (45 min read)
   - What is Fold?
   - Why "holographic"?
   - **AI benefits** (huge focus)
   - How it works
   - Key features
   - Architecture overview
   - Memory types
   - Fold vs. traditional approaches

2. **[[02-getting-started|Getting Started]]** (15 min)
   - Install with Docker (recommended)
   - Install for local development
   - First steps: connect a repo
   - Connect Claude Code
   - Troubleshooting setup issues

### For DevOps & Operators

3. **[[03-configuration|Configuration]]** (30 min)
   - Environment variables (all required & optional)
   - LLM provider setup (Gemini, OpenRouter, OpenAI)
   - Auth providers (Google, GitHub, corporate OIDC)
   - Git integration (GitHub/GitLab webhooks)
   - Database & storage setup
   - Embedding models
   - Advanced configuration
   - Provider-specific setup guides

4. **[[07-deployment|Deployment & Operations]]** (45 min)
   - Production architecture
   - Docker Compose (production-grade)
   - Nginx reverse proxy
   - Database management (PostgreSQL migration)
   - Qdrant scaling
   - Monitoring & observability (Prometheus, Grafana)
   - Performance tuning
   - Backup & disaster recovery
   - Security hardening
   - Scaling strategies
   - Operational checklists

### For Developers & Architects

5. **[[04-core-concepts|Core Concepts]]** (40 min)
   - What is a memory?
   - Memory types deep dive
   - How embeddings & vectors work
   - The knowledge graph
   - Link types and relationships
   - How semantic search works
   - File attachments
   - Content hashing
   - AI-suggested links

6. **[[05-api-reference|API Reference]]** (30 min reference)
   - REST API for all endpoints
   - Authentication endpoints
   - Project management
   - Memory CRUD operations
   - Search & context queries
   - Knowledge graph traversal
   - Repositories & webhooks
   - File attachments
   - AI sessions
   - Health & monitoring
   - Complete curl examples

### For AI Integration (Claude, Cursor, Windsurf)

7. **[[06-mcp-tools|MCP Tools Reference]]** (25 min)
   - What is MCP and why it matters
   - Setup instructions (Claude Code, Cursor, Windsurf)
   - 30+ MCP tools reference
   - Tool descriptions & examples
   - Common workflows
   - Tool integration patterns
   - Best practices
   - Error handling
   - Debugging tips

### Advanced & Specialized Topics

8. **[[08-advanced-topics|Advanced Topics]]** (20 min)
   - Metadata repository sync (bidirectional)
   - Knowledge graph traversal deep dive
   - AI-suggested links
   - Batch operations
   - Workspace mapping for AI agents
   - Custom embedding models
   - Custom LLM models
   - Database sharding (large scale)
   - Webhook reliability
   - Multi-tenant setup
   - Custom authentication

9. **[[09-troubleshooting|Troubleshooting & FAQ]]** (reference)
   - Installation issues
   - Authentication problems
   - Git integration issues
   - Search problems
   - AI & Claude integration troubleshooting
   - Performance optimization
   - Webhook issues
   - LLM & embedding errors
   - FAQ (common questions)
   - Getting help & bug reports

---

## 🚀 Quick Start Paths

### I just want to try Fold locally

1. Read: [[02-getting-started|Getting Started]] → Option 1 (Docker)
2. Follow the steps → You're running in 5 minutes
3. Read: [[01-overview|Overview]] to understand what you have

### I want to use Fold with Claude Code

1. Get running: [[02-getting-started|Getting Started]]
2. Connect a repo
3. Read: [[06-mcp-tools|MCP Tools Reference]]
4. Use Fold from Claude Code

### I'm setting up Fold for my team

1. Read: [[01-overview|Overview]] — understand the benefits
2. Setup: [[02-getting-started|Getting Started]] + [[03-configuration|Configuration]]
3. Deploy: [[07-deployment|Deployment & Operations]]
4. Integrate: [[06-mcp-tools|MCP Tools]] for your team's AI agents

### I'm operating Fold in production

1. Review: [[07-deployment|Deployment & Operations]]
2. Reference: [[03-configuration|Configuration]] for all settings
3. Monitor: Refer to Deployment section on observability
4. Troubleshoot: [[09-troubleshooting|Troubleshooting]]
5. Scale: [[08-advanced-topics|Advanced Topics]] for sharding/clustering

### I'm building something with Fold's API

1. Understand: [[04-core-concepts|Core Concepts]]
2. Reference: [[05-api-reference|API Reference]]
3. Integrate: [[06-mcp-tools|MCP Tools]] if building AI features
4. Advanced: [[08-advanced-topics|Advanced Topics]] for complex queries

---

## 📚 Documentation by Role

### Product Managers / Team Leads

- [[01-overview|Overview]] — Understand the value
- [[02-getting-started|Getting Started]] — See it working
- [[06-mcp-tools|MCP Tools]] — Understand AI integration

### Backend Developers / Architects

- [[01-overview|Overview]] — Full picture
- [[04-core-concepts|Core Concepts]] — Deep understanding
- [[05-api-reference|API Reference]] — Implementation details
- [[08-advanced-topics|Advanced Topics]] — Complex features

### DevOps / Infrastructure Engineers

- [[02-getting-started|Getting Started]] — Quick setup
- [[03-configuration|Configuration]] — All settings
- [[07-deployment|Deployment & Operations]] — Production guide
- [[09-troubleshooting|Troubleshooting]] — Common issues

### AI / ML Engineers

- [[01-overview|Overview]] — AI benefits section
- [[04-core-concepts|Core Concepts]] — How embeddings work
- [[06-mcp-tools|MCP Tools]] — Integration patterns
- [[08-advanced-topics|Advanced Topics]] — Custom models

### Full-Stack Developers Using Fold

- [[02-getting-started|Getting Started]] — Get it running
- [[04-core-concepts|Core Concepts]] — Understand the system
- [[05-api-reference|API Reference]] — Use the API
- [[06-mcp-tools|MCP Tools]] — Use with Claude Code
- [[09-troubleshooting|Troubleshooting]] — Fix issues

---

## 🎯 Common Tasks

### "How do I start Fold?"
→ [[02-getting-started|Getting Started]]

### "How do I connect Claude Code to Fold?"
→ [[06-mcp-tools|MCP Tools Reference]] → Setup Instructions

### "How do I index a new GitHub repository?"
→ [[02-getting-started|Getting Started]] → First Steps

### "What are the authentication options?"
→ [[03-configuration|Configuration]] → Auth Providers

### "How do I set up for production?"
→ [[07-deployment|Deployment & Operations]]

### "How does semantic search work?"
→ [[04-core-concepts|Core Concepts]] → Search & Retrieval

### "What's the difference between memories?"
→ [[04-core-concepts|Core Concepts]] → Memory Types

### "Can I use Fold for multiple teams?"
→ [[08-advanced-topics|Advanced Topics]] → Multi-Tenant Setup

### "Something's broken, help!"
→ [[09-troubleshooting|Troubleshooting & FAQ]]

### "How do I scale Fold?"
→ [[07-deployment|Deployment & Operations]] → Scaling

---

## 📖 Reading Order Recommendations

**First time with Fold (30 min):**
1. [[01-overview|Overview]] — 15 min
2. [[02-getting-started|Getting Started]] — 15 min

**Getting productive (2 hours):**
1. [[01-overview|Overview]]
2. [[02-getting-started|Getting Started]]
3. [[03-configuration|Configuration]] (skim)
4. [[06-mcp-tools|MCP Tools]] (if using with Claude)

**Deep dive (4+ hours):**
1. [[01-overview|Overview]]
2. [[04-core-concepts|Core Concepts]]
3. [[05-api-reference|API Reference]]
4. [[06-mcp-tools|MCP Tools]]
5. [[03-configuration|Configuration]]
6. [[07-deployment|Deployment]]

**Ops setup (3 hours):**
1. [[02-getting-started|Getting Started]]
2. [[03-configuration|Configuration]]
3. [[07-deployment|Deployment]]
4. [[09-troubleshooting|Troubleshooting]] (bookmark)

---

## 💡 Key Concepts to Understand

- **Holographic Memory**: Any fragment can reconstruct full context
- **Memories**: Building blocks of knowledge (code, decisions, sessions, specs)
- **Knowledge Graph**: Memories are linked by type (modifies, implements, causes, etc.)
- **Semantic Search**: Find meaning, not keywords
- **Embeddings**: Vector representations of text for similarity matching
- **MCP**: Protocol for AI agents to access Fold
- **Git Integration**: Auto-index repos, webhooks keep memories in sync

---

## 🔗 External Resources

- **GitHub**: https://github.com/Generation-One/fold
- **Web UI**: https://github.com/Generation-One/fold-ui
- **MCP Protocol**: https://modelcontextprotocol.io/
- **Qdrant**: https://qdrant.tech/
- **Claude Code**: https://claude.com/claude-code

---

## 📝 Document Updates

These docs are maintained alongside the codebase. If you find:
- **Inaccuracies**: Please open an issue
- **Missing information**: Please suggest additions
- **Confusing explanations**: Please let us know
- **Examples that don't work**: Please report them

GitHub Issues: https://github.com/Generation-One/fold/issues
