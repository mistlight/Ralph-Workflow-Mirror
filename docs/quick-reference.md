# Ralph Workflow - Quick Reference

A cheat sheet for common Ralph Workflow commands and flags.

## Beginner's Guide

New to Ralph or command-line tools? Start here.

**What is Ralph?**
Ralph is an AI-powered coding assistant that helps you implement features, fix bugs, and refactor code. It reads a file called `PROMPT.md` to understand what you want to do, then uses AI agents to write and review the code.

**Basic workflow:**
1. Create a `PROMPT.md` file describing what you want
2. Run `ralph` in your project directory
3. Ralph handles the rest and creates a git commit

**Your first Ralph command:**
```bash
# Create a prompt template for a new feature
ralph --init-prompt feature-spec

# Edit PROMPT.md with your requirements
# Then run ralph to implement it
ralph
```

**Choosing a preset mode:**
- Use `-Q` (quick) for small changes like typos or simple fixes
- Use `-U` (rapid) for minor bugs or small features
- Use `-S` (standard) for most features (this is the default)
- Use `-T` (thorough) for complex or important changes
- Use `-L` (long) only for critical features needing maximum review

**Common scenarios:**
```bash
# Fix a small bug quickly
ralph -Q "fix: typo in header"

# Add a new feature
ralph --init-prompt feature-spec
# Edit PROMPT.md, then:
ralph

# Just want more control over iterations
ralph -D 3 -R 2 "feat: add user settings"
```

## Short Flags Reference

| Short Flag | Long Form | Description |
|-----------|-----------|-------------|
| `-D N` | `--developer-iters N` | Number of developer iterations |
| `-R N` | `--reviewer-reviews N` | Number of review cycles |
| `-a AGENT` | `--developer-agent AGENT` | Developer agent |
| `-r AGENT` | `--reviewer-agent AGENT` | Reviewer agent |
| `-d` | `--diagnose` | Show diagnostic info |
| `-f` | `--full` | Full output mode (no truncation) |
| `-L` | `--long` | Long preset mode (15 dev + 10 reviews) |
| `-Q` | `--quick` | Quick preset mode (1 dev + 1 review) |
| `-S` | `--standard` | Standard preset mode (5 dev + 2 reviews) |
| `-T` | `--thorough` | Thorough preset mode (10 dev + 5 reviews) |
| `-U` | `--rapid` | Rapid preset mode (2 dev + 1 review) |
| `-q` | `--quiet` | Quiet mode (minimal output) |
| `-v N` | `--verbosity N` | Verbosity level (0-4) |
| `-c PATH` | `--config PATH` | Path to config file |
| `-i` | `--interactive` | Prompt for PROMPT.md template if missing |

## Help Commands

```bash
ralph --help           # Show basic help (quick start, common flags)
ralph --help-advanced  # Show comprehensive help (all options, templates, docs)
ralph --list-templates # Show available PROMPT.md templates
```

## Preset Modes

| Mode | Flag | Dev Iters | Reviews | Use Case |
|------|------|-----------|---------|----------|
| Quick | `-Q` | 1 | 1 | Rapid prototyping |
| Rapid | `-U` | 2 | 1 | Fast iteration |
| Standard | `-S` | 5 | 2 | Default workflow |
| Thorough | `-T` | 10 | 5 | Balanced but thorough |
| Long | `-L` | 15 | 10 | Most thorough |

## Common Command Patterns

### Basic Usage
```bash
# Default workflow
ralph "feat: implement feature"

# With custom commit message
ralph "feat: add user authentication"
```

### Preset Modes
```bash
ralph -Q "fix: small bug"              # Quick (1+1)
ralph -U "fix: minor bug"              # Rapid (2+1)
ralph -S "feat: normal change"         # Standard (5+2)
ralph -T "refactor: optimize"          # Thorough (10+5)
ralph -L "feat: complex feature"       # Long (15+10)
```

### Custom Iterations
```bash
ralph -D 3 -R 2 "fix: bug"
ralph -D 10 -R 5 "refactor: module"
```

### Specific Agents
```bash
# Use specific agents
ralph -a claude -r codex "feat: change"

# Use preset
ralph --preset opencode "feat: change"
```

### Verbosity Control
```bash
ralph -q "fix: typo"                   # Quiet
ralph -v1 "feat: change"               # Normal
ralph -v2 "feat: change"               # Verbose (default)
ralph -f "feat: complex change"        # Full
ralph --debug "feat: change"           # Debug with raw JSON
```

### Diagnostic Commands
```bash
ralph --diagnose                # Show system info and config
ralph --list-agents             # List all configured agents
ralph --list-available-agents   # List installed agents only
ralph --list-providers          # List OpenCode providers
ralph --dry-run                 # Validate without running
```

