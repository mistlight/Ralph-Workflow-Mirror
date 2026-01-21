# Ralph Workflow Website - Design Specification

**Document Type**: Design Specification  
**Status**: Draft  
**Created**: 2025-01-21

---

## Abstract

A comprehensive plan for building a Next.js TypeScript website for Ralph Workflow, serving as both marketing landing page and user documentation. Features include dynamic OpenCode model browsing via models.dev API, interactive configuration helpers, and detailed guides for the three supported CLI agents plus CCS profiles.

---

## 1. Technology Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Framework | Next.js 15 (App Router) | SSR, SSG, excellent DX |
| Language | TypeScript | Type safety, matches codebase quality standards |
| Styling | Tailwind CSS + shadcn/ui | Rapid development, accessibility |
| Animation | Framer Motion | Smooth transitions |
| Documentation | MDX | Rich docs with React components |
| Deployment | Vercel | Native Next.js integration |
| Package Manager | pnpm | Fast, disk-efficient |

---

## 2. Critical Clarification: Model Configuration by Agent

> **IMPORTANT**: The models.dev API (`https://models.dev/api.json`) is **ONLY applicable to OpenCode**. Each agent has its own distinct model configuration approach.

### Agent Model Configuration Matrix

| Agent | Model Source | Configuration Method | Source Reference |
|-------|--------------|---------------------|------------------|
| **OpenCode** | models.dev API | `-m provider/model` flag | `ralph-workflow/src/agents/opencode_api/` |
| **Claude Code** | Anthropic (built-in) | `--model` flag or default | Claude CLI documentation |
| **Codex CLI** | OpenAI (built-in) | Codex CLI options | Codex CLI documentation |
| **CCS Profiles** | Per-profile config | `~/.ccs/` config files | `ralph-workflow/src/agents/ccs.rs` |

### OpenCode + models.dev

From `ralph-workflow/src/agents/opencode_api/mod.rs`:
```rust
pub const API_URL: &str = "https://models.dev/api.json";
```

OpenCode supports 75+ providers through the AI SDK and models.dev catalog. Users select models via the `-m` flag:
```bash
opencode -m opencode/glm-4.7-free
opencode -m anthropic/claude-sonnet-4
opencode -m openai/gpt-4o
```

### Claude Code

Claude Code uses Anthropic's models directly. No external catalog required:
```bash
claude --model claude-sonnet-4
claude --model claude-opus-4
```

### Codex CLI

Codex CLI uses OpenAI's models directly. No external catalog required:
```bash
codex --model gpt-4o
```

### CCS (Claude Code Switch)

From `ralph-workflow/src/agents/ccs.rs`:

CCS manages multiple Claude profiles with different providers. Configuration is stored in:
- `~/.ccs/config.json` or `~/.ccs/config.yaml` - Profile definitions
- `~/.ccs/{profile}.settings.json` - Per-profile credentials

Usage syntax in Ralph:
```bash
ralph --developer-agent ccs/glm
ralph --developer-agent ccs/gemini
ralph --developer-agent ccs/work
```

**CCS does NOT use models.dev** - it has its own credential management system.

---

## 3. Site Architecture

### Information Architecture

```
ralph.dev/
├── (marketing)
│   ├── /                      # Landing page / hero
│   ├── /features              # Feature showcase
│   └── /use-cases             # Case studies and examples
│
├── (documentation)
│   ├── /docs                  # Documentation hub
│   ├── /docs/getting-started  # Quick start guide
│   ├── /docs/installation     # Installation instructions
│   ├── /docs/configuration    # Config file reference
│   ├── /docs/agents           # Agent overview + compatibility matrix
│   │   ├── /docs/agents/claude    # Claude Code guide
│   │   ├── /docs/agents/codex     # Codex CLI guide
│   │   ├── /docs/agents/opencode  # OpenCode guide (links to model browser)
│   │   └── /docs/agents/ccs       # CCS profiles guide (separate from models.dev)
│   ├── /docs/workflow         # Workflow phases explained
│   │   ├── /docs/workflow/planning
│   │   ├── /docs/workflow/development
│   │   ├── /docs/workflow/review
│   │   └── /docs/workflow/commit
│   ├── /docs/unattended       # Unattended operation guide
│   ├── /docs/prompt-guide     # Writing effective PROMPT.md
│   └── /docs/troubleshooting  # Common issues and solutions
│
├── (tools)
│   ├── /opencode-models       # OpenCode Model Browser (models.dev) - OPENCODE ONLY
│   ├── /config-builder        # Visual config generator
│   └── /prompt-templates      # PROMPT.md template gallery
│
└── (api)
    └── /api/opencode-models   # Proxy for models.dev with caching
```

---

## 4. Page Specifications

### 4.1 Landing Page (`/`)

**Purpose**: Convert visitors into users

**Sections**:

1. **Hero Section**
   - Headline: "Unattended AI Development Orchestration"
   - Subheadline: "Let Ralph manage your AI coding agents while you sleep"
   - CTA: "Get Started" / "View Documentation"
   - Animated terminal demo showing Ralph in action

2. **Problem Statement**
   - "AI agents require babysitting"
   - "Context switching kills productivity"  
   - "Long tasks need checkpoint/resume"

3. **Solution Overview**
   - PROMPT.md driven development
   - Multi-agent fallback chains
   - Automatic review cycles
   - Deterministic conflict resolution

4. **Supported Agents** (with clear distinction)
   ```
   ┌─────────────────────────────────────────────────────────┐
   │  SUPPORTED AI CODING AGENTS                            │
   ├─────────────────────────────────────────────────────────┤
   │  Claude Code     │  Codex CLI      │  OpenCode         │
   │  Anthropic       │  OpenAI         │  75+ providers    │
   │                  │                 │  via models.dev   │
   ├─────────────────────────────────────────────────────────┤
   │  + CCS Profiles (Claude Code Switch)                   │
   │  Multiple accounts: work, personal, gemini, glm, etc.  │
   └─────────────────────────────────────────────────────────┘
   ```

