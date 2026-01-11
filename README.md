# Ralph

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

Ralph is a **PROMPT-driven, multi-agent orchestrator** inspired by [Geoffrey Huntley](https://ghuntley.com/ralph/) for git repositories. It runs a developer agent (default: Claude) to make progress against `PROMPT.md`, then runs a reviewer agent (default: Codex) to review and apply fixes, optionally running your checks and creating a commit.

**Key Features:**
- Multi-agent workflow: Separate developer and reviewer agents for implementation and quality
- PROMPT-driven: Define your goals in `PROMPT.md`, let agents do the work
- Automatic fallback: Switch agents on rate limits or errors
- Pluggable agents: Use Claude, Codex, OpenCode, Aider, Goose, Cline, and more
- Streaming output: Real-time JSON parsing for Claude and Codex output
- Git worktree support: Works correctly in git worktrees

## What Ralph does

When you run `ralph`, it:

1. Ensures a few working files exist (creates them if missing): `PROMPT.md`, `.agent/STATUS.md`, `.agent/NOTES.md`, `.agent/ISSUES.md`, plus `.agent/logs/`.
   - If no agents config exists yet, Ralph first creates a full default template at `.agent/agents.toml` (or `RALPH_AGENTS_CONFIG`) and exits so you can review/edit it.
2. Runs the **developer agent** for `N` iterations (default: 5), prompting it based on `PROMPT.md`.
3. Runs the **reviewer agent** in a review -> fix -> review loop (default: 2 review passes).
4. Optionally runs a fast check after each dev iteration and/or a full check at the end.
5. Stages and commits changes (either via the reviewer, or via Ralph as a fallback).

## Prerequisites

- You must run Ralph inside a git repository (including git worktrees).
- You must have the agent CLIs you want to use installed and authenticated:
  - Defaults: `claude` and `codex`
  - Built-in alternatives: `opencode`, `aider`, `goose`, `cline`, `continue`, `amazon-q`, `gemini`
- Optional: `pbcopy` (macOS) for clipboard copy of prompts in interactive mode.

## Install

### From source (recommended)

```bash
# Clone the repository
git clone https://codeberg.org/mistlight/RalphWithReviewer.git
cd RalphWithReviewer

# Build optimized release
cargo build --release

# Install to ~/.cargo/bin (or use make install-local)
cargo install --path .
```

### Via Makefile

- System install (may require sudo): `make install`
- User install (no sudo): `make install-local`

### Development build

```bash
cargo build
./target/debug/ralph --help
```

## Quick start

1. In your repo root, create or edit `PROMPT.md` to describe what you want done.
2. Run Ralph:

```bash
ralph "feat: implement the prompt"
```

If you omit the commit message, Ralph uses a default.

Use `--preset opencode` to run `opencode` for both roles:

```bash
ralph --preset opencode "chore: run with opencode"
```

## CLI usage

```
ralph [COMMIT_MSG] [OPTIONS]
```

Common options:

- `--developer-iters <N>` (alias: `--claude-iters`): number of developer iterations
- `--reviewer-reviews <N>` (alias: `--codex-reviews`): number of reviewer re-review passes after fixes
- `--preset <default|opencode>`: pick a common agent combination quickly
- `--developer-agent <NAME>` (alias: `--driver-agent`): which agent to use for the developer role
- `--reviewer-agent <NAME>`: which agent to use for the reviewer role
- `--use-fallback`: enable automatic agent switching on failures (rate limits, token exhaustion, etc.)
- `-v, --verbosity <0..4>`: output verbosity (0=quiet, 2=default, 3=full, 4=debug; see also `--quiet`, `--full`, `--debug`)

Run `ralph --help` for the authoritative list.

## Configuration

Ralph's configuration is split across:

- **Environment variables** (core runtime configuration)
- **Config files** (TOML) for defining and overriding agent commands

### Config file locations

Ralph loads config files in order of priority (later overrides earlier):

1. **Built-in defaults** - All standard agents pre-configured
2. **Global config** (`~/.config/ralph/agents.toml`) - Your personal defaults
3. **Project config** (from `RALPH_AGENTS_CONFIG`, default: `.agent/agents.toml`) - Repo-specific settings

Notes:
- `RALPH_AGENTS_CONFIG` selects the *project config path*; it does not add an extra layer. If you set it, `.agent/agents.toml` is skipped unless you point `RALPH_AGENTS_CONFIG` at `.agent/agents.toml`.
- Relative `RALPH_AGENTS_CONFIG` paths are resolved against the git repo root (so running from subdirectories and git worktrees behaves consistently).

To generate config files:

```bash
# Create per-repo config
ralph --init

# Create global config (applies to all repositories)
ralph --init-global
```

### Config file format

```toml
# ~/.config/ralph/agents.toml or .agent/agents.toml

[agents.myagent]
cmd = "my-ai-tool run"
json_flag = "--json-stream"
yolo_flag = "--auto-fix"
verbose_flag = "--verbose"
can_commit = true
json_parser = "claude"  # "claude" | "codex" | "generic"
```

Fields:

- `cmd` (required): base command to run the agent
- `json_flag` (optional): flag appended when Ralph wants JSON output
- `yolo_flag` (optional): flag appended for non-interactive/autonomous mode
- `verbose_flag` (optional): flag appended for verbose output
- `can_commit` (optional, default `true`): whether this agent is allowed to run `git commit`
- `json_parser` (optional): how to parse the agent's output (`claude`, `codex`, or `generic`)

### Examples

Override the built-in `claude` agent command:

```toml
[agents.claude]
cmd = "claude -p"
json_flag = "--output-format=stream-json"
yolo_flag = "--dangerously-skip-permissions"
verbose_flag = "--verbose"
json_parser = "claude"
```

Add an agent that prints plain text (no JSON parsing):

```toml
[agents.my_plain_agent]
cmd = "some-tool chat"
json_parser = "generic"
```

You can start from `examples/agents.toml` and copy it into `.agent/agents.toml`.

### Agent priority

Ralph resolves agents in this order:

1. Built-in defaults (`claude`, `codex`, `opencode`, `aider`, `goose`, `cline`, `continue`, `amazon-q`, `gemini`)
2. Global config (`~/.config/ralph/agents.toml`) overrides by name
3. Per-repo config (`.agent/agents.toml`) overrides by name
4. `RALPH_DEVELOPER_CMD` / `RALPH_REVIEWER_CMD` (if set) override the command Ralph runs for those roles

Pick agents by name:

```bash
ralph --developer-agent claude --reviewer-agent codex
ralph --developer-agent myagent --reviewer-agent my_plain_agent
```

### Agent chains and fallback

Ralph supports configuring preferred agents and automatic fallback switching. The agent chain defines:

1. **Preferred agent** (first in the list): Primary choice for each role
2. **Fallback agents** (rest of the list): Tried in order if the preferred fails

If you don't pass `--developer-agent` / `--reviewer-agent` and you don't set `RALPH_DEVELOPER_AGENT` / `RALPH_REVIEWER_AGENT`, Ralph uses the first entry in the configured agent chain as the default for that role.

When `--use-fallback` is enabled, Ralph automatically switches agents on:

- Rate limits (429 errors)
- Token/context exhaustion
- API unavailability (503, timeout)
- Authentication failures
- Command not found

Enable fallback via CLI or environment variable:

```bash
ralph --use-fallback
# or
RALPH_USE_FALLBACK=1 ralph
```

Configure agent chains in your config file (use either `[agent_chain]` or legacy `[fallback]`):

```toml
[agent_chain]
developer = ["claude", "codex", "goose"]  # claude preferred, others are fallbacks
reviewer = ["codex", "claude"]             # codex preferred for reviews
max_retries = 3                            # retries per agent before trying next
retry_delay_ms = 1000                      # delay between retries
```

## Environment variables

### Agent selection and commands

| Variable | Description | Default |
|----------|-------------|---------|
| `RALPH_DEVELOPER_AGENT` | Developer agent name | `claude` |
| `RALPH_DRIVER_AGENT` | Alias for `RALPH_DEVELOPER_AGENT` | - |
| `RALPH_REVIEWER_AGENT` | Reviewer agent name | `codex` |
| `RALPH_DEVELOPER_CMD` | Override developer command | - |
| `RALPH_REVIEWER_CMD` | Override reviewer command | - |
| `RALPH_AGENTS_CONFIG` | Project agents TOML path (replaces `.agent/agents.toml`) | `.agent/agents.toml` |
| `RALPH_PRESET` | Preset agent combo (`default`, `opencode`) | - |

### Iterations and review passes

| Variable | Description | Default |
|----------|-------------|---------|
| `RALPH_DEVELOPER_ITERS` | Developer iterations | `5` |
| `RALPH_REVIEWER_REVIEWS` | Reviewer re-review passes | `2` |

### Checks

| Variable | Description |
|----------|-------------|
| `FAST_CHECK_CMD` | Command run after each dev iteration (non-blocking) |
| `FULL_CHECK_CMD` | Command run at the end (blocking; failure aborts) |

Examples:

```bash
FAST_CHECK_CMD="cargo fmt -- --check && cargo clippy -D warnings"
FULL_CHECK_CMD="cargo test"
```

### Behavior and output

| Variable | Description | Default |
|----------|-------------|---------|
| `RALPH_INTERACTIVE` | Keep agents interactive | `1` |
| `RALPH_PROMPT_PATH` | Where to save last prompt | `.agent/last_prompt.txt` |
| `RALPH_DEVELOPER_CONTEXT` | Developer context level (0=minimal, 1=normal) | `1` |
| `RALPH_REVIEWER_CONTEXT` | Reviewer context level (0=minimal/fresh, 1=normal) | `0` |
| `RALPH_VERBOSITY` | Output verbosity (0-4) | `2` |
| `RALPH_USE_FALLBACK` | Enable automatic agent fallback | `0` |

## Files Ralph creates

Ralph uses a `.agent/` working directory for run state and logs:

- `.agent/logs/`: agent run logs (JSON lines or plain text)
- `.agent/STATUS.md`: high-level status tracking
- `.agent/NOTES.md`: scratchpad notes
- `.agent/ISSUES.md`: issues found / to address
- `.agent/PLAN.md`: current iteration plan (created then deleted each iteration)
- `.agent/commit-message.txt`: generated commit message
- `.agent/archive/`: archives prior status/notes/issues when using "fresh eyes" reviewer context
- `.agent/last_prompt.txt`: last prompt generated and sent to an agent (configurable)

If you don't want these tracked in git, add this to your repo's `.gitignore`:

```gitignore
.agent/
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Not a git repository" | Run Ralph inside a repo (or `cd` into the repo root) |
| Agent command not found | Ensure the CLI is installed and on your `PATH`, or define a custom agent in `.agent/agents.toml` |
| Garbled output / parsing errors | Set `json_parser = "generic"` for that agent in `.agent/agents.toml` |
| No commit created | Ralph falls back to `git add -A` + `git commit` if the reviewer doesn't commit |
| Rate limit errors | Use `--use-fallback` to auto-switch agents, or wait and retry |

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Run tests: `cargo test`
4. Run lints: `cargo clippy && cargo fmt --check`
5. Submit a pull request

## License

AGPL-3.0. See [LICENSE](LICENSE).
