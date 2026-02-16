# Cloud Mode Environment Variables Reference

This document provides a complete reference for all environment variables used by ralph-workflow's cloud mode.

**Important:** These environment variables are for **cloud platform operators only**. They are not documented in user-facing CLI help or configuration files.

## Cloud Mode Control

### `RALPH_CLOUD_MODE`

**Type:** Boolean  
**Default:** `false`  
**Required:** Yes (to enable cloud mode)

Enable cloud reporting mode. When set to `true` (case-insensitive) or `1`, ralph-workflow activates cloud integration.

```bash
RALPH_CLOUD_MODE=true
```

Valid values for enabled:
- `true`, `TRUE`, `True`
- `1`

All other values (including unset, empty string, `false`, `0`) disable cloud mode.

## Cloud API Configuration

### `RALPH_CLOUD_API_URL`

**Type:** String (URL)  
**Default:** None  
**Required:** Yes (when cloud mode enabled)

Base URL for the cloud API. Must be HTTPS (HTTP will be rejected).

```bash
RALPH_CLOUD_API_URL=https://api.ralph-cloud.example.com
```

### `RALPH_CLOUD_API_TOKEN`

**Type:** String (bearer token)  
**Default:** None  
**Required:** Yes (when cloud mode enabled)

Bearer token for API authentication. This token is used in the `Authorization` header for all API requests.

```bash
RALPH_CLOUD_API_TOKEN=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Security:** This token is never logged or stored in checkpoints. It remains in memory only.

### `RALPH_CLOUD_RUN_ID`

**Type:** String  
**Default:** None  
**Required:** No (but recommended)

Unique identifier for this pipeline run, assigned by the cloud orchestrator. Used in API endpoints and progress reports.

```bash
RALPH_CLOUD_RUN_ID=run_20250215_abc123
```

## Cloud Behavior Configuration

### `RALPH_CLOUD_HEARTBEAT_INTERVAL`

**Type:** Integer (seconds)  
**Default:** `30`  
**Required:** No

Interval in seconds between heartbeat pings to the cloud API.

```bash
RALPH_CLOUD_HEARTBEAT_INTERVAL=60
```

**Recommendation:** Use at least 10 seconds to avoid overwhelming the API. For long-running pipelines, 30-60 seconds is appropriate.

### `RALPH_CLOUD_GRACEFUL_DEGRADATION`

**Type:** Boolean  
**Default:** `true`  
**Required:** No

Whether to continue pipeline execution when cloud API calls fail. When `true`, API failures are logged as warnings but do not halt the pipeline.

```bash
RALPH_CLOUD_GRACEFUL_DEGRADATION=true
```

Valid values for enabled:
- `true`, `TRUE`, `True` (or unset - defaults to true)

Valid values for disabled:
- `false`, `FALSE`, `False`
- `0`

## Git Authentication

### `RALPH_GIT_AUTH_METHOD`

**Type:** Enum  
**Default:** `ssh`  
**Required:** No

Authentication method for git remote operations.

```bash
RALPH_GIT_AUTH_METHOD=token
```

Valid values:
- `ssh` or `ssh-key` - Use SSH key authentication
- `token` - Use HTTPS token authentication
- `credential-helper` - Use git credential helper (e.g., gcloud, aws)

### `RALPH_GIT_SSH_KEY_PATH`

**Type:** String (file path)  
**Default:** None (uses default SSH key discovery)  
**Required:** No (only when auth_method=ssh and not using SSH agent)

Path to private SSH key for git authentication.

```bash
RALPH_GIT_SSH_KEY_PATH=/root/.ssh/id_rsa
```

**Security:** Ensure key file has appropriate permissions (0600).

### `RALPH_GIT_TOKEN`

**Type:** String  
**Default:** None  
**Required:** Yes (when auth_method=token)

Git access token for HTTPS authentication (e.g., GitHub personal access token).

```bash
RALPH_GIT_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

**Security:** This token is never logged. It's used to configure git credentials temporarily.

### `RALPH_GIT_TOKEN_USERNAME`

**Type:** String  
**Default:** `x-access-token`  
**Required:** No (when auth_method=token)

Username to use with token authentication. Different git hosts use different conventions:
- GitHub: `x-access-token` or `oauth2`
- GitLab: `oauth2`
- Azure DevOps: `<empty string>` or specific format

```bash
RALPH_GIT_TOKEN_USERNAME=oauth2
```

### `RALPH_GIT_CREDENTIAL_HELPER`

**Type:** String  
**Default:** `gcloud`  
**Required:** No (when auth_method=credential-helper)

Name of git credential helper to use.

```bash
RALPH_GIT_CREDENTIAL_HELPER=gcloud
```

Common values:
- `gcloud` - Google Cloud SDK credential helper
- `aws codecommit credential-helper` - AWS CodeCommit helper

## Git Remote Operations

### `RALPH_GIT_REMOTE`

**Type:** String  
**Default:** `origin`  
**Required:** No

Name of the git remote to push to.