5. **Feature Highlights** (linked cards)
   - Unattended Operation
   - Multi-Agent Fallback
   - Checkpoint/Resume
   - Automatic Code Review
   - Conventional Commits

6. **Quick Start**
   ```bash
   # Install Ralph
   cargo install ralph-workflow

   # Initialize in your project (smart init - infers what you need)
   ralph --init

   # Write your specification
   vim PROMPT.md

   # Run unattended (commit message is optional - defaults to "chore: apply PROMPT loop + review/fix/review")
   ralph
   # OR with custom commit message:
   ralph "feat: implement user authentication"

   # Go to sleep. Wake up to finished code.
   ```

   **Recommended: Git Worktree Workflow**

   Ralph is designed for unattended, long-running tasks. Using Git worktrees lets you run multiple Ralph sessions in parallel without blocking your main development workflow:

   ```bash
   # Create worktrees for parallel features
   git worktree add ../feature-auth feature/auth
   git worktree add ../feature-api feature/api

   # Run Ralph in each worktree simultaneously
   cd ../feature-auth && ralph "implement user auth"
   cd ../feature-api && ralph "implement REST API"
   ```

---

### 4.2 Features Page (`/features`)

| Feature | Description | Source Reference |
|---------|-------------|------------------|
| Unattended Operation | Run overnight without interaction | `src/phases/development.rs` |
| Agent Fallback Chains | Auto-switch agents on failure | `src/agents/fallback.rs` |
| PROMPT.md Workflow | Product manager thinking | `src/prompts/developer.rs` |
| Review Cycles | Automatic code review | `src/phases/review.rs` |
| Checkpoint/Resume | Recover from interruptions | `src/checkpoint/` |
| CCS Integration | Multiple Claude profiles | `src/agents/ccs.rs` |
| OpenCode + models.dev | Dynamic model discovery | `src/agents/opencode_api/` |
| Commit Generation | Conventional Commits auto | `src/prompts/commit.rs` |

---

### 4.3 Agent Documentation (`/docs/agents/*`)

#### 4.3.1 Claude Code (`/docs/agents/claude`)

**Source Reference**: `ralph-workflow/src/agents/registry.rs` (built-in agent definitions)

**Content**:
- Installation: Link to Anthropic's Claude Code installation
- Configuration in `ralph-workflow.toml`:
  ```toml
  [agents.claude]
  cmd = "claude"
  output_flag = "--output-format=stream-json"
  yolo_flag = "--dangerously-skip-permissions"
  verbose_flag = "--verbose"
  print_flag = "-p"
  can_commit = true
  json_parser = "claude"
  ```
- Model selection: `--model claude-sonnet-4` (no external catalog)
- Best practices for unattended mode
- Using `--dangerously-skip-permissions` for automation

#### 4.3.2 Codex CLI (`/docs/agents/codex`)

**Source Reference**: `ralph-workflow/src/agents/registry.rs`

**Content**:
- Installation: Link to OpenAI Codex CLI installation
- Configuration in `ralph-workflow.toml`:
  ```toml
  [agents.codex]
  cmd = "codex"
  output_flag = "--output-format json"
  yolo_flag = "--full-auto"
  can_commit = true
  json_parser = "codex"
  ```
- Model selection: Built into Codex CLI (no external catalog)
- Recommended as reviewer (different perspective from Claude)
- `--full-auto` flag for unattended operation

#### 4.3.3 OpenCode (`/docs/agents/opencode`)

**Source Reference**: `ralph-workflow/src/agents/opencode_api/`, `ralph-workflow/src/agents/providers/types.rs`

**Content**:
- Installation: Link to OpenCode installation
- **Model Selection via models.dev**:
  - Link to `/opencode-models` browser
  - `-m provider/model` syntax
  - Free tier models (e.g., `opencode/glm-4.7-free`)
- Configuration in `ralph-workflow.toml`:
  ```toml
  [agents.opencode]
  cmd = "opencode"
  output_flag = "--output-format json"
  yolo_flag = "--yes"
  can_commit = true
  json_parser = "opencode"
  model_flag = "-m opencode/glm-4.7-free"  # From models.dev
  ```
- Provider list (from `src/agents/providers/types.rs`):
  - OpenCode Zen (gateway)
  - Anthropic, OpenAI, Google
  - Groq, Together, Fireworks, Cerebras
  - OpenRouter, DeepSeek, Mistral
  - And 60+ more providers

#### 4.3.4 CCS Profiles (`/docs/agents/ccs`)

**Source Reference**: `ralph-workflow/src/agents/ccs.rs`, `ralph-workflow/src/agents/ccs_env.rs`

**IMPORTANT CALLOUT**:
> CCS (Claude Code Switch) has its **own configuration system** separate from models.dev. CCS manages multiple Claude profiles with different providers and credentials stored in `~/.ccs/`.

**Content**:

1. **What is CCS?**
   - Universal AI profile manager
   - Supports Claude, Gemini, Copilot, OpenRouter, and more
   - Installation: `https://github.com/kaitranntt/ccs`

2. **CCS Configuration Files** (from `src/agents/ccs_env.rs`):
   - `~/.ccs/config.json` or `~/.ccs/config.yaml` - Profile registry
   - `~/.ccs/{profile}.settings.json` - Per-profile credentials

3. **Using CCS with Ralph**:
   ```bash
   # Use specific CCS profile
   ralph --developer-agent ccs/glm
   ralph --developer-agent ccs/gemini
   ralph --developer-agent ccs/work
   
   # Default CCS profile
   ralph --developer-agent ccs
   ```

