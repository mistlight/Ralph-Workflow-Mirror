# Security Model

This document describes the security model, threat model, and security guarantees of Ralph Workflow's isolation modes.

## Overview

Ralph Workflow provides two primary security modes for isolating AI agent execution:

1. **Container Mode** (Linux) - Uses Docker/Podman containers
2. **User-Account Mode** (macOS/Linux) - Uses dedicated user account

### Platform-Specific Defaults

The security mode defaults to the most appropriate option for each platform:

- **Linux**: Defaults to **Container Mode** (via `security_mode: "auto"`)
  - Strongest isolation with container boundaries
  - Requires Docker or Podman to be installed

- **macOS**: Defaults to **User-Account Mode** (via `security_mode: "auto"`)
  - Linux containers cannot run macOS binaries
  - Dedicated user (`ralph-agent`) provides filesystem isolation

You can override the default with:
- `--security-mode container` - Force container mode
- `--security-mode user-account` - Force user-account mode
- `--security-mode none` - Disable security isolation
- `--no-container-mode` - Disable container mode (when using `security_mode: "auto"`)

Both modes are designed to prevent AI agents from:
- Modifying files outside the designated workspace
- Accessing sensitive system files
- Executing arbitrary commands on the host
- Escorting isolation boundaries

## Threat Model

### Adversary Model

We consider the AI agent as a potentially adversarial actor that:
- May attempt to read sensitive files (credentials, keys, personal data)
- May attempt to modify system configuration
- May attempt to execute arbitrary commands
- May attempt to escape isolation boundaries
- Receives untrusted input from prompts and external sources

### Attack Vectors

The following attack vectors are considered and mitigated:

#### 1. Filesystem Escape

**Threat**: Agent attempts to read or write files outside the workspace.

**Container Mode Mitigation**:
- Only workspace is mounted read-write
- Sensitive paths (`/etc`, `/proc`, `/sys`, `/dev`) are explicitly blocked
- Read-only mounts for configuration directories
- No host root access

**User-Account Mode Mitigation**:
- Agent runs as dedicated user with limited permissions
- Cannot modify files owned by other users
- Cannot access sensitive directories without proper permissions

#### 2. Command Injection

**Threat**: Agent attempts to inject malicious commands through prompts or configuration.

**Mitigations**:
- Shell argument parsing with `shell_words::split()`
- Proper single-quote escaping for wrapper scripts
- Validation of environment variable names and values
- Null byte detection in all user inputs

#### 3. Environment Variable Injection

**Threat**: Agent attempts to set dangerous environment variables to manipulate process behavior.

**Mitigations**:
- CCS (external config) filtering of dangerous env vars: `PATH`, `LD_*`, `DYLD_*`, `IFS`, etc.
- Value validation for shell metacharacters
- Newline detection to prevent multi-line injection

#### 4. Path Traversal

**Threat**: Agent attempts to access files outside workspace via path traversal.

**Mitigations**:
- CCS settings path validation with `is_path_safe_for_resolution()`
- Blocking of absolute paths from external config
- Resolution of `~` to home directory with validation

#### 5. Container Escape

**Threat**: Agent attempts to escape container via privileged operations.

**Mitigations**:
- Containers run without privileged mode
- No host socket or volume mounts for system directories
- User namespace isolation (when available)
- Resource limits enforced

## Security Guarantees

### Container Mode (Linux)

**Filesystem Isolation**:
- ✓ Agent can only write to `/workspace`
- ✓ Agent cannot read `/etc/passwd`, `/etc/shadow`, or other sensitive files
- ✓ Agent cannot write outside mounted directories
- ✓ Agent cannot mount additional volumes

**Process Isolation**:
- ✓ Agent runs as unprivileged user inside container
- ✓ Agent cannot see or interact with host processes
- ✓ Container exit cleans up all agent processes

**Network Isolation**:
- ✓ Network access controlled by configuration
- ✓ Ports are explicitly published (not all-by-default)
- ✓ localhost binding only by default (no external exposure)

**Tool Access**:
- ✓ Host tools are mounted read-only (cannot modify)
- ✓ Language version managers are isolated to container
- ✓ System binaries are read-only
- ✓ `~/.claude` is mounted read-only to `/home/ralph/.claude` for MCP/Skills access
- ✓ `~/.config/claude` is mounted read-only to `/home/ralph/.config/claude` for additional MCP configuration
- ✓ Extended version manager support: asdf, mise, chruby, fnm, volta
- ✓ Python virtual environments (.venv, venv, env) are automatically mounted
- ✓ Go workspace bin directories are automatically mounted

**Known Limitations**:
- Container with `--network=host` can access host network services
- Privileged container mode (if enabled) reduces isolation
- Container breakout vulnerabilities in runtime may apply

### User-Account Mode (macOS/Linux)

**Filesystem Isolation**:
- ✓ Agent can only write to files it owns
- ✓ Agent cannot modify files owned by other users
- ✓ Agent cannot access sensitive directories without permissions
- ✓ Agent's home directory is isolated

