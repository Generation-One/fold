# Fold Project - Manual Testing Guide

This guide provides step-by-step instructions for manually testing Fold functionality that requires live server interaction or cannot be fully automated.

---

## Prerequisites

### Services Running
- [x] Fold server on http://localhost:8765
- [x] Qdrant on http://localhost:6334 (or configured location)
- [x] SQLite database at ./data/fold.db

### Tools Needed
```bash
curl          # HTTP testing
jq            # JSON formatting (optional)
sqlite3       # Database inspection
```

---

## Test 1: Server Health Verification

### 1.1 Basic Health Check
```bash
curl -v http://localhost:8765/health
```

**Expected Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "timestamp": "2026-01-31T..."
}
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ status field is "healthy"
- ✓ version present
- ✓ timestamp in ISO format

### 1.2 Readiness Check
```bash
curl -v http://localhost:8765/health/ready
```

**Expected Response:**
```json
{
  "ready": true,
  "checks": [
    {
      "name": "database",
      "status": "healthy",
      "latency_ms": 0,
      "message": null
    },
    {
      "name": "qdrant",
      "status": "healthy",
      "latency_ms": 0,
      "message": null
    },
    {
      "name": "embeddings",
      "status": "healthy",
      "latency_ms": 0,
      "message": null
    }
  ]
}
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ ready field is true
- ✓ All checks have status "healthy"
- ✓ latency_ms is measured and returned
- ✓ latency < 100ms for each check

---

## Test 2: System Status & Metrics

### 2.1 System Status
```bash
curl -s http://localhost:8765/status | jq .
```

**Expected Response Structure:**
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": <number>,
  "database": {
    "connected": true,
    "pool_size": 10,
    "active_connections": <number>
  },
  "qdrant": {
    "connected": true,
    "collections": <number>,
    "total_points": <number>
  },
  "embeddings": {
    "model": "hash-placeholder",
    "loaded": true,
    "dimension": 384
  },
  "jobs": {
    "pending": 0,
    "running": 0,
    "failed_24h": 0
  },
  "metrics": {
    "total_requests": <number>,
    "total_errors": 0,
    "memory_usage_mb": 0
  }
}
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ uptime_seconds >= 0
- ✓ database.connected = true
- ✓ database.pool_size >= 5
- ✓ qdrant.connected = true
- ✓ embeddings.dimension = 384
- ✓ metrics present and valid

### 2.2 Prometheus Metrics
```bash
curl -s http://localhost:8765/metrics
```

**Expected Output:** Prometheus format with these metrics:
```
fold_requests_total <number>
fold_errors_total <number>
fold_memory_usage_bytes <number>
fold_up 1
```

**Pass Criteria:**
- ✓ Prometheus format (# HELP, # TYPE, metric_name value)
- ✓ fold_requests_total present
- ✓ fold_errors_total present
- ✓ fold_memory_usage_bytes present
- ✓ fold_up = 1 (service is up)

---

## Test 3: Authentication & Authorization

### 3.1 Unauthenticated Request (Should Fail)
```bash
curl -v http://localhost:8765/projects
```

**Expected Response:**
```
HTTP/1.1 401 Unauthorized

{"error":{"code":"UNAUTHENTICATED","message":"Not authenticated"}}
```

**Pass Criteria:**
- ✓ HTTP 401 status
- ✓ Error message indicates authentication required
- ✓ Request blocked without token

### 3.2 Create Test Token (Database Direct)
```bash
# Open SQLite database
sqlite3 ./data/fold.db

# Create test user
INSERT INTO users (id, provider, subject, email, display_name, created_at)
VALUES ('user_test_1', 'test', 'test_subject', 'test@example.com', 'Test User', datetime('now'));

# Create test token
INSERT INTO api_tokens (id, user_id, name, token_prefix, token_hash, project_ids, created_at)
VALUES (
  'token_test_1',
  'user_test_1',
  'Test Token',
  'testtoken',
  'mock_hash_for_testing',
  '[]',
  datetime('now')
);
```

**Note:** For production, use proper OIDC and token generation flow.

### 3.3 Protected Request (With Token)
```bash
curl -H "Authorization: Bearer testtoken_placeholder" \
  http://localhost:8765/projects
```

**Expected Behavior:**
- ✓ Request processes (may return 403 if scope issues)
- ✓ Not rejected with 401 (auth is recognized)

---

## Test 4: Qdrant Vector Operations

### 4.1 Check Qdrant Connection
```bash
curl -s http://localhost:6334/health
```

**Expected Response:**
```json
{
  "title": "Qdrant",
  "version": "..."
}
```

**Pass Criteria:**
- ✓ Qdrant responds to health check
- ✓ Version returned
- ✓ Service is operational

### 4.2 Check Collections (Via Database)
```bash
# Start Fold server
cargo run --release