4. **Ralph Configuration for CCS** (from `src/config/unified.rs`):
   ```toml
   [ccs]
   output_flag = "--output-format=stream-json"
   verbose_flag = "--verbose"
   print_flag = "-p"
   yolo_flag = "--dangerously-skip-permissions"
   json_parser = "claude"
   can_commit = true
   
   [ccs_aliases]
   work = "ccs work"
   personal = "ccs personal"
   glm = "ccs glm"
   gemini = "ccs gemini"
   ```

5. **Direct Claude Bypass** (from `src/agents/ccs.rs` lines 7-35):
   - Ralph can bypass the `ccs` wrapper and use `claude` directly
   - Environment variables loaded from CCS settings
   - Preserves streaming flag passthrough

6. **Troubleshooting CCS**:
   - `RALPH_CCS_DEBUG=1` for detailed logging
   - Profile fuzzy matching (typo tolerance)
   - Common issues from `docs/agent-compatibility.md`

---

### 4.4 OpenCode Model Browser (`/opencode-models`)

**CRITICAL**: This page is **ONLY for OpenCode**. Must include prominent notice.

**Banner**:
```
┌─────────────────────────────────────────────────────────────────┐
│  OPENCODE MODEL BROWSER                                         │
│                                                                 │
│  This catalog is for OpenCode only.                            │
│  - Claude Code: Uses Anthropic models (--model flag)           │
│  - Codex CLI: Uses OpenAI models (built-in)                    │
│  - CCS Profiles: Uses ~/.ccs/ configuration (see CCS Guide)    │
└─────────────────────────────────────────────────────────────────┘
```

**Data Source**: `https://models.dev/api.json`

**Features**:

1. **Provider Filter** (sidebar)
   - OpenCode Zen, Anthropic, OpenAI, Google
   - Groq, Together, Fireworks, Cerebras
   - OpenRouter, DeepSeek, Mistral
   - (Full list from `src/agents/providers/types.rs`)

2. **Model Search**
   - Search by model name
   - Filter by context length
   - Free tier only toggle

3. **Model Card Display**:
   ```
   ┌─────────────────────────────────────────────┐
   │ GLM-4.7 Free                          FREE  │
   │ Provider: OpenCode Zen                      │
   │ Context: 204,800 tokens                     │
   │ Family: glm-free                            │
   │                                             │
   │ Model Flag: -m opencode/glm-4.7-free        │
   │                                             │
   │ [Copy Flag] [Copy Config Snippet]           │
   └─────────────────────────────────────────────┘
   ```

4. **Copy Actions**:
   - Model flag: `-m opencode/glm-4.7-free`
   - Config snippet:
     ```toml
     [agents.opencode-glm]
     cmd = "opencode"
     model_flag = "-m opencode/glm-4.7-free"
     json_parser = "opencode"
     can_commit = true
     ```

**API Implementation**:
```typescript
// app/api/opencode-models/route.ts
export async function GET() {
  const response = await fetch('https://models.dev/api.json', {
    next: { revalidate: 86400 } // 24 hours
  });
  const data = await response.json();
  return NextResponse.json(transformCatalog(data));
}
```

---

### 4.5 Configuration Reference (`/docs/configuration`)

**Source Reference**: `ralph-workflow/src/config/unified.rs`, `ralph-workflow/examples/ralph-workflow.toml`

**Content**:

1. **File Location**: `~/.config/ralph-workflow.toml`

2. **Configuration Sections**:

   **`[general]`** - Global settings (from `GeneralConfig` struct):
   ```toml
   [general]
   verbosity = 2                    # 0=quiet, 1=normal, 2=verbose, 3=full, 4=debug
   interactive = true               # Keep agent in foreground
   isolation_mode = true            # Delete NOTES.md/ISSUES.md at start
   auto_detect_stack = true         # Detect Rust/JS/Python for review
   checkpoint_enabled = true        # Enable checkpoint/resume
   developer_iters = 5              # Developer iterations
   reviewer_reviews = 2             # Review cycles
   developer_context = 1            # 0=minimal, 1=standard, 2=full
   reviewer_context = 0
   review_depth = "standard"        # standard/comprehensive/security/incremental
   ```

   **`[ccs]`** - CCS defaults (from `CcsConfig` struct):
   ```toml
   [ccs]
   output_flag = "--output-format=stream-json"
   yolo_flag = "--dangerously-skip-permissions"
   verbose_flag = "--verbose"
   print_flag = "-p"
   streaming_flag = "--include-partial-messages"
   json_parser = "claude"
   can_commit = true
   ```

   **`[ccs_aliases]`** - CCS profile shortcuts:
   ```toml
   [ccs_aliases]
   work = "ccs work"
   personal = "ccs personal"
   glm = "ccs glm"
   gemini = { cmd = "ccs gemini", json_parser = "claude" }
   ```

   **`[agents]`** - Custom agent definitions (from `AgentConfigToml`):
   ```toml
   [agents.my-claude-opus]
   cmd = "claude"
   output_flag = "--output-format=stream-json"
   yolo_flag = "--dangerously-skip-permissions"
   model_flag = "--model claude-opus-4"
   json_parser = "claude"
   can_commit = true
   ```

   **`[agent_chain]`** - Fallback configuration:
   ```toml
   [agent_chain]
   developer = ["claude", "codex", "opencode"]
   reviewer = ["codex", "claude"]
   commit = ["claude", "codex", "opencode"]
   max_retries = 3
   retry_delay_ms = 1000
   ```

3. **Environment Variables** (from `src/cli/args.rs`):
   | Variable | Purpose |
   |----------|---------|
   | `RALPH_DEVELOPER_ITERS` | Override dev iterations |
   | `RALPH_REVIEWER_REVIEWS` | Override review cycles |
   | `RALPH_VERBOSITY` | Output verbosity (0-4) |
   | `RALPH_CCS_DEBUG` | Enable CCS debug logging |
   | `RALPH_REVIEWER_UNIVERSAL_PROMPT` | Force simplified review prompt |

