# Ralph

Ralph is a **PROMPT-driven, multi-agent orchestrator** inspired by [Geoffrey Huntley](https://ghuntley.com/ralph/) for git repositories. It runs a developer agent (default: Claude) to make progress against `PROMPT.md`, then runs a reviewer agent (default: Codex) to review and apply fixes, optionally running your checks and creating a commit.

## What Ralph does

When you run `ralph`, it:

1. Ensures a few working files exist (creates them if missing): `PROMPT.md`, `.agent/STATUS.md`, `.agent/NOTES.md`, `.agent/ISSUES.md`, plus `.agent/logs/`.
2. Runs the **developer agent** for `N` iterations (default: 5), prompting it based on `PROMPT.md`.
3. Runs the **reviewer agent** in a review → fix → review loop (default: 2 review passes).
4. Optionally runs a fast check after each dev iteration and/or a full check at the end.
5. Stages and commits changes (either via the reviewer, or via Ralph as a fallback).

## Prerequisites

- You must run Ralph inside a git repository.
- You must have the agent CLIs you want to use installed and authenticated:
  - Defaults: `claude` and `codex`
  - Built-in alternatives: `opencode`, `aider`
- Optional: `pbcopy` (macOS) for clipboard copy of prompts in interactive mode.

## Install

### Build locally (debug)

`cargo build`

Run with:

`./target/debug/ralph --help`

### Build optimized release

`cargo build --release`

Run with:

`./target/release/ralph --help`

### Install via Makefile

- System install (may require sudo): `make install`
- User install (no sudo): `make install-local`

## Quick start

1. In your repo root, create or edit `PROMPT.md` to describe what you want done.
2. Run Ralph:

`ralph "feat: implement the prompt"`

If you omit the commit message, Ralph uses a default.

## CLI usage

`ralph [COMMIT_MSG] [OPTIONS]`

Common options:

- `--claude-iters <N>`: number of developer iterations
- `--codex-reviews <N>`: number of reviewer re-review passes after fixes
- `--developer-agent <NAME>`: which agent to use for the developer role
- `--reviewer-agent <NAME>`: which agent to use for the reviewer role
- `-v, --verbosity <0..3>`: output verbosity (0=quiet, 3=full)

Run `ralph --help` for the authoritative list.

## Configuration overview

Ralph’s configuration is split across:

- **Environment variables** (core runtime configuration)
- **An agents config file** (TOML) for defining and overriding agent commands

### Where to store the config file

By default, Ralph looks for an agents config file at:

- `.agent/agents.toml`

This is intended to live in your **repository root** (next to `PROMPT.md`).

Note: the config path is read as a normal filesystem path. If you run `ralph` from a subdirectory, relative paths may not resolve the way you expect. The simplest approach is to run `ralph` from the repo root, or set `RALPH_AGENTS_CONFIG` to an absolute path.

### What the config file should look like (`.agent/agents.toml`)

Create `.agent/agents.toml` with one or more agent definitions:

```toml
# .agent/agents.toml

[agents.myagent]
cmd = "my-ai-tool run"
json_flag = "--json-stream"
yolo_flag = "--auto-fix"
verbose_flag = "--verbose"
can_commit = true
json_parser = "claude" # "claude" | "codex" | "generic"
```

Fields:

- `cmd` (required): base command to run the agent
- `json_flag` (optional): flag appended when Ralph wants JSON output
- `yolo_flag` (optional): flag appended for non-interactive/autonomous mode
- `verbose_flag` (optional): flag appended for verbose output
- `can_commit` (optional, default `true`): whether this agent is allowed to run `git commit`
- `json_parser` (optional): how to parse the agent’s output (`claude`, `codex`, or `generic`)

Examples:

- Override the built-in `claude` agent command:

```toml
[agents.claude]
cmd = "claude -p"
json_flag = "--output-format=stream-json"
yolo_flag = "--dangerously-skip-permissions"
verbose_flag = "--verbose"
json_parser = "claude"
```

- Add an agent that prints plain text (no JSON parsing):

```toml
[agents.my_plain_agent]
cmd = "some-tool chat"
json_parser = "generic"
```

You can start from `examples/agents.toml` and copy it into `.agent/agents.toml`.

### How to use the config file

Ralph loads agents in this order:

1. Built-in defaults (`claude`, `codex`, `opencode`, `aider`)
2. Agents from `.agent/agents.toml` (or `RALPH_AGENTS_CONFIG`) override defaults by name
3. `CLAUDE_CMD` / `CODEX_CMD` (if set) override the command Ralph runs for those roles

Pick agents by name:

- `ralph --developer-agent claude --reviewer-agent codex`
- `ralph --developer-agent myagent --reviewer-agent my_plain_agent`

## Environment variables

### Agent selection and commands

- `RALPH_DEVELOPER_AGENT`: developer agent name (default `claude`)
- `RALPH_REVIEWER_AGENT`: reviewer agent name (default `codex`)
- `CLAUDE_CMD`: override the exact command used for the developer role (highest priority)
- `CODEX_CMD`: override the exact command used for the reviewer role (highest priority)
- `RALPH_AGENTS_CONFIG`: path to the agents TOML file (default `.agent/agents.toml`)

### Iterations and review passes

- `CLAUDE_ITERS`: number of developer iterations (default `5`)
- `CODEX_REVIEWS`: number of reviewer re-review passes after fixes (default `2`)

### Checks

- `FAST_CHECK_CMD`: shell command run after each developer iteration (non-blocking)
- `FULL_CHECK_CMD`: shell command run at the end (blocking; failure aborts)

Examples:

- `FAST_CHECK_CMD="cargo fmt -- --check && cargo clippy -D warnings"`
- `FULL_CHECK_CMD="cargo test"`

### Behavior and output

- `RALPH_INTERACTIVE`: `1` (default) keeps agents interactive; `0` avoids interactive affordances
- `RALPH_USE_PTY`: `1` to use a PTY for agent commands (default `0`)
- `RALPH_REVIEWER_COMMITS`: `1` (default) lets the reviewer create the final commit; `0` makes Ralph commit instead
- `RALPH_PROMPT_PATH`: where Ralph writes the last generated prompt (default `.agent/last_prompt.txt`)
- `RALPH_DEVELOPER_CONTEXT`: `0` minimal, `1` normal (default `1`)
- `RALPH_REVIEWER_CONTEXT`: `0` minimal/fresh eyes, `1` normal (default `0`)
- `RALPH_VERBOSITY`: `0..3` (default `1`)

## Files Ralph creates

Ralph uses a `.agent/` working directory for run state and logs:

- `.agent/logs/`: agent run logs (JSON lines or plain text)
- `.agent/STATUS.md`: high-level status tracking
- `.agent/NOTES.md`: scratchpad notes
- `.agent/ISSUES.md`: issues found / to address
- `.agent/archive/`: archives prior status/notes/issues when using “fresh eyes” reviewer context
- `.agent/last_prompt.txt`: last prompt generated and sent to an agent (configurable)

If you don’t want these tracked in git, add this to your repo’s `.gitignore`:

```gitignore
.agent/
```

## Troubleshooting

- **“Not a git repository”**: run Ralph inside a repo (or `cd` into the repo root).
- **Agent command not found**: ensure the CLI (`claude`, `codex`, etc.) is installed and on your `PATH`, or set `CLAUDE_CMD` / `CODEX_CMD`, or define a custom agent in `.agent/agents.toml`.
- **Garbled output / parsing errors**: set `json_parser = "generic"` for that agent in `.agent/agents.toml` to disable JSON parsing.
- **No commit created**: if the reviewer did not create a commit, Ralph falls back to `git add -A` + `git commit -m <msg>`.

## License

MIT. See `LICENSE`.