```bash
RALPH_GIT_REMOTE=origin
```

### `RALPH_GIT_PUSH_BRANCH`

**Type:** String  
**Default:** Current branch  
**Required:** No

Target branch to push commits to. If not specified, pushes to the current branch.

```bash
RALPH_GIT_PUSH_BRANCH=feature/ralph-run-abc123
```

### `RALPH_GIT_FORCE_PUSH`

**Type:** Boolean  
**Default:** `false`  
**Required:** No

**DANGEROUS:** Allow force push to remote. Use with extreme caution.

```bash
RALPH_GIT_FORCE_PUSH=false
```

**Warning:** Force push to protected branches (main, master) should never be enabled in production.

## Pull Request Configuration

### `RALPH_GIT_CREATE_PR`

**Type:** Boolean  
**Default:** `false`  
**Required:** No

Whether to create a pull request instead of direct push to the target branch.

```bash
RALPH_GIT_CREATE_PR=true
```

### `RALPH_GIT_PR_BASE_BRANCH`

**Type:** String  
**Default:** `main` (or `master` if main doesn't exist)  
**Required:** No (when create_pr=true)

Base branch for the pull request.

```bash
RALPH_GIT_PR_BASE_BRANCH=main
```

### `RALPH_GIT_PR_TITLE`

**Type:** String (template)  
**Default:** None (uses generated title)  
**Required:** No

Pull request title template. Supports placeholders:
- `{run_id}` - The cloud run ID
- `{prompt_summary}` - Summary of the user prompt

```bash
RALPH_GIT_PR_TITLE="Ralph workflow run {run_id}"
```

### `RALPH_GIT_PR_BODY`

**Type:** String (template)  
**Default:** None (uses generated body)  
**Required:** No

Pull request body/description template. Supports same placeholders as title.

```bash
RALPH_GIT_PR_BODY="Automated changes from ralph-workflow

Run ID: {run_id}
Prompt: {prompt_summary}

Generated by ralph-workflow in cloud mode."
```

## Environment Variable Validation

When cloud mode is enabled (`RALPH_CLOUD_MODE=true`), the following validation occurs at startup:

1. **Required fields check:**
   - `RALPH_CLOUD_API_URL` must be set
   - `RALPH_CLOUD_API_TOKEN` must be set

2. **URL validation:**
   - API URL must start with `https://` (HTTP is rejected)

3. **Authentication validation:**
   - If `auth_method=token`, `RALPH_GIT_TOKEN` must be set
   - If `auth_method=credential-helper`, `RALPH_GIT_CREDENTIAL_HELPER` must be set

4. **PR validation:**
   - If `create_pr=true`, `RALPH_GIT_PR_BASE_BRANCH` should be set

If validation fails, ralph-workflow will exit with a clear error message indicating which configuration is missing or invalid.

## Example Configurations

### Minimal Cloud Mode (SSH Authentication)

```bash
RALPH_CLOUD_MODE=true
RALPH_CLOUD_API_URL=https://api.example.com
RALPH_CLOUD_API_TOKEN=secret_token_123
```

### Token Authentication with PR Creation

```bash
RALPH_CLOUD_MODE=true
RALPH_CLOUD_API_URL=https://api.example.com
RALPH_CLOUD_API_TOKEN=secret_token_123
RALPH_CLOUD_RUN_ID=run_abc123

RALPH_GIT_AUTH_METHOD=token
RALPH_GIT_TOKEN=ghp_token_here
RALPH_GIT_TOKEN_USERNAME=oauth2
RALPH_GIT_CREATE_PR=true
RALPH_GIT_PR_BASE_BRANCH=main
RALPH_GIT_PR_TITLE="Ralph run {run_id}"
```

### Google Cloud with Credential Helper

```bash
RALPH_CLOUD_MODE=true
RALPH_CLOUD_API_URL=https://api.example.com
RALPH_CLOUD_API_TOKEN=secret_token_123

RALPH_GIT_AUTH_METHOD=credential-helper
RALPH_GIT_CREDENTIAL_HELPER=gcloud
RALPH_GIT_REMOTE=origin
RALPH_GIT_PUSH_BRANCH=feature/automated-changes
```

## Security Best Practices

1. **Never log sensitive environment variables:**
   - `RALPH_CLOUD_API_TOKEN`
   - `RALPH_GIT_TOKEN`
   - `RALPH_GIT_SSH_KEY_PATH` (path is OK, contents are not)

2. **Use secret management systems:**
   - Inject tokens via Kubernetes secrets, AWS Secrets Manager, etc.
   - Avoid hardcoding tokens in Dockerfiles or CI config

3. **Rotate tokens regularly:**
   - Cloud API tokens should expire and be rotated
   - Git tokens should have minimal required permissions

4. **Audit token access:**
   - Monitor which services/users access the tokens
   - Log token usage at the cloud API layer (not in ralph-workflow)

5. **Use least privilege:**
   - Git tokens should only have push permissions to specific branches
   - Cloud API tokens should only have write access to specific run IDs