4. **YOLO Mode (Unattended Operation)**:

   Ralph is designed for **unattended automation**. YOLO ("You Only Live Once") mode enables agents to run autonomously without user confirmation.

   **How YOLO Mode Works**:
   - No prompts for file operations
   - No confirmation for tool calls
   - Fully autonomous code changes
   - Permission is given upfront when you run `ralph`

   **Agent-Specific YOLO Flags**:
   | Agent | YOLO Flag | Configuration |
   |-------|-----------|---------------|
   | Claude Code | `--dangerously-skip-permissions` | `yolo_flag = "--dangerously-skip-permissions"` |
   | CCS Profiles | `--dangerously-skip-permissions` | `yolo_flag = "--dangerously-skip-permissions"` |
   | Codex CLI | `--full-auto` | `yolo_flag = "--full-auto"` |
   | OpenCode | `--yes` | `yolo_flag = "--yes"` |
   | Aider | `--yes-always` | `yolo_flag = "--yes-always"` |

   **Disabling YOLO Mode**:
   If you prefer interactive mode (not recommended for unattended operation), set `yolo_flag = ""` in your agent configuration:

   ```toml
   [agents.claude]
   cmd = "claude"
   output_flag = "--output-format=stream-json"
   yolo_flag = ""  # Disable YOLO mode for interactive prompts
   ```

   **Note**: YOLO mode is enabled by default because Ralph is designed for unattended operation. The system trusts your upfront decision to run Ralph and won't ask for confirmation during execution.

---

### 4.6 Config Builder (`/config-builder`)

**Interactive wizard with agent-specific flows**

**Step 1: Agent Selection**
```
Which AI coding agents do you have installed?

[ ] Claude Code (Anthropic)
    - Uses Anthropic models directly
    
[ ] Codex CLI (OpenAI)
    - Uses OpenAI models directly
    
[ ] OpenCode
    - Uses models.dev catalog (75+ providers)
    - [Browse Models →]
    
[ ] CCS (Claude Code Switch)
    - Multiple profiles with different providers
    - Requires separate ~/.ccs/ setup
```

**Step 2: Model Configuration** (conditional)

*If OpenCode selected:*
```
Select OpenCode model (from models.dev):

Provider: [OpenCode Zen     ▼]
Model:    [GLM-4.7 Free     ▼]

Preview: -m opencode/glm-4.7-free
```

*If CCS selected:*
```
Enter your CCS profile names (comma-separated):

[work, personal, glm, gemini          ]

These will be available as:
  - ccs/work
  - ccs/personal
  - ccs/glm
  - ccs/gemini
```

**Step 3: Role Assignment**
```
Assign agents to roles:

Developer (primary): [claude        ▼]
Developer fallbacks: [codex, opencode]

Reviewer (primary):  [codex         ▼]
Reviewer fallbacks:  [claude        ]
```

**Step 4: Iteration Settings**
```
Choose a preset or customize:

( ) Quick    - 1 dev iteration, 1 review
( ) Rapid    - 2 dev iterations, 1 review
(•) Standard - 5 dev iterations, 2 reviews
( ) Thorough - 10 dev iterations, 5 reviews
( ) Long     - 15 dev iterations, 10 reviews
( ) Custom   - [Developer: __] [Reviewer: __]
```

**Step 5: Output**
```toml
# Generated ralph-workflow.toml
# Save to: ~/.config/ralph-workflow.toml

[general]
verbosity = 2
interactive = true
developer_iters = 5
reviewer_reviews = 2

[ccs_aliases]
work = "ccs work"
personal = "ccs personal"
glm = "ccs glm"

[agent_chain]
developer = ["claude", "codex", "opencode"]
reviewer = ["codex", "claude"]

[Copy] [Download] [Save to Clipboard]
```

---

### 4.7 Unattended Operation Guide (`/docs/unattended`)

**Source Reference**: `ralph-workflow/examples/ralph-workflow.toml` (YOLO mode section)

**Content**:

1. **Philosophy**
   > "Think like a Product Manager, not a Pair Programmer"
   
   - AI agents cannot ask clarification questions in unattended mode
   - Detailed specs prevent bad assumptions
   - Define edge cases upfront

2. **YOLO Mode Explained** (from config example lines 114-143):
   ```
   Ralph is designed for UNATTENDED automation. Agents run with auto-approval
   (yolo mode) by default, meaning:
   - No prompts for file operations
   - No confirmation for tool calls
   - Fully autonomous code changes
   
   This is deliberate: permission is given upfront when you run Ralph.
   ```

3. **Agent-Specific YOLO Flags**:
   | Agent | YOLO Flag | Source |
   |-------|-----------|--------|
   | Claude/CCS | `--dangerously-skip-permissions` | `src/agents/ccs.rs` |
   | Codex | `--full-auto` | Built-in registry |
   | OpenCode | `--yes` | Built-in registry |
   | Aider | `--yes-always` | Built-in registry |

4. **PROMPT.md Best Practices**:
   - Be explicit about scope boundaries
   - Include acceptance criteria as checkboxes
   - Define edge cases and error handling
   - Reference existing code patterns
   - Specify test requirements

