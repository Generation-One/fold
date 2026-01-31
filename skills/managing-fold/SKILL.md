---
name: managing-fold
description: Use this skill when deploying, configuring, or operating a Fold instance. Triggers on keywords like Fold deployment, Fold configuration, docker-compose setup, Qdrant scaling, LLM provider setup, auth configuration, webhook setup, or production operations for Fold.
---

# Managing Fold

Deploy, configure and operate Fold instances for teams and AI agents.

**Latest Documentation**: Always check [github.com/Generation-One/fold/wiki](https://github.com/Generation-One/fold/wiki) for current deployment guides and configuration options.

# Quick Deployment

## Docker Compose (Recommended)

```bash
# Clone and start
git clone https://github.com/Generation-One/fold.git
cd fold
docker-compose up -d

# Bootstrap admin user
curl -X POST http://localhost:8765/auth/bootstrap \
  -H "Content-Type: application/json" \
  -d '{"token": "your-secure-token"}'
```

## Environment Configuration

Create `.env` file with required variables:

```bash
# Required
DATABASE_URL=sqlite:///data/fold.db
QDRANT_URL=http://qdrant:6334

# LLM Provider (choose one)
GEMINI_API_KEY=your-key        # Free tier available
OPENROUTER_API_KEY=your-key    # Multi-model access
OPENAI_API_KEY=your-key        # OpenAI direct

# Authentication
JWT_SECRET=generate-secure-secret
AUTH_PROVIDER=google           # google, github, or oidc
```

See [Configuration](https://github.com/Generation-One/fold/wiki/Configuration) for all options.

# Core Operations

## 1. Project Management

```bash
# Create project via API
curl -X POST http://localhost:8765/api/projects \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "My Project", "slug": "my-project"}'

# List projects
curl http://localhost:8765/api/projects \
  -H "Authorization: Bearer $TOKEN"
```

## 2. Repository Integration

Connect GitHub/GitLab repositories for automatic indexing:

```bash
# Add repository
curl -X POST http://localhost:8765/api/repositories \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "project_slug": "my-project",
    "url": "https://github.com/org/repo",
    "branch": "main"
  }'
```

Configure webhooks for automatic sync on push events.

## 3. Health Monitoring

```bash
# Check service health
curl http://localhost:8765/health

# Check Qdrant status
curl http://localhost:6333/health
```

## 4. Backup and Restore

```bash
# Backup SQLite database
cp /data/fold.db /backups/fold-$(date +%Y%m%d).db

# Backup Qdrant snapshots
curl -X POST http://localhost:6333/collections/fold_memories/snapshots
```

# Production Configuration

## Nginx Reverse Proxy

```nginx
server {
    listen 443 ssl;
    server_name fold.example.com;

    location / {
        proxy_pass http://localhost:8765;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

## Scaling Qdrant

For large deployments, run Qdrant in cluster mode:

```yaml
# docker-compose.prod.yml
services:
  qdrant:
    image: qdrant/qdrant:latest
    environment:
      QDRANT__CLUSTER__ENABLED: "true"
    volumes:
      - qdrant_data:/qdrant/storage
```

## LLM Provider Fallback

Fold supports automatic fallback between providers:

```bash
# Primary: Gemini (free tier)
GEMINI_API_KEY=your-key

# Fallback: OpenRouter
OPENROUTER_API_KEY=your-key

# Final fallback: OpenAI
OPENAI_API_KEY=your-key
```

# Troubleshooting

**Container fails to start**: Check Docker logs with `docker-compose logs fold`

**Qdrant connection errors**: Verify Qdrant is running and accessible at configured URL

**Embedding failures**: Check LLM provider API keys and rate limits

**Webhook not triggering**: Verify webhook URL is accessible from GitHub/GitLab

See [Troubleshooting & FAQ](https://github.com/Generation-One/fold/wiki/Troubleshooting-FAQ) for detailed solutions.

# Operational Checklist

- [ ] Environment variables configured
- [ ] Database initialised
- [ ] Qdrant running and healthy
- [ ] LLM provider keys valid
- [ ] Auth provider configured
- [ ] Admin user created
- [ ] Repositories connected
- [ ] Webhooks configured
- [ ] Backups scheduled
- [ ] Monitoring enabled