**Process Isolation**:
- ✓ Agent processes run with dedicated user UID
- ✓ Agent cannot send signals to other users' processes
- ✓ Process visibility is limited to user's own processes

**Tool Access**:
- ✓ Agent has access to all user-installed tools
- ✓ No duplication or mounting required
- ✓ Language version managers work seamlessly
- ✓ Symbolic links to host version managers (rbenv, rvm, nvm, pyenv, asdf, mise, fnm, volta, cargo)
- ✓ Automatic shell initialization for all detected version managers

**Known Limitations**:
- Agent can run `sudo` if configured (for package management)
- Agent can read world-readable files on the system
- No isolation from host processes (same filesystem namespace)

## Configuration Security

### CCS (External Config) Security

When loading configuration from external sources (CCS), the following validations apply:

1. **Dangerous Environment Variables**: Blocked from external config
   - `PATH`, `LD_*`, `DYLD_*`, `IFS`, `TERM`, etc.
   - Full list in `src/agents/config.rs::DANGEROUS_ENV_VAR_PATTERNS`

2. **Unsafe Environment Variable Values**: Rejected if contain:
   - Shell metacharacters: `$`, `` ` ``, `\`, `|`, `;`, `&`, `>`, `<`, `*`, `?`, `[`, `]`, `{`, `}`, `(`, `)`
   - Newlines or carriage returns

3. **Path Traversal Prevention**: Settings paths must be:
   - Relative paths with `~/` prefix (expanded to home directory)
   - Absolute paths are rejected from external config

## Security Best Practices

### For Users

1. **Security is Enabled by Default** - Container mode (Linux) or user-account mode (macOS) is used automatically
2. **Use Container Mode on Linux** when possible for strongest isolation
3. **Review Agent Commands** before running in production
4. **Keep Container Runtime Updated** to patch security vulnerabilities
5. **Use Read-Only Mounts** for tools that don't need write access
6. **Limit Network Access** with `--container-network disabled` when not needed
7. **Run `--security-check`** to verify setup before use

### For Developers

1. **Validate All External Input**: Paths, env vars, commands
2. **Use Safe Shell Parsing**: `shell_words::split()` not manual splitting
3. **Test Isolation**: Verify agent cannot escape boundaries
4. **Document Security Assumptions**: What is trusted vs untrusted
5. **Follow Principle of Least Privilege**: Grant minimal necessary access

## Security Auditing

### Verification Steps

1. **Filesystem Isolation Test**:
   ```bash
   # In container mode, verify cannot read /etc/passwd
   ralph --security-mode container -- "cat /etc/passwd"
   # Should fail or return container file only
   ```

2. **Write Protection Test**:
   ```bash
   # Verify cannot write outside workspace
   ralph -- "echo 'test' > /tmp/test.txt"
   # Should fail
   ```

3. **Process Isolation Test**:
   ```bash
   # Verify cannot see host processes
   ralph -- "ps aux"
   # Should only show container processes
   ```

4. **Security Check**:
   ```bash
   ralph --security-check
   # Should report all systems operational
   ```

## Reporting Security Issues

If you discover a security vulnerability or isolation bypass:

1. **Do not open a public issue**
2. Email details to: security@ralph-workflow.dev
3. Include: Steps to reproduce, expected behavior, actual behavior
4. Allow time for patch before disclosure

## Security References

- [OWASP Command Injection](https://owasp.org/www-community/attacks/Command_Injection)
- [Container Security Best Practices](https://snyk.io/blog/10-docker-image-security-best-practices/)
- [Linux User Namespace Isolation](https://man7.org/linux/man-pages/man7/user_namespaces.7.html)
- [Docker Security](https://docs.docker.com/engine/security/)

## Changelog

### Version 0.4.0
- **Security is now enabled by default**: Container mode on Linux, user-account mode on macOS
- Platform-specific defaults via `security_mode: "auto"`
- Tool discovery for language version managers (.rbenv, .nvm, .pyenv, .jenv, etc.)
- Port auto-detection and forwarding for dev servers (Rails, Django, Vite, etc.)
- MCP/Skills directory mounting (`~/.claude`) in container mode

### Version 0.5.0
- **Extended Version Manager Support**: Added detection for asdf, mise, chruby, fnm, volta
- **Project-Local Tool Detection**: Automatic mounting of Python virtualenvs (.venv, venv, env)
- **Enhanced Shell Initialization**: Shell init scripts generated for all detected version managers
- **Additional MCP Configuration**: Mount `~/.config/claude` for MCP servers and skills
- **Improved Port Detection**: Added SvelteKit, SolidJS, Astro, Remix framework support
- **Codex Agent Detection**: Added `is_codex_agent()` helper for future integration

### Version 0.3.6
- Added CCS environment variable filtering
- Added path traversal prevention for external config
- Added shell escaping for git wrapper scripts
- Added security isolation tests
- Added `--security-check` command