5. **Overnight/Long-Running Operation**:

   Ralph is designed to run unattended for extended periods. Here's how to get started easily with a small prompt that can run overnight:

   **Quick Start for Overnight Runs**:
   ```bash
   # 1. Initialize config (if not already done)
   ralph --init

   # 2. Create a simple PROMPT.md for overnight work
   cat > PROMPT.md << 'EOF'
   # Task: Add User Authentication

   Implement JWT-based authentication:
   - Login endpoint with email/password
   - JWT token generation and validation
   - Protected routes middleware
   - Password hashing with bcrypt
   - Unit tests for auth functions

   Use existing project patterns in src/auth/.
   EOF

   # 3. Run with higher iteration count for overnight processing
   ralph -D 10 -R 5
   # -D 10: 10 developer iterations (more thorough)
   # -R 5:  5 review cycles (comprehensive review)
   # Commit message is optional - uses default if omitted
   ```

   **Estimated Run Times** (varies by task complexity and model speed):
   | Configuration | Estimated Time | Best For |
   |--------------|----------------|----------|
   | `ralph -Q` (quick: 1+1) | 10-30 minutes | Small fixes, typos |
   | `ralph -U` (rapid: 2+1) | 20-60 minutes | Minor features |
   | `ralph` (default: 5+2) | 1-3 hours | Standard features |
   | `ralph -D 10 -R 5` | 3-8 hours | Complex features (overnight) |
   | `ralph -L` (long: 15+10) | 8-12 hours | Critical features (full night) |

   **Session Continuation**:
   - If Ralph is interrupted, run `ralph --resume` to continue
   - Ralph will show progress and ask if you want to continue
   - Use `ralph --no-resume` in scripts to skip the prompt

   **Using Worktrees for Parallel Overnight Runs**:
   ```bash
   # Set up multiple worktrees before leaving
   git worktree add ../feature-auth feature/auth
   git worktree add ../feature-api feature/api

   # Start Ralph in each worktree
   cd ../feature-auth && ralph -D 10 "implement auth"
   cd ../feature-api && ralph -D 10 "implement API"

   # Check progress in the morning
   ```

6. **Worktree Strategy** (RECOMMENDED for parallel features):
   ```bash
   # Create worktrees for parallel development
   git worktree add ../feature-auth feature/auth
   git worktree add ../feature-api feature/api

   # Run Ralph in each worktree
   cd ../feature-auth && ralph
   cd ../feature-api && ralph
   ```

   **Why Worktrees?**
   - Ralph is designed for unattended, long-running tasks (hours to overnight)
   - Running Ralph on main blocks you from working on other features
   - Worktrees let you run multiple Ralph sessions in parallel
   - Each worktree has its own git state, so commits don't interfere

7. **Checkpoint/Resume**:
   - Checkpoints stored in `.agent/checkpoint.json`
   - Interactive resume prompt when checkpoint exists
   - Resume with `ralph --resume`
   - Skip prompt with `ralph --no-resume` (for CI/CD)
   - Recovery strategies: `--recovery-strategy fail|auto|force`
   - Validate with `ralph --dry-run`

   **Interactive Resume Behavior**:
   When Ralph detects a checkpoint from a previous interrupted run, it displays:
   ```
   Resuming from checkpoint:
   - Phase: Development (3/10 iterations)
   - Elapsed: 2h 15m
   - Next: Developer iteration 4

   Continue from checkpoint? [Y/n]:
   ```

---

### 4.8 Git Worktree Workflow (`/docs/workflow/worktrees`)