# In another terminal, create a test memory via API
# (Requires auth token, see Test 3 for creating token)

# Check database for Qdrant collection info
sqlite3 ./data/fold.db
SELECT * FROM memories LIMIT 5;
```

**Pass Criteria:**
- ✓ Memories created and stored
- ✓ Embedding vectors sent to Qdrant
- ✓ No errors in server logs about Qdrant

---

## Test 5: Job Queue Verification

### 5.1 Check Job Status
```bash
curl -s http://localhost:8765/status/jobs
```

**Expected Response:**
```json
{
  "jobs": [],
  "total": 0,
  "pending": 0,
  "running": 0,
  "completed": 0
}
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ jobs array present (may be empty)
- ✓ Status counts present

### 5.2 Get Job Details (If Jobs Exist)
```bash
# First trigger a job (e.g., bulk memory creation)
# Then check its status

curl -s http://localhost:8765/status/jobs/{job_id}
```

**Expected Response:**
```json
{
  "id": "job_...",
  "type": "...",
  "status": "pending|running|completed|failed",
  "progress": {
    "total": <number>,
    "processed": <number>,
    "percent": <number>
  }
}
```

**Pass Criteria:**
- ✓ Job found and status returned
- ✓ Progress tracking working
- ✓ Latency measured

---

## Test 6: Memory & Embedding Lifecycle

### 6.1 Create Memory (Requires Auth Token)
```bash
curl -X POST http://localhost:8765/projects/{project_id}/memories \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{
    "type": "general",
    "title": "Test Memory",
    "content": "This is test content for semantic search"
  }'
```

**Expected Response:**
```json
{
  "id": "mem_...",
  "type": "general",
  "title": "Test Memory",
  "content": "This is test content...",
  "created_at": "2026-01-31T..."
}
```

**Pass Criteria:**
- ✓ HTTP 201 status
- ✓ Memory created with id
- ✓ Fields preserved
- ✓ Timestamps present

### 6.2 Verify Embedding in Qdrant
Check database and logs:
```bash
# Check for memory in database
sqlite3 ./data/fold.db
SELECT id, type, title FROM memories ORDER BY created_at DESC LIMIT 5;

# Check server logs for Qdrant operations
# Should see messages like:
# "Stored embedding in Qdrant" or similar
```

**Pass Criteria:**
- ✓ Memory found in database
- ✓ No errors about Qdrant in logs
- ✓ Memory ready for search

### 6.3 Search Memory
```bash
curl -X POST http://localhost:8765/projects/{project_id}/search \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "test content"
  }'
```

**Expected Response:**
```json
{
  "results": [
    {
      "id": "mem_...",
      "type": "general",
      "title": "Test Memory",
      "similarity_score": 0.95
    }
  ]
}
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ Results returned (may be empty if Qdrant down)
- ✓ similarity_score present and 0-1 range

### 6.4 Update Memory
```bash
curl -X PUT http://localhost:8765/projects/{project_id}/memories/{memory_id} \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Updated Memory",
    "content": "Updated content"
  }'
```

**Expected Response:**
```json
{
  "id": "mem_...",
  "type": "general",
  "title": "Updated Memory",
  "content": "Updated content",
  "updated_at": "2026-01-31T..."
}
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ Fields updated
- ✓ updated_at changed
- ✓ Embedding re-indexed in Qdrant

### 6.5 Delete Memory
```bash
curl -X DELETE http://localhost:8765/projects/{project_id}/memories/{memory_id} \
  -H "Authorization: Bearer {token}"
```

**Expected Response:**
```
HTTP/1.1 204 No Content
```

**Pass Criteria:**
- ✓ HTTP 204 status
- ✓ Memory deleted from database
- ✓ Embedding removed from Qdrant
- ✓ Subsequent GET returns 404

---

## Test 7: Attachment Handling

### 7.1 Upload Attachment
```bash
curl -X POST http://localhost:8765/projects/{project_id}/memories/{memory_id}/attachments \
  -H "Authorization: Bearer {token}" \
  -F "file=@/path/to/test.txt"
```

**Expected Response:**
```json
{
  "id": "att_...",
  "memory_id": "mem_...",
  "filename": "test.txt",
  "content_type": "text/plain",
  "size_bytes": 1234,
  "created_at": "2026-01-31T..."
}
```

**Pass Criteria:**
- ✓ HTTP 201 status
- ✓ Attachment created
- ✓ File stored in ./data/attachments/
- ✓ Metadata in database

### 7.2 Download Attachment
```bash
curl -o /tmp/downloaded.txt \
  http://localhost:8765/projects/{project_id}/memories/{memory_id}/attachments/{att_id} \
  -H "Authorization: Bearer {token}"
```

**Pass Criteria:**
- ✓ HTTP 200 status
- ✓ File downloaded successfully
- ✓ Content matches original
- ✓ Content-Type header correct

