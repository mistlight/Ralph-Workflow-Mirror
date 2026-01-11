# Ralph

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**Ralph automates your AI coding workflow.** Write what you want in plain English, and Ralph coordinates AI agents to build it, review it, and commit it - all hands-free.

Inspired by [Geoffrey Huntley's Ralph concept](https://ghuntley.com/ralph/).

## How It Works

```
You write PROMPT.md     Ralph runs AI agents      You get working code
describing what         to implement and          committed to your
you want built          review the changes        git repository
       |                       |                        |
       v                       v                        v
   "Add a dark            Developer Agent         git commit -m
    mode toggle"          writes the code         "feat: add dark
                               |                  mode toggle"
                               v
                          Reviewer Agent
                          checks quality
                          and fixes issues
```

**That's it.** You describe your goal, Ralph does the coding.

## Who Is This For?

- **Vibecoding enthusiasts** - Let AI write your code while you focus on ideas
- **Developers** - Automate repetitive implementation tasks
- **Teams** - Consistent AI-assisted development with built-in code review
- **Anyone** building software with AI assistants (Claude, Codex, Aider, OpenCode, Goose, and more)

## Quick Start

### 1. Install Ralph

```bash
# Option A: From source (requires Rust)
git clone https://codeberg.org/mistlight/RalphWithReviewer.git
cd RalphWithReviewer
cargo install --path .

# Option B: Using Makefile
git clone https://codeberg.org/mistlight/RalphWithReviewer.git
cd RalphWithReviewer
make install-local
```

### 2. Install AI Agents

Ralph needs AI coding tools to do the actual work. Install at least one:

| Agent | Install | Notes |
|-------|---------|-------|
| **Claude Code** | `npm install -g @anthropic/claude-code` | Recommended developer agent |
| **Codex** | `npm install -g @openai/codex` | Recommended reviewer agent |
| **OpenCode** | See [opencode.ai](https://opencode.ai) | Works for both roles |
| **Aider** | `pip install aider-chat` | Popular alternative |

After installing, make sure you've authenticated with your chosen agent (e.g., `claude auth` or set API keys).

### 3. Run Ralph

Navigate to any git repository and:

```bash
# Create a file describing what you want
echo "Add a button that says Hello World" > PROMPT.md

# Run Ralph
ralph
```

Ralph will:
1. Create working files in `.agent/` (if they don't exist)
2. Run the developer agent to implement your prompt
3. Run the reviewer agent to check and fix issues
4. Generate a commit message and commit the changes

## Basic Usage

### The PROMPT.md File

This is where you describe what you want built. Write in plain English:

```markdown
# What I Want

Add a dark mode toggle to the settings page.

## Requirements
- Toggle switch in the top-right corner
- Save preference to localStorage
- Apply dark theme immediately when toggled
```

### Running Ralph

```bash
# Basic usage - uses agents configured in agent_chain
ralph

# Quick mode for rapid prototyping (1 dev iteration + 1 review)
ralph --quick

# Control how many times agents iterate
ralph --developer-iters 3 --reviewer-reviews 1

# See what's happening in detail
ralph --full
```

### Checking What Agents Are Available

```bash
# List all configured agents
ralph --list-agents

# List only agents you have installed
ralph --list-available-agents
```

## Configuration

### First Run Setup

On first run, Ralph creates a `.agent/` folder in your repo with:
- `agents.toml` - Agent configuration (edit to customize)
- `STATUS.md`, `NOTES.md`, `ISSUES.md` - Working files for agents
- `logs/` - Agent output logs

### Choosing Agents

Ralph uses two agents with different roles:

| Role | What It Does | Default |
|------|--------------|---------|
| **Developer** | Writes and implements code | First agent in `agent_chain.developer` |
| **Reviewer** | Reviews code and suggests fixes | First agent in `agent_chain.reviewer` |

The default agent chains are configured in `.agent/agents.toml`. Edit this file to use your preferred agents.

Change agents via command line:
```bash
ralph --developer-agent aider --reviewer-agent opencode
```

Or via environment variables:
```bash
export RALPH_DEVELOPER_AGENT=aider
export RALPH_REVIEWER_AGENT=opencode
```

### Agent Configuration File

Edit `.agent/agents.toml` to customize agents or add new ones:

```toml
# Override an existing agent
[agents.claude]
cmd = "claude -p"
output_flag = "--output-format=stream-json"
yolo_flag = "--dangerously-skip-permissions"

# Add a custom agent
[agents.myagent]
cmd = "my-ai-tool run"
json_parser = "generic"  # Use "generic" if agent doesn't output JSON
```

Create a global config that applies to all your repos:
```bash
ralph --init-global
# Creates ~/.config/ralph/agents.toml
```

## Environment Variables

Quick reference for the most common settings:

| Variable | What It Does | Default |
|----------|--------------|---------|
| `RALPH_DEVELOPER_AGENT` | Which agent writes code | From `agent_chain` |
| `RALPH_REVIEWER_AGENT` | Which agent reviews code | From `agent_chain` |
| `RALPH_DEVELOPER_ITERS` | How many times developer runs | `5` |
| `RALPH_REVIEWER_REVIEWS` | How many review passes | `2` |
| `RALPH_VERBOSITY` | Output detail (0-4) | `2` |
| `FAST_CHECK_CMD` | Run after each iteration | - |
| `FULL_CHECK_CMD` | Run at the end (e.g., tests) | - |

Example with checks:
```bash
export FAST_CHECK_CMD="npm run lint"
export FULL_CHECK_CMD="npm test"
ralph
```

## Advanced Features

### Automatic Fallback

If an agent hits rate limits or errors, Ralph automatically switches to the next agent in the chain. Configure fallback chains in `.agent/agents.toml`:

```toml
[agent_chain]
developer = ["claude", "codex", "aider"]  # Try in order
reviewer = ["codex", "claude"]
max_retries = 3
```

### Verbosity Levels

Control how much output you see:

| Level | Flag | What You See |
|-------|------|--------------|
| 0 | `-q` or `--quiet` | Minimal - just results |
| 1 | `-v1` | Normal output |
| 2 | `-v2` | Verbose (default) |
| 3 | `--full` | Everything, no truncation |
| 4 | `--debug` | Raw JSON, maximum detail |

### Plumbing Commands

For scripting and CI/CD:

```bash
# Generate commit message without committing
ralph --generate-commit-msg

# Show the generated message
ralph --show-commit-msg

# Apply commit using generated message
ralph --apply-commit
```

## Files Ralph Creates

All working files live in `.agent/`:

```
.agent/
├── agents.toml        # Agent configuration
├── STATUS.md          # Current status
├── NOTES.md           # Agent notes/scratchpad
├── ISSUES.md          # Issues found
├── commit-message.txt # Generated commit message
├── last_prompt.txt    # Last prompt sent to agent
└── logs/              # Agent run logs
```

Add to `.gitignore` if you don't want these tracked:
```
.agent/
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Not a git repository" | Run Ralph inside a git repo |
| "Agent not found" | Install the agent CLI and ensure it's on your PATH. Ralph shows installation hints. |
| Garbled/broken output | Set `json_parser = "generic"` for that agent |
| Rate limit errors | Ralph auto-retries with backoff. Configure fallback agents for faster recovery. |
| Network/connection errors | Check internet, firewall, VPN. Ralph auto-retries network issues. |
| Authentication errors | Run `<agent> auth` to authenticate, or check your API key. |
| No commit created | Ralph falls back to `git commit` if the reviewer doesn't |
| Nothing happening | Try `ralph --debug` to see what's going on |

## Common Workflows

### Quick Prototyping
```bash
# Use quick mode for fast iteration
echo "Add a logout button to the navbar" > PROMPT.md
ralph --quick
```

### Full Feature with Review
```bash
# Full pipeline: multiple dev iterations + thorough review
echo "Implement user authentication with JWT" > PROMPT.md
ralph --developer-iters 5 --reviewer-reviews 2
```

### Bug Fix with Tests
```bash
cat > PROMPT.md << 'EOF'
Fix the login bug where users get logged out after 5 minutes.
Add a test to prevent regression.
EOF
FULL_CHECK_CMD="npm test" ralph
```

### Iterative Development
```bash
# Start with a rough prompt
echo "Build a todo app" > PROMPT.md
ralph --quick

# Refine with more thorough review
echo "Add due dates and priority levels to the todo app" > PROMPT.md
ralph
```

## FAQ

### Can I use Ralph at work / in my Fortune 500 company?

**Yes, absolutely.** Ralph is a CLI tool you run locally. Using it doesn't affect the license of your code in any way.

### Does the AGPL license apply to code I generate with Ralph?

**No.** The AGPL-3.0 license covers *only the Ralph tool itself* — the Rust source code in this repository. It does **not** apply to:

- Code generated by AI agents that Ralph orchestrates
- Your PROMPT.md files
- Your project's source code
- Any output, commits, or artifacts Ralph creates in your repository

The code you create with Ralph is entirely yours, under whatever license you choose.

### Common AGPL Misconceptions for CLI Tools

| Misconception | Reality |
|---------------|---------|
| "Using an AGPL tool makes my code AGPL" | ❌ False. AGPL covers the tool, not its output. Using `gcc` (GPL) doesn't make your C code GPL. Same principle. |
| "I can't use AGPL tools in a corporate environment" | ❌ False. You can use Ralph freely. You only need to share source if you *modify and distribute Ralph itself*. |
| "AI-generated code inherits Ralph's license" | ❌ False. The AI agents (Claude, Codex, etc.) generate the code, not Ralph. Ralph just orchestrates. |
| "My company's legal team will reject this" | Show them this FAQ! Ralph is a local dev tool like `make` or `git`. |

### What would require me to share source code?

Only if you **modify Ralph itself** and **distribute your modified version** (or provide it as a network service). Normal usage — running Ralph to build your projects — requires nothing from you.

### Is there a commercial/enterprise license available?

For now, no. The AGPL is the only license. But again, you can freely use Ralph in any commercial setting without concern. If you need a different license for redistribution purposes, open an issue.

### TL;DR

**Use Ralph anywhere. Your code stays yours. The AGPL only covers Ralph's source code, not anything you create with it.**

## Contributing

Contributions welcome!

1. Fork the repository
2. Create a feature branch
3. Run tests: `cargo test`
4. Run lints: `cargo clippy && cargo fmt --check`
5. Submit a pull request

## License

AGPL-3.0. See [LICENSE](LICENSE).