**Source Reference**: README.md lines 64-66 (author's recommended workflow)

**Why Use Worktrees with Ralph?**

Ralph Workflow is designed for **unattended, long-running tasks**. A single Ralph run can take anywhere from 30 minutes to overnight depending on:
- Number of developer iterations (default: 5, but you can use `-D 10` for longer runs)
- Number of review cycles (default: 2, but can be increased with `-R 5`)
- Complexity of the task
- AI agent response times

**The Problem**: Running Ralph directly on your main branch blocks you from:
- Working on other features simultaneously
- Making quick fixes while Ralph is running
- Reviewing Ralph's output until it completes

**The Solution: Git Worktrees**

Git worktrees allow you to have multiple working directories for the same repository, each checked out to a different branch. This is the **recommended workflow** for Ralph.

#### Creating Worktrees

```bash
# From your main repository
cd /path/to/your/repo

# Create worktrees for different features
git worktree add ../feature-auth feature/auth
git worktree add ../feature-api feature/api
git worktree add ../bugfix-login bugfix/login

# List all worktrees
git worktree list
```

#### Running Ralph in Worktrees

```bash
# Run Ralph in each worktree simultaneously
cd ../feature-auth
ralph "implement user authentication"

# In another terminal
cd ../feature-api
ralph "implement REST API"

# In a third terminal (work on something else yourself)
cd ../your-main-repo
# Continue your own work while Ralph runs in parallel
```

#### Overnight Workflow Example

```bash
# Before leaving work, set up multiple Ralph sessions:
cd ../feature-auth && ralph -D 10 "implement JWT auth"
cd ../feature-api && ralph -D 10 "implement REST endpoints"
cd ../bugfix-login && ralph -Q "fix login timeout"

# Go home. Wake up to 3 completed features.
```

#### Worktree Management

```bash
# Remove a worktree after merging
git worktree remove ../feature-auth

# Or prune after merging the branch
git worktree prune
```

#### Benefits Summary

| Benefit | Description |
|---------|-------------|
| **Parallel Development** | Run multiple Ralph sessions simultaneously |
| **Unblocked Main Branch** | Keep working while Ralph processes in background |
| **Isolation** | Each worktree has independent git state |
| **Easy Cleanup** | Remove worktrees after merging PRs |

---

### 4.9 Workflow Phases Documentation (`/docs/workflow/*`)

**Source Reference**: `ralph-workflow/src/phases/`

#### Planning Phase (`/docs/workflow/planning`)
- Input: PROMPT.md
- Output: PLAN.md
- Template: `planning_xml` (from `src/prompts/template_catalog.rs`)

#### Development Phase (`/docs/workflow/development`)
- Iterative execution
- XML status tracking
- Template: `developer_iteration_xml`

#### Review Phase (`/docs/workflow/review`)
- Code review generation
- Output: ISSUES.md
- Templates: `standard_review`, `comprehensive_review`, `security_review`, `universal_review`
- Universal prompt for GLM/ZhipuAI compatibility

#### Commit Phase (`/docs/workflow/commit`)
- Conventional Commits generation
- Template: `commit_message_xml`

---

### 4.10 PROMPT.md Template Gallery (`/prompt-templates`)

**Source Reference**: `ralph-workflow/src/prompts/template_catalog.rs`

**Available Templates** (from embedded templates):

| Template | Description | Use Case |
|----------|-------------|----------|
| `developer_iteration_xml` | Implementation with XML output | Standard development |
| `planning_xml` | Planning phase prompt | Task breakdown |
| `review_xml` | Review mode prompt | Code review |
| `fix_mode_xml` | Fix issues from review | Bug fixing |
| `commit_message_xml` | Conventional Commits | Commit generation |
| `standard_review` | Balanced review checklist | Default review |
| `comprehensive_review` | 12-category thorough review | Deep review |
| `security_review` | OWASP Top 10 focused | Security audit |
| `universal_review` | Simplified for GLM/Qwen | Compatibility |
| `conflict_resolution` | Merge conflict handling | Rebase conflicts |

**Template Card UI**:
```
┌──────────────────────────────────────────────────┐
│ Standard Review                         standard │
│                                                  │
│ Balanced review with comprehensive checklist.    │
│ Best for: General code review                    │
│                                                  │
│ [Preview] [Copy] [Download]                      │
└──────────────────────────────────────────────────┘
```

---

## 5. API Design

### `/api/opencode-models` - models.dev Proxy (OpenCode Only)

```typescript
// app/api/opencode-models/route.ts
import { NextResponse } from 'next/server';

const MODELS_DEV_URL = 'https://models.dev/api.json';
const CACHE_TTL = 86400; // 24 hours

export async function GET() {
  const response = await fetch(MODELS_DEV_URL, {
    next: { revalidate: CACHE_TTL }
  });
  
  if (!response.ok) {
    return NextResponse.json(
      { error: 'Failed to fetch models catalog' },
      { status: 502 }
    );
  }
  
  const data = await response.json();
  const catalog = transformToNormalizedFormat(data);
  
  return NextResponse.json({
    ...catalog,
    notice: 'This catalog is for OpenCode only. Claude and Codex use built-in models. CCS uses ~/.ccs/ configuration.',
    fetchedAt: new Date().toISOString()
  });
}

interface NormalizedCatalog {
  providers: Provider[];
  models: Model[];
}

interface Provider {
  id: string;
  name: string;
  description: string;
}

interface Model {
  id: string;
  name: string;
  providerId: string;
  family?: string;
  contextLength?: number;
  isFree: boolean;
  modelFlag: string; // e.g., "-m opencode/glm-4.7-free"
}
```

---

## 6. Component Library

### Core Components

| Component | Purpose | Props |
|-----------|---------|-------|
| `<AgentCard>` | Agent feature display | `agent`, `features`, `modelSource` |
| `<ModelCard>` | OpenCode model display | `model`, `provider`, `onCopy` |
| `<ConfigPreview>` | TOML syntax highlighting | `config`, `copyable` |
| `<AgentBadge>` | Agent type indicator | `agent: 'claude' \| 'codex' \| 'opencode' \| 'ccs'` |
| `<ModelSourceNotice>` | Clarification banner | `currentAgent` |
| `<CodeBlock>` | Syntax-highlighted code | `language`, `code`, `copyable` |
| `<Terminal>` | Animated terminal demo | `commands`, `speed` |
| `<DocsNav>` | Documentation sidebar | `sections`, `current` |
| `<Callout>` | Info/warning/danger boxes | `type`, `title`, `children` |

### Agent-Specific Components

```tsx
// components/model-source-notice.tsx
export function ModelSourceNotice({ agent }: { agent: string }) {
  if (agent === 'opencode') {
    return (
      <Callout type="info" title="Model Selection">
        OpenCode uses the <Link href="/opencode-models">models.dev catalog</Link>.
        Browse 75+ providers and select with <code>-m provider/model</code>.
      </Callout>
    );
  }
  
  if (agent === 'ccs') {
    return (
      <Callout type="warning" title="CCS Configuration">
        CCS profiles are configured in <code>~/.ccs/</code>, not models.dev.
        See the <Link href="/docs/agents/ccs">CCS Guide</Link> for setup.
      </Callout>
    );
  }
  
  // claude or codex
  return (
    <Callout type="info" title="Built-in Models">
      {agent === 'claude' ? 'Claude Code' : 'Codex CLI'} uses built-in models.
      No external catalog required.
    </Callout>
  );
}
```

---

## 7. Source Code References

The website documentation MUST be accurate to the source code. Reference these files:

| Documentation Topic | Primary Source Files |
|---------------------|---------------------|
| CLI flags & presets | `ralph-workflow/src/cli/args.rs` |
| Configuration schema | `ralph-workflow/src/config/unified.rs` |
| Configuration types | `ralph-workflow/src/config/types.rs` |
| Agent registry | `ralph-workflow/src/agents/registry.rs` |
| Agent configuration | `ralph-workflow/src/agents/config.rs` |
| CCS implementation | `ralph-workflow/src/agents/ccs.rs` |
| CCS environment loading | `ralph-workflow/src/agents/ccs_env.rs` |
| OpenCode API client | `ralph-workflow/src/agents/opencode_api/mod.rs` |
| OpenCode providers | `ralph-workflow/src/agents/providers/types.rs` |
| Workflow phases | `ralph-workflow/src/phases/*.rs` |
| JSON parsers | `ralph-workflow/src/json_parser/mod.rs` |
| Template catalog | `ralph-workflow/src/prompts/template_catalog.rs` |
| Example config | `ralph-workflow/examples/ralph-workflow.toml` |
| Agent compatibility | `docs/agent-compatibility.md` |

**When documentation is unclear, always check the source code.**

---

## 8. Directory Structure

```
website/
├── app/
│   ├── (marketing)/
│   │   ├── page.tsx                    # Landing page
│   │   ├── features/page.tsx
│   │   └── use-cases/page.tsx
│   │
│   ├── (docs)/
│   │   └── docs/
│   │       ├── layout.tsx              # Docs layout with sidebar
│   │       ├── page.tsx                # Docs hub
│   │       ├── [[...slug]]/page.tsx    # Dynamic MDX routes
│   │       └── _content/               # MDX source files
│   │
│   ├── (tools)/
│   │   ├── opencode-models/page.tsx    # OpenCode model browser (NOT for Claude/Codex/CCS)
│   │   ├── config-builder/page.tsx
│   │   └── prompt-templates/page.tsx
│   │
│   ├── api/
│   │   └── opencode-models/route.ts    # models.dev proxy
│   │
│   ├── layout.tsx
│   ├── globals.css
│   └── not-found.tsx
│
├── components/
│   ├── ui/                             # shadcn components
│   ├── marketing/
│   │   ├── hero.tsx
│   │   ├── agent-cards.tsx
│   │   └── feature-grid.tsx
│   ├── docs/
│   │   ├── sidebar.tsx
│   │   ├── toc.tsx
│   │   ├── search.tsx
│   │   └── model-source-notice.tsx
│   ├── tools/
│   │   ├── model-browser.tsx
│   │   ├── model-card.tsx
│   │   ├── model-filter.tsx
│   │   └── config-wizard.tsx
│   └── shared/
│       ├── code-block.tsx
│       ├── terminal-animation.tsx
│       ├── callout.tsx
│       └── copy-button.tsx
│
├── lib/
│   ├── opencode-models.ts              # models.dev client (OpenCode only)
│   ├── config-schema.ts                # TOML schema types
│   └── templates.ts                    # Template definitions
│
├── content/
│   └── docs/                           # MDX documentation files
│       ├── index.mdx
│       ├── getting-started.mdx
│       ├── installation.mdx
│       ├── configuration.mdx
│       ├── agents/
│       │   ├── index.mdx
│       │   ├── claude.mdx
│       │   ├── codex.mdx
│       │   ├── opencode.mdx            # Links to /opencode-models
│       │   └── ccs.mdx                 # Separate from models.dev
│       ├── workflow/
│       │   ├── index.mdx
│       │   ├── planning.mdx
│       │   ├── development.mdx
│       │   ├── review.mdx
│       │   └── commit.mdx
│       ├── unattended.mdx
│       ├── prompt-guide.mdx
│       └── troubleshooting.mdx
│
├── public/
│   ├── images/
│   │   ├── agents/                     # Agent logos
│   │   └── diagrams/                   # Workflow diagrams
│   └── og/                             # Open Graph images
│
├── tailwind.config.ts
├── next.config.ts
├── tsconfig.json
└── package.json
```

---

## 9. Development Phases

### Phase 1: Foundation (Week 1-2)

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Next.js + TypeScript setup | High | 2h | App Router |
| Tailwind + shadcn setup | High | 2h | |
| Basic layout + navigation | High | 4h | |
| Landing page (static) | High | 8h | |
| Documentation MDX setup | High | 4h | |
| Getting Started page | High | 4h | Reference `examples/ralph-workflow.toml` |

**Deliverable**: Basic marketing site with getting started docs

### Phase 2: Documentation (Week 3-4)

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Configuration reference | High | 8h | Reference `src/config/unified.rs` |
| Claude Code guide | High | 4h | |
| Codex CLI guide | High | 4h | |
| OpenCode guide | High | 4h | Link to model browser |
| CCS guide | High | 6h | Reference `src/agents/ccs.rs`, separate from models.dev |
| Workflow phase docs | Medium | 8h | Reference `src/phases/` |
| Unattended operation | High | 6h | |
| Search implementation | Medium | 4h | |

**Deliverable**: Complete documentation site

### Phase 3: Interactive Tools (Week 5-6)

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| models.dev API integration | High | 8h | OpenCode only, clear notice |
| OpenCode Model Browser UI | High | 12h | Rename from generic "models" |
| Config builder wizard | Medium | 16h | Agent-specific flows |
| Template gallery | Medium | 8h | From `src/prompts/template_catalog.rs` |
| Copy/download actions | Low | 4h | |

**Deliverable**: Functional interactive tools

### Phase 4: Polish (Week 7-8)

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Animations + transitions | Low | 8h | |
| Mobile responsiveness | High | 8h | |
| Performance optimization | Medium | 4h | |
| Analytics integration | Low | 2h | |
| OG images / social cards | Medium | 4h | |
| Final testing + fixes | High | 8h | |

**Deliverable**: Production-ready website

---

## 10. Success Criteria

### Launch Checklist

- [ ] Clear distinction between OpenCode models.dev vs Claude/Codex/CCS
- [ ] Model browser explicitly labeled "OpenCode Model Browser"
- [ ] CCS documentation separate from models.dev
- [ ] All agent guides accurate to source code
- [ ] Configuration reference matches `unified.rs` struct
- [ ] Mobile responsive
- [ ] Lighthouse score > 90
- [ ] No broken links

### Content Accuracy Checklist

- [ ] CLI flags match `src/cli/args.rs`
- [ ] Config options match `src/config/unified.rs`
- [ ] CCS behavior matches `src/agents/ccs.rs`
- [ ] OpenCode providers match `src/agents/providers/types.rs`
- [ ] Templates match `src/prompts/template_catalog.rs`

---

## 11. Open Questions

1. **Domain**: Is `ralph.dev` available? Alternatives: `ralphworkflow.dev`, `useralph.dev`

2. **Hosting**: Vercel (recommended for Next.js) or self-hosted?

3. **Analytics**: Plausible (privacy-focused) vs Google Analytics vs none?

4. **Documentation sync**: 
   - Manual updates per release?
   - Build-time extraction from source code?
   - Link to GitHub raw files?

5. **CCS documentation depth**: How much to duplicate from CCS's own docs vs linking?

6. **Blog/Changelog**: Include release notes section?

---

## Appendix A: Agent Compatibility Matrix Component

From `docs/agent-compatibility.md`:

```tsx
// components/docs/agent-matrix.tsx
const agents = [
  { 
    name: 'Claude Code', 
    developer: 'excellent', 
    reviewer: 'excellent',
    modelSource: 'Built-in (Anthropic)'
  },
  { 
    name: 'Codex CLI', 
    developer: 'excellent', 
    reviewer: 'excellent',
    modelSource: 'Built-in (OpenAI)'
  },
  { 
    name: 'OpenCode', 
    developer: 'good', 
    reviewer: 'good',
    modelSource: 'models.dev (75+ providers)'
  },
  { 
    name: 'CCS/GLM', 
    developer: 'good', 
    reviewer: 'partial',
    modelSource: '~/.ccs/ profiles'
  },
];

export function AgentMatrix() {
  return (
    <table className="w-full">
      <thead>
        <tr>
          <th>Agent</th>
          <th>Developer</th>
          <th>Reviewer</th>
          <th>Model Source</th>
        </tr>
      </thead>
      <tbody>
        {agents.map(agent => (
          <tr key={agent.name}>
            <td>{agent.name}</td>
            <td><StatusBadge status={agent.developer} /></td>
            <td><StatusBadge status={agent.reviewer} /></td>
            <td className="text-sm text-muted-foreground">{agent.modelSource}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

---

## Appendix B: OpenCode Provider List

From `ralph-workflow/src/agents/providers/types.rs`:

```typescript
// lib/opencode-providers.ts
export const OPENCODE_PROVIDERS = [
  // Gateway
  { id: 'opencode-zen', name: 'OpenCode Zen', category: 'Gateway' },
  
  // Chinese AI
  { id: 'zai-direct', name: 'Z.AI Direct', category: 'Chinese AI' },
  { id: 'moonshot', name: 'Moonshot AI / Kimi', category: 'Chinese AI' },
  { id: 'minimax', name: 'MiniMax AI', category: 'Chinese AI' },
  
  // Major Cloud
  { id: 'anthropic', name: 'Anthropic', category: 'Major Cloud' },
  { id: 'openai', name: 'OpenAI', category: 'Major Cloud' },
  { id: 'google', name: 'Google AI', category: 'Major Cloud' },
  { id: 'google-vertex', name: 'Google Vertex AI', category: 'Major Cloud' },
  { id: 'amazon-bedrock', name: 'Amazon Bedrock', category: 'Major Cloud' },
  { id: 'azure-openai', name: 'Azure OpenAI', category: 'Major Cloud' },
  
  // Fast Inference
  { id: 'groq', name: 'Groq', category: 'Fast Inference' },
  { id: 'together', name: 'Together AI', category: 'Fast Inference' },
  { id: 'fireworks', name: 'Fireworks AI', category: 'Fast Inference' },
  { id: 'cerebras', name: 'Cerebras', category: 'Fast Inference' },
  { id: 'sambanova', name: 'SambaNova', category: 'Fast Inference' },
  
  // Gateway/Aggregator
  { id: 'openrouter', name: 'OpenRouter', category: 'Gateway' },
  { id: 'cloudflare', name: 'Cloudflare Workers AI', category: 'Gateway' },
  
  // Specialized
  { id: 'deepseek', name: 'DeepSeek', category: 'Specialized' },
  { id: 'xai', name: 'xAI (Grok)', category: 'Specialized' },
  { id: 'mistral', name: 'Mistral AI', category: 'Specialized' },
  { id: 'cohere', name: 'Cohere', category: 'Specialized' },
  { id: 'perplexity', name: 'Perplexity', category: 'Specialized' },
  
  // Local
  { id: 'ollama', name: 'Ollama', category: 'Local' },
  { id: 'lmstudio', name: 'LM Studio', category: 'Local' },
  { id: 'llamacpp', name: 'llama.cpp', category: 'Local' },
  
  // ... and more (75+ total)
] as const;
```

---

## Appendix C: Landing Page Copy

### Hero Section

**Headline**: Unattended AI Development Orchestration

**Subheadline**: Ralph manages your AI coding agents through long-running tasks with automatic fallback, review cycles, and checkpoint/resume. Write your spec, start Ralph, and come back to finished code.

**CTA Primary**: Get Started  
**CTA Secondary**: Read the Docs

### Supported Agents Section

```
┌─────────────────────────────────────────────────────────────────┐
│                    SUPPORTED AI CODING AGENTS                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Claude Code          Codex CLI           OpenCode            │
│   ───────────          ─────────           ────────            │
│   Anthropic            OpenAI              75+ providers       │
│   Built-in models      Built-in models     via models.dev      │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   + CCS (Claude Code Switch)                                   │
│   ─────────────────────────────                                │
│   Multiple profiles: work, personal, glm, gemini               │
│   Own credential system (~/.ccs/)                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Quick Start

```bash
# Install
cargo install ralph-workflow

# Initialize
cd your-project
ralph --init

# Create PROMPT.md with your specification
vim PROMPT.md

# Run unattended (commit message is optional - uses default if omitted)
ralph
# OR with custom commit message:
ralph "feat: implement user authentication"

# Go to sleep. Wake up to finished code.
```

**Tip: Use Git Worktrees for Parallel Development**

Ralph is designed for long-running unattended tasks. Using Git worktrees lets you run multiple Ralph sessions simultaneously:

```bash
# Create worktrees for different features
git worktree add ../feature-auth feature/auth
git worktree add ../feature-api feature/api

# Run Ralph in each worktree overnight
cd ../feature-auth && ralph "implement user auth"
cd ../feature-api && ralph "implement REST API"
```

---

*End of Document*