### 7.3 Delete Attachment
```bash
curl -X DELETE http://localhost:8765/projects/{project_id}/memories/{memory_id}/attachments/{att_id} \
  -H "Authorization: Bearer {token}"
```

**Pass Criteria:**
- ✓ HTTP 204 status
- ✓ File deleted from filesystem
- ✓ Metadata deleted from database
- ✓ Subsequent GET returns 404

---

## Test 8: Error Handling

### 8.1 Invalid Project ID
```bash
curl -v http://localhost:8765/projects/invalid_id \
  -H "Authorization: Bearer {token}"
```

**Expected Response:**
```
HTTP/1.1 404 Not Found

{"error":{"code":"NOT_FOUND","message":"Project not found"}}
```

**Pass Criteria:**
- ✓ HTTP 404 status
- ✓ Descriptive error message

### 8.2 Malformed Request
```bash
curl -X POST http://localhost:8765/projects/{project_id}/memories \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"invalid": json'
```

**Expected Response:**
```
HTTP/1.1 400 Bad Request

{"error":{"code":"INVALID_REQUEST",...}}
```

**Pass Criteria:**
- ✓ HTTP 400 status
- ✓ Error indicates parsing/validation issue

### 8.3 Qdrant Unavailable Graceful Degradation
```bash
# Stop Qdrant (docker stop qdrant or kill service)

# Create memory - should still work
curl -X POST http://localhost:8765/projects/{project_id}/memories \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{...}'
```

**Expected Behavior:**
- ✓ Memory created successfully
- ✓ Warning logged about Qdrant
- ✓ Server continues operating
- ✓ /health/ready shows degraded status

---

## Test 9: Background Health Monitoring

### 9.1 Check Monitor Output
Monitor script running in background checks every 5 minutes:
```bash
tail -f /tmp/health_monitor.log  # Or wherever output redirected
```

**Expected Output Every 5 Minutes:**
```
[2026-01-31 10:52:17] ✓ Health check OK (HTTP 200)
[2026-01-31 10:57:17] ✓ Health check OK (HTTP 200)
[2026-01-31 11:02:17] ✓ Health check OK (HTTP 200)
```

**Pass Criteria:**
- ✓ Check runs on schedule
- ✓ HTTP 200 returned consistently
- ✓ No stalls detected
- ✓ Clear success/failure indication

---

## Test 10: Load & Performance

### 10.1 Concurrent Requests
```bash
# Test 10 concurrent requests
for i in {1..10}; do
  curl -s http://localhost:8765/health &
done
wait

# Check that all completed successfully
```

**Expected Behavior:**
- ✓ All requests complete
- ✓ No timeout errors
- ✓ Server remains responsive
- ✓ Responses consistent

### 10.2 Large Bulk Operation
```bash
# Create 50 memories in bulk (requires implementation)
curl -X POST http://localhost:8765/projects/{project_id}/memories/bulk \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '[
    {"type": "general", "title": "Mem 1", "content": "..."},
    {"type": "general", "title": "Mem 2", "content": "..."},
    ...
  ]'
```

**Expected Behavior:**
- ✓ Operation completes in <5 seconds
- ✓ All memories created
- ✓ Progress tracked
- ✓ No memory leaks

---

## Troubleshooting

### Server Won't Start
```bash
# Check if port 8765 is already in use
lsof -i :8765

# Check database permissions
ls -la ./data/fold.db

# Check environment variables
echo $QDRANT_URL
echo $DATABASE_PATH
```

### Qdrant Not Connected
```bash
# Check Qdrant is running
curl http://localhost:6334/health

# Check network connectivity
telnet localhost 6334

# Check logs for connection errors
grep "Qdrant" server.log
```

### Health Check Returns Unhealthy
```bash
# Check each dependency separately
curl http://localhost:8765/health/ready

# Verify database
sqlite3 ./data/fold.db "SELECT 1;"

# Verify Qdrant
curl http://localhost:6334/health
```

---

## Test Automation

### Run All Tests
```bash
cargo test --release
```

### Run Specific Test Suite
```bash
cargo test --test api_integration
cargo test --test db_integration
cargo test --test webhook_integration
```

### Run Live Server Tests
```bash
# Terminal 1
cargo run --release

# Terminal 2
cargo test --release -- --test-threads=1
```

---

## Success Criteria Summary

✅ All health endpoints return 200
✅ Status endpoint shows all components healthy
✅ Metrics in Prometheus format
✅ Authentication working and enforced
✅ Memories can be created, updated, deleted
✅ Embeddings stored and searchable
✅ Attachments working
✅ Error handling graceful
✅ Background monitoring functional
✅ No performance degradation under load

---

**Last Updated:** January 31, 2026
**Status:** Ready for manual testing
**Next Step:** Run tests and verify results
