# Ralph Workflow

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**Ralph Workflow is an unattended AI agent orchestrator for long-running development tasks.** Write a detailed specification in `PROMPT.md`, start Ralph, and walk away. It coordinates AI agents through multiple development iterations and review cycles, producing commits automatically.

Ralph works best when you think like a Product Manager: scope out every detail of the feature you need. The more detail in your specification, the better Ralph performs. It is designed to run for hours without babysitting.

Inspired by [Geoffrey Huntley's Ralph Workflow concept](https://ghuntley.com/ralph/).

## When to Use Ralph

**Ralph excels at:**
- Long-running feature implementations with detailed specifications
- Systematic refactoring workflows requiring multiple iterations
- Test suite generation with comprehensive review
- Documentation writing with multiple review passes
- Any task where you can write a detailed spec and let it run unattended

**Not ideal for:**
- Vague or undefined requirements (Ralph needs detailed specs)
- Simple one-off commands (use Claude Code directly)
- Real-time interactive debugging
- Tasks requiring human judgment at each step

## How It Works

Ralph runs a multi-phase workflow:

1. **Developer Phase**: AI agent implements your spec through multiple iterations
   - Creates `PLAN.md` from your `PROMPT.md`
   - Executes the plan and makes code changes
   - Auto-commits after each iteration
   - Cleans up and repeats for configured iterations

2. **Review Phase**: AI reviewer checks quality and fixes issues
   - Reviews code and creates `ISSUES.md` with problems found
   - Developer agent fixes the issues
   - Repeats until no issues or max cycles reached

3. **Final Commit**: Generates a meaningful commit message via AI

All orchestration files (PLAN.md, ISSUES.md) are controlled by Ralph, not the AI agents. This ensures deterministic, reliable operation.

## Design Philosophy

Ralph makes **deterministic decisions whenever possible**, only calling on AI when needed:

- **Conflict Resolution**: Prompts AI specifically about conflicts, then resolves automatically
- **File I/O**: The orchestrator controls all file writes, not the agents
- **Git Operations**: Ralph handles rebasing, committing, and status checks deterministically
- **Checkpoint/Resume**: Saves state after each phase; interrupted runs can resume with `--resume`

## Quick Start

### 1. Install

```bash
git clone https://codeberg.org/mistlight/RalphWithReviewer.git
cd RalphWithReviewer
cargo install --path .
make install # (if you want to install this system wide)
```

Alternatively you can use cargo crate
```bash
cargo install ralph-workflow
```

### 2. Install AI Agents

Install at least one AI agent:

| Agent | Install | Recommended Role |
|-------|---------|------------------|
| **Claude Code** | `npm install -g @anthropic/claude-code` | Developer |
| **Codex** | `npm install -g @openai/codex` | Reviewer |
| **OpenCode** | See [opencode.ai](https://opencode.ai) | Either |

### 3. Run Ralph

```bash
# Create config file
ralph --init

# Navigate to your git repo
cd /path/to/your/project

# Create PROMPT.md from a template
ralph --init feature-spec
# Edit PROMPT.md with detailed requirements

# Run Ralph and walk away
ralph
```

## Writing Effective Specifications

Your `PROMPT.md` should be detailed. Example:

```markdown
# Task: Refactor Auth Module

## Description
Refactor the authentication module to use OAuth2 instead of basic auth.

## Requirements
1. Use passport-oauth2 library
2. Support GitHub and Google providers
3. Maintain backward compatibility with API keys
4. Add comprehensive tests

## Files to Update
- src/auth/mod.rs
- src/auth/oauth.rs (new)
- tests/auth_test.rs

## Constraints
- No breaking changes to public API
- All existing tests must pass
```

## Common Commands

### Preset Modes (control thoroughness)

```bash
ralph -Q "fix: small bug"              # Quick: 1 dev + 1 review
ralph -U "feat: minor change"          # Rapid: 2 dev + 1 review
ralph -S "feat: add feature"           # Standard: 5 dev + 2 reviews (default)
ralph -T "refactor: optimize"          # Thorough: 10 dev + 5 reviews
ralph -L "feat: complex feature"       # Long: 15 dev + 10 reviews
```

### Custom Iterations

```bash
ralph -D 3 -R 2 "feat: implement feature"  # 3 dev iterations, 2 review cycles
ralph -D 10 -R 0 "feat: no review"         # Skip review phase entirely
```

### Choose Agents

```bash
ralph -a claude -r codex "feat: change"    # Claude for dev, Codex for review
ralph -a opencode "feat: change"           # Use OpenCode for development
```

### Verbosity Control

```bash
ralph -q "fix: typo"                   # Quiet mode
ralph -f "feat: complex change"        # Full output (no truncation)
ralph -d                               # Diagnose: show system info
```

### Recovery

```bash
ralph --resume                         # Resume from last checkpoint
ralph --dry-run                        # Validate setup without running
```

## Configuration

Ralph uses `~/.config/ralph-workflow.toml`:

```bash
ralph --init   # Creates config if missing
```

Configure agent chains and defaults:

```toml
[general]
developer_iters = 5
reviewer_reviews = 2

[agent_chain]
developer = ["claude", "codex", "opencode"]
reviewer = ["codex", "claude"]
max_retries = 3
```

Environment variables override config:
- `RALPH_DEVELOPER_AGENT` - Developer agent
- `RALPH_REVIEWER_AGENT` - Reviewer agent
- `RALPH_DEVELOPER_ITERS` - Developer iterations
- `RALPH_REVIEWER_REVIEWS` - Review cycles
- `RALPH_VERBOSITY` - Output detail (0-4)

## Files Created by Ralph

```
.agent/
├── PLAN.md            # Current iteration plan (orchestrator-written)
├── ISSUES.md          # Review findings (orchestrator-written)
├── STATUS.md          # Current status
├── commit-message.txt # Generated commit message
├── checkpoint.json    # For --resume
├── start_commit       # Baseline for diffs
└── logs/              # Detailed per-phase logs
```

## Documentation

- **[Quick Reference](docs/quick-reference.md)** - Cheat sheet for commands and flags
- **[Agent Compatibility](docs/agent-compatibility.md)** - Supported AI agents
- **[Git Workflow](docs/git-workflow.md)** - How Ralph handles commits and diffs
- **[Template Guide](docs/template-guide.md)** - PROMPT.md templates

## FAQ

**Can I use Ralph at work?**

Yes. Ralph is a local CLI tool. The AGPL license covers only the Ralph source code, not anything you create with it.

**Does AGPL apply to my generated code?**

No. The AGPL covers only Ralph itself, not your code or Ralph's output.

**What if Ralph gets interrupted?**

Use `ralph --resume` to continue from the last checkpoint.

## Contributing

Contributions welcome!

1. Fork the repository
2. Create a feature branch
3. Run tests: `cargo test`
4. Run lints: `cargo clippy && cargo fmt --check`
5. Submit a pull request

## License

AGPL-3.0. See [LICENSE](LICENSE).
