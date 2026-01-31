# Fold Documentation

Complete documentation for Fold, a holographic memory system for development teams and AI agents.

## 📖 Documentation Map

**Start here:** [[Overview & Concepts|Overview & Concepts]] — Understand what Fold is and why it matters

### For Everyone

1. **[[Overview & Concepts|Overview & Concepts]]** (45 min read)
   - What is Fold?
   - Why "holographic"?
   - **AI benefits** (huge focus)
   - How it works
   - Key features
   - Architecture overview
   - Memory types
   - Fold vs. traditional approaches

2. **[[Getting-Started|Getting Started]]** (15 min)
   - Install with Docker (recommended)
   - Install for local development
   - First steps: connect a repo
   - Connect Claude Code
   - Troubleshooting setup issues

### For DevOps & Operators

3. **[[Configuration|Configuration]]** (30 min)
   - Environment variables (all required & optional)
   - LLM provider setup (Gemini, OpenRouter, OpenAI)
   - Auth providers (Google, GitHub, corporate OIDC)
   - Git integration (GitHub/GitLab webhooks)
   - Database & storage setup
   - Embedding models
   - Advanced configuration
   - Provider-specific setup guides

4. **[[Deployment-Operations|Deployment & Operations]]** (45 min)
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

5. **[[Core-Concepts|Core Concepts]]** (40 min)
   - What is a memory?
   - Memory types deep dive
   - How embeddings & vectors work
   - The knowledge graph
   - Link types and relationships
   - How semantic search works
   - File attachments
   - Content hashing
   - AI-suggested links

6. **[[API-Reference|API Reference]]** (30 min reference)
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

7. **[[MCP-Tools-Reference|MCP Tools Reference]]** (25 min)
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

8. **[[Advanced-Topics|Advanced Topics]]** (20 min)
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

9. **[[Troubleshooting-FAQ|Troubleshooting & FAQ]]** (reference)
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

1. Read: [[Getting-Started|Getting Started]] → Option 1 (Docker)
2. Follow the steps → You're running in 5 minutes
3. Read: [[Overview-Concepts|Overview]] to understand what you have

### I want to use Fold with Claude Code

1. Get running: [[Getting-Started|Getting Started]]
2. Connect a repo
3. Read: [[MCP-Tools-Reference|MCP Tools Reference]]
4. Use Fold from Claude Code

### I'm setting up Fold for my team

1. Read: [[Overview-Concepts|Overview]] — understand the benefits
2. Setup: [[Getting-Started|Getting Started]] + [[Configuration|Configuration]]
3. Deploy: [[Deployment-Operations|Deployment & Operations]]
4. Integrate: [[MCP-Tools-Reference|MCP Tools]] for your team's AI agents

### I'm operating Fold in production

1. Review: [[Deployment-Operations|Deployment & Operations]]
2. Reference: [[Configuration|Configuration]] for all settings
3. Monitor: Refer to Deployment section on observability
4. Troubleshoot: [[Troubleshooting-FAQ|Troubleshooting]]
5. Scale: [[Advanced-Topics|Advanced Topics]] for sharding/clustering

### I'm building something with Fold's API

1. Understand: [[Core-Concepts|Core Concepts]]
2. Reference: [[API-Reference|API Reference]]
3. Integrate: [[MCP-Tools-Reference|MCP Tools]] if building AI features
4. Advanced: [[Advanced-Topics|Advanced Topics]] for complex queries

---

## 📚 Documentation by Role

### Product Managers / Team Leads

- [[Overview-Concepts|Overview]] — Understand the value
- [[Getting-Started|Getting Started]] — See it working
- [[MCP-Tools-Reference|MCP Tools]] — Understand AI integration

### Backend Developers / Architects

- [[Overview-Concepts|Overview]] — Full picture
- [[Core-Concepts|Core Concepts]] — Deep understanding
- [[API-Reference|API Reference]] — Implementation details
- [[Advanced-Topics|Advanced Topics]] — Complex features

### DevOps / Infrastructure Engineers

- [[Getting-Started|Getting Started]] — Quick setup
- [[Configuration|Configuration]] — All settings
- [[Deployment-Operations|Deployment & Operations]] — Production guide
- [[Troubleshooting-FAQ|Troubleshooting]] — Common issues

### AI / ML Engineers

- [[Overview-Concepts|Overview]] — AI benefits section
- [[Core-Concepts|Core Concepts]] — How embeddings work
- [[MCP-Tools-Reference|MCP Tools]] — Integration patterns
- [[Advanced-Topics|Advanced Topics]] — Custom models

### Full-Stack Developers Using Fold

- [[Getting-Started|Getting Started]] — Get it running
- [[Core-Concepts|Core Concepts]] — Understand the system
- [[API-Reference|API Reference]] — Use the API
- [[MCP-Tools-Reference|MCP Tools]] — Use with Claude Code
- [[Troubleshooting-FAQ|Troubleshooting]] — Fix issues

---

## 🎯 Common Tasks

### "How do I start Fold?"
→ [[Getting-Started|Getting Started]]

### "How do I connect Claude Code to Fold?"
→ [[MCP-Tools-Reference|MCP Tools Reference]] → Setup Instructions

### "How do I index a new GitHub repository?"
→ [[Getting-Started|Getting Started]] → First Steps

### "What are the authentication options?"
→ [[Configuration|Configuration]] → Auth Providers

### "How do I set up for production?"
→ [[Deployment-Operations|Deployment & Operations]]

### "How does semantic search work?"
→ [[Core-Concepts|Core Concepts]] → Search & Retrieval

### "What's the difference between memories?"
→ [[Core-Concepts|Core Concepts]] → Memory Types

### "Can I use Fold for multiple teams?"
→ [[Advanced-Topics|Advanced Topics]] → Multi-Tenant Setup

### "Something's broken, help!"
→ [[Troubleshooting-FAQ|Troubleshooting & FAQ]]

### "How do I scale Fold?"
→ [[Deployment-Operations|Deployment & Operations]] → Scaling

---

## 📖 Reading Order Recommendations

**First time with Fold (30 min):**
1. [[Overview-Concepts|Overview]] — 15 min
2. [[Getting-Started|Getting Started]] — 15 min

**Getting productive (2 hours):**
1. [[Overview-Concepts|Overview]]
2. [[Getting-Started|Getting Started]]
3. [[Configuration|Configuration]] (skim)
4. [[MCP-Tools-Reference|MCP Tools]] (if using with Claude)

**Deep dive (4+ hours):**
1. [[Overview-Concepts|Overview]]
2. [[Core-Concepts|Core Concepts]]
3. [[API-Reference|API Reference]]
4. [[MCP-Tools-Reference|MCP Tools]]
5. [[Configuration|Configuration]]
6. [[Deployment-Operations|Deployment]]

**Ops setup (3 hours):**
1. [[Getting-Started|Getting Started]]
2. [[Configuration|Configuration]]
3. [[Deployment-Operations|Deployment]]
4. [[Troubleshooting-FAQ|Troubleshooting]] (bookmark)

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
