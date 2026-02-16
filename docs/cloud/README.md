# Ralph Cloud Deployment Guide

## Overview

Ralph-workflow supports running in containerized cloud environments with external orchestration. This mode is **not exposed to CLI users** and is configured entirely through environment variables.

## Architecture

In cloud mode, ralph-workflow:
- Reports progress to a cloud API via HTTP instead of terminal output
- Pushes commits to remote repository after each successful commit
- Can create pull requests at the end of runs
- Sends periodic heartbeat pings to indicate container is alive

## Configuration

Cloud mode is enabled exclusively through environment variables. No CLI flags or config file options are available.

### Required Environment Variables

```bash
RALPH_CLOUD_MODE=true
RALPH_CLOUD_API_URL=https://your-cloud-api.example.com
RALPH_CLOUD_API_TOKEN=<bearer-token>
RALPH_CLOUD_RUN_ID=<unique-run-identifier>
```

### Optional Environment Variables

```bash
RALPH_CLOUD_HEARTBEAT_INTERVAL=30  # seconds (default: 30)
RALPH_CLOUD_GRACEFUL_DEGRADATION=true  # continue on API failures (default: true)
```

## Git Authentication

Configure git authentication for remote push operations:

### SSH Key Authentication (Default)

```bash
RALPH_GIT_AUTH_METHOD=ssh
RALPH_GIT_SSH_KEY_PATH=/root/.ssh/id_rsa  # optional, uses default if not set
```

### Token Authentication

```bash
RALPH_GIT_AUTH_METHOD=token
RALPH_GIT_TOKEN=ghp_xxxxxxxxxxxx
RALPH_GIT_TOKEN_USERNAME=x-access-token  # usually "oauth2" or "x-access-token"
```

### Credential Helper

```bash
RALPH_GIT_AUTH_METHOD=credential-helper
RALPH_GIT_CREDENTIAL_HELPER=gcloud  # or aws codecommit credential-helper
```

## Remote Push Configuration

```bash
RALPH_GIT_REMOTE=origin  # default: origin
RALPH_GIT_PUSH_BRANCH=feature/ralph-run-123  # defaults to current branch
RALPH_GIT_FORCE_PUSH=false  # dangerous, use with caution
```

## Pull Request Creation

```bash
RALPH_GIT_CREATE_PR=true
RALPH_GIT_PR_BASE_BRANCH=main
RALPH_GIT_PR_TITLE="Ralph workflow run {run_id}"
RALPH_GIT_PR_BODY="Automated changes from ralph-workflow"
```

## Docker Container Setup

```dockerfile
FROM rust:1.75 as builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin ralph-workflow

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y git gh
COPY --from=builder /build/target/release/ralph-workflow /usr/local/bin/
VOLUME /workspace
WORKDIR /workspace
CMD ["ralph-workflow", "--isolation-mode"]
```

## API Contract

The cloud API should support the following endpoints:

### Progress Reporting

```
POST /v1/runs/{run_id}/progress
Content-Type: application/json
Authorization: Bearer <token>

{
  "timestamp": "2025-02-15T10:30:00Z",
  "phase": "Development",
  "previous_phase": "Planning",
  "iteration": 2,
  "total_iterations": 5,
  "review_pass": null,
  "total_review_passes": null,
  "message": "Development iteration 2/5 started",
  "event_type": {
    "type": "iteration_started",
    "iteration": 2
  }
}
```

### Heartbeat

```
POST /v1/runs/{run_id}/heartbeat
Authorization: Bearer <token>
```

### Completion

```
POST /v1/runs/{run_id}/complete
Content-Type: application/json
Authorization: Bearer <token>

{
  "success": true,
  "commit_sha": "abc123...",
  "iterations_used": 3,
  "review_passes_used": 2,
  "issues_found": false,
  "duration_secs": 245
}
```

### Artifact Upload (Optional)

```
POST /v1/runs/{run_id}/artifacts
Content-Type: multipart/form-data
Authorization: Bearer <token>

(file upload for PLAN.md, ISSUES.md, logs, etc.)
```

## Container Initialization Flow

The cloud orchestrator performs these steps before starting the container:

1. **Clone repository** into `/workspace` volume
2. **Checkout target branch** or create new branch for changes
3. **Inject credentials** via env vars or mounted secrets
4. **Set run metadata** (run ID, API endpoint, etc.)
5. **Start container** with ralph-workflow entrypoint

## Post-Pipeline Flow

After ralph-workflow completes:

1. **Push changes** to remote (or create PR)
2. **Report completion** to cloud API with:
   - Final commit SHA
   - PR URL (if created)
   - Pipeline metrics (iterations, duration)
   - Success/failure status
3. **Upload artifacts** (logs, PLAN.md, ISSUES.md) if configured
4. **Container exits** with appropriate exit code

## Security Considerations

- API token provided via env var (not logged, not in checkpoints)
- HTTPS required for all API calls (reject HTTP)
- Sensitive data (file contents, credentials) never included in progress updates
- Rate limiting awareness (respect 429 responses)
- Token rotation support (detect 401, fail gracefully)
- Git credentials (SSH keys, tokens) securely injected via environment or volume mounts

## Graceful Degradation

When cloud API is unreachable:
1. Log warning but continue pipeline execution
2. Queue progress updates in memory (bounded buffer)
3. Retry on next update with exponential backoff
4. On completion, attempt final result upload with retries
5. If all retries fail, write results to local file for manual recovery

## Testing

Use `MockCloudReporter` for integration tests:

```rust
use ralph_workflow::cloud::MockCloudReporter;

let reporter = MockCloudReporter::new();
// ... run pipeline with mock reporter ...

assert_eq!(reporter.progress_count(), 5);
assert_eq!(reporter.heartbeat_count(), 10);
```

## Troubleshooting

### Cloud mode not activating

- Ensure `RALPH_CLOUD_MODE=true` is set (case-insensitive)
- Check that API URL and token are provided
- Verify environment variables are visible to the container

### Git push failing

- Check authentication method matches available credentials
- For SSH: ensure key is mounted and permissions are correct (0600)
- For token: verify token has push permissions
- Check network connectivity to git remote

### API calls failing

- Verify API URL is reachable from container
- Check bearer token is valid
- Review cloud API logs for error details
- Enable graceful degradation to continue on API failures

### Heartbeat not sending

- Check heartbeat interval is reasonable (>= 10 seconds recommended)
- Verify cloud reporter was initialized correctly
- Check for thread panics in container logs