### Workflow Management
```bash
ralph --resume                  # Resume from checkpoint
ralph --no-isolation            # Keep NOTES.md/ISSUES.md
ralph --reset-start-commit      # Reset diff baseline
```

### PROMPT.md Templates
```bash
ralph --list-templates          # Show available templates
ralph --init-prompt feature-spec
ralph --init-prompt bug-fix
ralph --init-prompt refactor
ralph --interactive             # Prompt if PROMPT.md missing
```

### Configuration
```bash
ralph --init-global             # Create ~/.config/ralph-workflow.toml
ralph --init-legacy             # Create .agent/agents.toml (legacy)
```

## Review Depth Levels

| Level | Description |
|-------|-------------|
| `standard` | Balanced review (default) |
| `comprehensive` | Thorough review |
| `security` | OWASP-focused security review |
| `incremental` | Changed files only |

```bash
ralph --review-depth security "feat: auth"
ralph --review-depth incremental "fix: bug"
```

## Verbosity Levels

| Level | Flag | Output |
|-------|------|--------|
| 0 | `-q` | Minimal |
| 1 | `-v1` | Normal |
| 2 | (default) | Verbose |
| 3 | `-f`, `--full` | Everything |
| 4 | `--debug` | Raw JSON |

## Plumbing Commands (Scripting)

```bash
ralph --generate-commit-msg    # Generate message only
ralph --show-commit-msg        # Display generated message
ralph --apply-commit           # Commit using generated message
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RALPH_DEVELOPER_AGENT` | From config | Developer agent |
| `RALPH_REVIEWER_AGENT` | From config | Reviewer agent |
| `RALPH_DEVELOPER_ITERS` | `5` | Developer iterations |
| `RALPH_REVIEWER_REVIEWS` | `2` | Review cycles |
| `RALPH_VERBOSITY` | `2` | Verbosity (0-4) |
| `RALPH_ISOLATION_MODE` | `1` | Isolation on/off |

## Streaming Configuration

Ralph's streaming system handles real-time AI agent output. These environment variables control streaming behavior:

| Variable | Default | Range | Description |
|----------|---------|-------|-------------|
| `RALPH_STREAMING_SNAPSHOT_THRESHOLD` | `200` | 50-1000 | Max delta size before warning (chars) |
| `RALPH_STREAMING_FUZZY_MATCH_RATIO` | `85` | 50-95 | Fuzzy detection threshold (%) |

## CLI Flags

| Flag | Description |
|------|-------------|
| `--show-streaming-metrics` | Display streaming quality metrics after agent completion |

## Files Created by Ralph

```
.agent/
├── STATUS.md          # Current status
├── NOTES.md           # Agent notes
├── ISSUES.md          # Issues found during review
├── PLAN.md            # Current iteration plan
├── commit-message.txt # Generated commit message
├── checkpoint.json    # For --resume
├── last_prompt.txt    # Last prompt sent
├── start_commit       # Baseline for reviewer diffs
└── logs/              # Agent run logs
```

## Quick Setup

```bash
# 1. Install Ralph
git clone https://codeberg.org/mistlight/RalphWithReviewer.git
cd RalphWithReviewer
cargo install --path .

# 2. Create config
ralph --init-global

# 3. Install AI agents (choose one)
npm install -g @anthropic/claude-code  # Claude Code
npm install -g @openai/codex           # Codex
# OR see opencode.ai for OpenCode

# 4. Run Ralph
cd /path/to/your/project
ralph --init-prompt feature-spec
# Edit PROMPT.md with your requirements
ralph
```

## Common Workflows

### Quick Prototyping
```bash
echo "Add logout button" > PROMPT.md
ralph -Q
```

### Full Feature with Review
```bash
echo "Implement JWT auth" > PROMPT.md
ralph -D 5 -R 2
```

### Bug Fix with Tests
```bash
cat > PROMPT.md << 'EOF'
Fix login timeout bug.
Add regression test.
EOF
FULL_CHECK_CMD="npm test" ralph
```

### Iterative Development
```bash
# Start with quick mode
echo "Build todo app" > PROMPT.md
ralph -Q

# Refine with thorough mode
echo "Add due dates" > PROMPT.md
ralph -T
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Not a git repository" | Run inside a git repo |
| "Agent not found" | Install agent CLI, ensure it's on PATH |
| Garbled output | Set `json_parser = "generic"` in config |
| Rate limits | Configure fallback agents in config |
| Auth errors | Run agent's auth command or set API key |
| No commit created | Ensure there are meaningful changes |

For more help, run `ralph --diagnose` or see the [full documentation](README.md#documentation).
