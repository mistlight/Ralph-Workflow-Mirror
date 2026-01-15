# Ralph Workflow

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**Ralph Workflow automates your AI coding workflow.** Write what you want in plain English, and Ralph coordinates AI agents to build it, review it, and commit it - all hands-free.

Inspired by [Geoffrey Huntley's Ralph Workflow concept](https://ghuntley.com/ralph/).

## What It Does

Ralph Workflow reads your requirements from `PROMPT.md`, runs a developer AI agent to implement the changes, then runs a reviewer agent to check quality and fix issues. It automatically generates meaningful git commits throughout the process. Just describe what you want, and Ralph handles the rest.

## Quick Start

### 1. Install Ralph Workflow

```bash
git clone https://codeberg.org/mistlight/RalphWithReviewer.git
cd RalphWithReviewer
cargo install --path .
```

### 2. Install AI Agents

Install at least one AI agent:

| Agent | Install | Role |
|-------|---------|------|
| **Claude Code** | `npm install -g @anthropic/claude-code` | Developer/Reviewer |
| **Codex** | `npm install -g @openai/codex` | Reviewer |
| **OpenCode** | See [opencode.ai](https://opencode.ai) | Both |

### 3. Run Ralph Workflow

```bash
# Create config file
ralph --init-global

# Navigate to your git repo
cd /path/to/your/project

# Create PROMPT.md
ralph --init-prompt feature-spec
# Edit PROMPT.md with your requirements

# Run Ralph
ralph
```

Ralph will:
1. Run the developer agent to implement your prompt
2. Run the reviewer agent to check and fix issues
3. Generate a commit message and commit the changes

## Common Commands

### Preset Modes

```bash
ralph -Q "fix: small bug"              # Quick mode (1 dev + 1 review)
ralph -S "feat: add feature"           # Standard mode (5 dev + 2 reviews)
ralph -T "refactor: optimize"          # Thorough mode (10 dev + 5 reviews)
ralph -L "feat: complex feature"       # Long mode (15 dev + 10 reviews)
```

### Custom Iterations

```bash
ralph -D 3 -R 2 "feat: implement feature"
```

### Choosing Agents

```bash
ralph -a claude -r codex "feat: change"
ralph --preset opencode "feat: change"
```

### Verbosity Control

```bash
ralph -q "fix: typo"                   # Quiet mode
ralph -f "feat: complex change"        # Full output
ralph --diagnose                       # Show diagnostic info
```

## Configuration

Ralph uses `~/.config/ralph-workflow.toml` for configuration:

```bash
ralph --init-global
```

Configure agents in the config file:

```toml
[agent_chain]
developer = ["claude", "codex", "aider"]
reviewer = ["codex", "claude"]
max_retries = 3
```

Environment variables override config settings:
- `RALPH_DEVELOPER_AGENT` - Which agent writes code
- `RALPH_REVIEWER_AGENT` - Which agent reviews code
- `RALPH_DEVELOPER_ITERS` - Developer iterations (default: 5)
- `RALPH_REVIEWER_REVIEWS` - Review cycles (default: 2)
- `RALPH_VERBOSITY` - Output detail (0-4)

## Documentation

- **[Quick Reference](docs/quick-reference.md)** - Cheat sheet for all commands and flags
- **[Getting Started Guide](docs/getting-started.md)** - Detailed installation and first steps
- **[Configuration Guide](docs/configuration.md)** - All configuration options
- **[Agent Setup](docs/agents.md)** - Agent installation and provider configuration
- **[Advanced Features](docs/advanced.md)** - Checkpoint, metrics, isolation, review depth
- **[Presets Guide](docs/presets.md)** - Detailed preset documentation
- **[Workflows](docs/workflows.md)** - Common workflow examples
- **[Troubleshooting](docs/troubleshooting.md)** - Common issues and solutions
- **[Agent Compatibility](docs/agent-compatibility.md)** - Supported AI agents and configurations
- **[Git Workflow](docs/git-workflow.md)** - How Ralph handles commits and diffs

## FAQ

**Can I use Ralph Workflow at work/in my company?**

Yes! Ralph is a local CLI tool. The AGPL license covers only the Ralph source code, not anything you create with it. Your code stays yours.

**Does the AGPL license apply to code I generate?**

No. The AGPL covers only the Ralph Workflow tool itself, not your code or any output Ralph creates.

## Contributing

Contributions welcome!

1. Fork the repository
2. Create a feature branch
3. Run tests: `cargo test`
4. Run lints: `cargo clippy && cargo fmt --check`
5. Submit a pull request

## License

AGPL-3.0. See [LICENSE](LICENSE).
