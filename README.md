# Ralph Workflow 

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**Ralph Workflow automates your AI coding workflow.** Write what you want in plain English, and Ralph coordinates AI agents to build it, review it, and commit it - all hands-free.

Inspired by [Geoffrey Huntley's Ralph Workflow concept](https://ghuntley.com/ralph/).

I created Ralph Workflow to enable this workflow with different AI agents, with intelligent fallback when you run out of tokens in Claude for instance, so you can truly run this very flexibly.

## Features

- **AI-Powered Development** - Automatically runs AI agents (Claude, Codex, OpenCode, Aider, etc.) to implement your features
- **Built-in Code Review** - Runs a reviewer agent to check quality and fix issues before committing
- **Automatic Git Commits** - Generates meaningful commit messages and commits your changes
- **Intelligent Fallback** - Automatically switches to backup agents when rate limits or errors occur
- **Language-Specific Reviews** - Detects your tech stack and provides tailored guidance (Rust, Python, JS/TS, Go, and more)
- **Checkpoint/Resume** - Recover from interruptions without losing progress
- **45+ Provider Support** - Works with OpenAI, Anthropic, Google, Groq, DeepSeek, and many more via OpenCode
- **Flexible Configuration** - Configure agents, models, and fallback chains via config file or environment variables

## How It Works

```
You write PROMPT.md     Ralph Workflow runs AI agents      You get working code
describing what         to implement and                   committed to your
you want built          review the changes                 git repository
       |                       |                                |
       v                       v                                v
   "Add a dark            Developer Agent                  git commit -m
    mode toggle"          writes the code                  "feat: add dark
                               |                            mode toggle"
                               v
                          Reviewer Agent
                          checks quality
                          and fixes issues
```

**That's it.** You describe your goal, Ralph Workflow does the coding.

## Who Is This For?

- **Vibecoding enthusiasts** - Let AI write your code while you focus on ideas
- **Developers** - Automate repetitive implementation tasks
- **Teams** - Consistent AI-assisted development with built-in code review
- **Anyone** building software with AI assistants (Claude, Codex, Aider, OpenCode, Goose, and more)

## Quick Start

### 1. Install Ralph Workflow

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

Ralph Workflow needs AI coding tools to do the actual work. Install at least one:

| Agent | Install | Notes |
|-------|---------|-------|
| **Claude Code** | `npm install -g @anthropic/claude-code` | Recommended developer agent |
| **Codex** | `npm install -g @openai/codex` | Recommended reviewer agent |
| **OpenCode** | See [opencode.ai](https://opencode.ai) | Works for both roles |
| **Aider** | `pip install aider-chat` | Popular alternative |

After installing, make sure you've authenticated with your chosen agent (e.g., `claude auth` or set API keys).

### 3. Run Ralph Workflow

Navigate to any git repository and:

```bash
# Option A: Create from template (recommended)
ralph --init-prompt feature-spec
# Edit PROMPT.md with your task details

# Option B: Interactive mode (prompts if PROMPT.md is missing)
ralph --interactive

# Option C: Create manually
echo "Add a button that says Hello World" > PROMPT.md

# Run Ralph Workflow
ralph
```

Ralph Workflow will:
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

### Running Ralph Workflow

```bash
# Basic usage - uses agents configured in agent_chain
ralph

# Interactive mode - prompts to create PROMPT.md if missing
ralph --interactive
# or: ralph -i

# Quick mode for rapid prototyping (1 dev iteration + 1 review)
ralph --quick

# Control how many times agents iterate
ralph --developer-iters 3 --reviewer-reviews 1

# See what's happening in detail
ralph --full
```

### PROMPT.md Templates

Ralph Workflow includes pre-built templates to help you create well-structured PROMPT.md files for different task types:

| Template | Name | Best For |
|----------|------|----------|
| `feature-spec` | Comprehensive product specification | Full features with multiple sections (Goal, Acceptance, Constraints, Context, Implementation Notes) |
| `bug-fix` | Bug fix template | Fixing bugs with Issue, Expected Behavior, and Acceptance sections |
| `refactor` | Code refactoring | Restructuring code with Goal and Acceptance sections |
| `test` | Test writing | Adding tests with Goal and Acceptance sections |
| `docs` | Documentation update | Updating docs with Goal and Acceptance sections |
| `quick` | Quick/small change | Minor changes with just Goal and Acceptance sections |

**List available templates:**
```bash
ralph --list-templates
```

**Create PROMPT.md from a template:**
```bash
# Create a feature specification template
ralph --init-prompt feature-spec

# Create a bug fix template
ralph --init-prompt bug-fix

# Create a quick change template
ralph --init-prompt quick
```

**Interactive mode:**
```bash
# Prompts to create PROMPT.md if it's missing
ralph --interactive
# or: ralph -i

# The interactive prompt will:
# 1. Ask if you want to create PROMPT.md from a template
# 2. Show all available templates
# 3. Let you select one (defaults to feature-spec)
# 4. Create the file and exit
# 5. You then edit the file and run ralph again
```

After creating the template, edit `PROMPT.md` with your task details, then run `ralph` as usual.

**Why use templates?**
- **Structure**: Ensures your PROMPT.md has all required sections (Goal, Acceptance)
- **Clarity**: Prompts you to include important context like constraints and implementation notes
- **Consistency**: Provides a standard format that AI agents understand well
- **Speed**: Start with a template instead of writing from scratch

### Checking What Agents Are Available

```bash
# List all configured agents
ralph --list-agents

# List only agents you have installed
ralph --list-available-agents
```

## Configuration

### First Run Setup

On first run, Ralph Workflow creates a `.agent/` folder in your repo with:
- `STATUS.md`, `NOTES.md`, `ISSUES.md` - Working files for agents
- `logs/` - Agent output logs

Ralph Workflow configuration lives in a single unified file:

```bash
ralph --init-global
# Or: ralph --init
# Creates ~/.config/ralph-workflow.toml
```

Use a custom config file path:
```bash
ralph --config /path/to/custom-config.toml
# Or: ralph -c /path/to/custom-config.toml
```

### Choosing Agents

Ralph Workflow uses two agents with different roles:

| Role | What It Does | Default |
|------|--------------|---------|
| **Developer** | Writes and implements code | First agent in `agent_chain.developer` |
| **Reviewer** | Reviews code and suggests fixes | First agent in `agent_chain.reviewer` |

The default agent chains are configured in `~/.config/ralph-workflow.toml` under `[agent_chain]`.

Change agents via command line:
```bash
ralph --developer-agent aider --reviewer-agent opencode
```

Or use a preset for common agent combinations:
```bash
ralph --preset opencode  # Use OpenCode for both developer and reviewer
```

Or via environment variables:
```bash
export RALPH_DEVELOPER_AGENT=aider
export RALPH_REVIEWER_AGENT=opencode
```

### Agent Configuration File

Edit `~/.config/ralph-workflow.toml` to customize agents or add new ones:

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

**Important (unattended runs / automation)**: Ralph defaults to enabling autonomous permissions for agents that support it (e.g. `yolo_flag = "--dangerously-skip-permissions"`). This is intentional: unattended operation relies on non-interactive execution.

To explicitly disable YOLO mode (recommended only for interactive/manual runs), set the flag to an empty string:
```toml
[agents.claude]
yolo_flag = ""
```

## Environment Variables

Quick reference for the most common settings:

| Variable | What It Does | Default |
|----------|--------------|---------|
| `RALPH_DEVELOPER_AGENT` | Which agent writes code | From `agent_chain` |
| `RALPH_REVIEWER_AGENT` | Which agent reviews code | From `agent_chain` |
| `RALPH_DEVELOPER_MODEL` | Model flag for developer (e.g., `-m opencode/glm-4.7-free`) | From `model_flag` |
| `RALPH_REVIEWER_MODEL` | Model flag for reviewer (e.g., `-m opencode/claude-sonnet-4`) | From `model_flag` |
| `RALPH_DEVELOPER_PROVIDER` | Provider override for developer (e.g., `opencode`, `anthropic`) | - |
| `RALPH_REVIEWER_PROVIDER` | Provider override for reviewer (e.g., `opencode`, `anthropic`) | - |
| `RALPH_DEVELOPER_ITERS` | How many times developer runs | `5` |
| `RALPH_REVIEWER_REVIEWS` | How many review passes | `2` |
| `RALPH_VERBOSITY` | Output detail (0-4) | `2` |
| `RALPH_AUTO_DETECT_STACK` | Enable language detection | `true` |
| `RALPH_CHECKPOINT_ENABLED` | Enable checkpoint/resume | `true` |
| `RALPH_STRICT_VALIDATION` | Strict PROMPT.md validation | `false` |
| `RALPH_REVIEW_DEPTH` | Review thoroughness level | `standard` |
| `FAST_CHECK_CMD` | Run after each iteration | - |
| `FULL_CHECK_CMD` | Run at the end (e.g., tests) | - |

Example with checks:
```bash
export FAST_CHECK_CMD="npm run lint"
export FULL_CHECK_CMD="npm test"
ralph
```

Notes:
- `FAST_CHECK_CMD` and `FULL_CHECK_CMD` are executed directly (no implicit `sh -c`).
- For shell features (pipes, `&&`, redirects), make it explicit: `FULL_CHECK_CMD="sh -c 'npm test && npm run lint'"`.
- Avoid embedding credentials in these commands; prefer environment variables or credential helpers.

## Advanced Features

### Language-Specific Code Review

Ralph Workflow automatically detects your project's technology stack and provides tailored review guidance:

- **Rust**: Memory safety, lifetime annotations, error handling, unsafe code audit
- **Python**: PEP 8 compliance, type hints, security (eval, SQL injection)
- **JavaScript/TypeScript**: Strict mode, Promise handling, DOM security
- **Go**: Error checking, goroutine safety, idiomatic patterns
- **And more**: Java, Ruby, C/C++, PHP, Swift, Kotlin, Elixir...

Framework-specific checks are also included (React, Django, Rails, Spring, etc.).

Disable with: `RALPH_AUTO_DETECT_STACK=false`

### Review Depth Levels

Control how thorough the code review is:

| Level | Flag | Description |
|-------|------|-------------|
| `standard` | `--review-depth standard` | Balanced review (default) |
| `comprehensive` | `--review-depth comprehensive` | In-depth analysis with priority-ordered checks |
| `security` | `--review-depth security` | Security-focused (OWASP Top 10) |
| `incremental` | `--review-depth incremental` | Changed files only (git diff) |

```bash
# Security-focused review for sensitive code
ralph --review-depth security

# Quick review of just your changes
ralph --review-depth incremental

# Set via environment variable
RALPH_REVIEW_DEPTH=comprehensive ralph
```

### Checkpoint and Resume

Long pipelines can be interrupted. Ralph Workflow saves checkpoints at each phase and can resume from the last saved phase start (the last phase may be re-run):

```bash
# If Ralph Workflow is interrupted, resume from where you left off:
ralph --resume
```

Checkpoints are saved in `.agent/checkpoint.json` and cleared on successful completion.

Disable with: `RALPH_CHECKPOINT_ENABLED=false`

### Dry Run Validation

Validate your setup before running agents:

```bash
ralph --dry-run
```

This checks:
- PROMPT.md exists and has content
- Goal and acceptance check sections (warnings if missing)
- Agent configuration is valid
- Project stack detection

### Review Metrics

The final summary now includes issue metrics from the review phase:
- Issues found by severity (Critical/High/Medium/Low)
- Resolution rate percentage
- Warning for unresolved blocking issues

### Automatic Fallback

If an agent hits rate limits or errors, Ralph Workflow automatically switches to the next agent in the chain. Configure fallback chains in `~/.config/ralph-workflow.toml`:

```toml
[agent_chain]
developer = ["claude", "codex", "aider"]  # Try in order
reviewer = ["codex", "claude"]
max_retries = 3
```

### OpenCode Provider Types

[OpenCode](https://opencode.ai) supports 45+ backend providers via the AI SDK, giving you maximum flexibility. Ralph recognizes all major providers with dedicated support for authentication and configuration.

> **Important:** OpenCode Zen (`opencode/*`) and Z.AI Direct (`zai/*`) are **separate endpoints** with different authentication!
> - `opencode/*` routes through OpenCode's Zen gateway at opencode.ai
> - `zai/*` or `zhipuai/*` connects directly to Z.AI's API at api.z.ai
> - Z.AI Coding Plan is a separate subscription tier selected during `opencode auth login` (model prefix remains `zai/*`)

Run `ralph --list-providers` to see all supported providers with authentication commands.

#### Provider Categories

**OpenCode Gateway**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `opencode/` | OpenCode Zen | `opencode/glm-4.7-free` | `opencode auth login` → "OpenCode Zen" |

**Chinese AI Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `zai/` | Z.AI (Standard tier) | `zai/glm-4.7` | `opencode auth login` → "Z.AI" |
| `zai/` | Z.AI (Coding Plan tier) | `zai/glm-4.7` | `opencode auth login` → "Z.AI Coding Plan" |
| `moonshot/` | Moonshot (Kimi) | `moonshot/kimi-k2` | Set `MOONSHOT_API_KEY` |
| `minimax/` | MiniMax | `minimax/abab6.5-chat` | Set `MINIMAX_API_KEY` |

**Major Cloud Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `anthropic/` | Anthropic | `anthropic/claude-sonnet-4` | Set `ANTHROPIC_API_KEY` |
| `openai/` | OpenAI | `openai/gpt-4o` | Set `OPENAI_API_KEY` |
| `google/` | Google AI Studio | `google/gemini-2.0-flash` | Set `GOOGLE_GENERATIVE_AI_API_KEY` |
| `google-vertex/` | Google Vertex AI | `google-vertex/gemini-2.0-flash` | `gcloud auth` + set `GOOGLE_VERTEX_PROJECT` |
| `amazon-bedrock/` | Amazon Bedrock | `amazon-bedrock/anthropic.claude-3-5-sonnet` | `aws configure` |
| `azure-openai/` | Azure OpenAI | `azure-openai/gpt-4o` | Set `AZURE_OPENAI_*` vars |

**Fast Inference Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `groq/` | Groq | `groq/llama-3.3-70b-versatile` | Set `GROQ_API_KEY` |
| `together/` | Together AI | `together/meta-llama/Llama-3-70b` | Set `TOGETHER_API_KEY` |
| `fireworks/` | Fireworks AI | `fireworks/llama-v3p1-70b` | Set `FIREWORKS_API_KEY` |
| `cerebras/` | Cerebras | `cerebras/llama3.3-70b` | Set `CEREBRAS_API_KEY` |
| `sambanova/` | SambaNova | `sambanova/Meta-Llama-3.3-70B` | Set `SAMBANOVA_API_KEY` |
| `deep-infra/` | Deep Infra | `deep-infra/meta-llama/Llama-3.3-70B` | Set `DEEPINFRA_API_KEY` |

**Gateway/Aggregator Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `openrouter/` | OpenRouter | `openrouter/anthropic/claude-3.5-sonnet` | Set `OPENROUTER_API_KEY` |
| `cloudflare/` | Cloudflare Workers AI | `cloudflare/@cf/meta/llama-3-8b` | Set `CLOUDFLARE_*` vars |
| `vercel/` | Vercel AI Gateway | `vercel/gpt-4o` | `opencode /connect vercel` |
| `helicone/` | Helicone | `helicone/gpt-4o` | `opencode /connect helicone` |
| `zenmux/` | ZenMux | `zenmux/gpt-4o` | `opencode /connect zenmux` |

**Specialized Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `deepseek/` | DeepSeek | `deepseek/deepseek-chat` | Set `DEEPSEEK_API_KEY` |
| `xai/` | xAI (Grok) | `xai/grok-2` | Set `XAI_API_KEY` |
| `mistral/` | Mistral AI | `mistral/mistral-large-latest` | Set `MISTRAL_API_KEY` |
| `cohere/` | Cohere | `cohere/command-r-plus` | Set `COHERE_API_KEY` |
| `perplexity/` | Perplexity | `perplexity/sonar-pro` | Set `PERPLEXITY_API_KEY` |
| `ai21/` | AI21 Labs | `ai21/jamba-1.5-large` | Set `AI21_API_KEY` |
| `copilot/` | GitHub Copilot | `copilot/gpt-4o` | GitHub Copilot subscription |
| `venice-ai/` | Venice AI | `venice-ai/llama-3-70b` | `opencode /connect venice-ai` |

**Cloud Platform Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `baseten/` | Baseten | `baseten/llama-3-70b` | `opencode /connect baseten` |
| `cortecs/` | Cortecs | `cortecs/llama-3-70b` | `opencode /connect cortecs` |
| `scaleway/` | Scaleway | `scaleway/llama-3-70b` | `opencode /connect scaleway` |
| `ovhcloud/` | OVHcloud | `ovhcloud/llama-3-70b` | `opencode /connect ovhcloud` |
| `io-net/` | IO.NET | `io-net/llama-3-70b` | `opencode /connect io-net` |
| `nebius/` | Nebius | `nebius/llama-3-70b` | `opencode /connect nebius` |

**Enterprise/Industry Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `sap-ai-core/` | SAP AI Core | `sap-ai-core/gpt-4o` | Set `AICORE_*` vars |
| `azure-cognitive-services/` | Azure Cognitive Services | `azure-cognitive-services/gpt-4o` | Set `AZURE_COGNITIVE_SERVICES_*` vars |

**Open-Source Model Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `huggingface/` | Hugging Face | `huggingface/Qwen/Qwen2.5-Coder-32B` | Set `HF_TOKEN` |
| `replicate/` | Replicate | `replicate/meta/llama-3-70b-instruct` | Set `REPLICATE_API_TOKEN` |

**Local Providers**
| Prefix | Provider | Example | Authentication |
|--------|----------|---------|----------------|
| `ollama/` | Ollama | `ollama/llama3` | Run `ollama serve` locally |
| `lmstudio/` | LM Studio | `lmstudio/local-model` | Start LM Studio server |
| `ollama-cloud/` | Ollama Cloud | `ollama-cloud/llama3` | `opencode /connect ollama-cloud` |
| `llama.cpp/` | llama.cpp | `llama.cpp/local-model` | Run `llama-server` locally |

### Agent Aliases for OpenCode Configurations

Ralph provides 45+ pre-configured agent aliases for different providers:

**Core Aliases**
| Alias | Provider | Model | Use Case |
|-------|----------|-------|----------|
| `opencode-zen-glm` | OpenCode Zen | `opencode/glm-4.7-free` | Free tier, try first |
| `opencode-zen-claude` | OpenCode Zen | `opencode/claude-sonnet-4` | Premium via OpenCode |
| `opencode-zai-glm` | Z.AI Direct | `zai/glm-4.7` | Z.AI direct access |
| `opencode-zai-glm-codingplan` | Z.AI Coding Plan | `zai/glm-4.7` | 3x usage, 1/7 cost (auth selection) |
| `opencode-direct-claude` | Anthropic | `anthropic/claude-sonnet-4` | Your API key |
| `opencode-openai` | OpenAI | `openai/gpt-4o` | Your API key |

**Additional Provider Aliases**
| Alias | Provider | Model |
|-------|----------|-------|
| `opencode-google` | Google AI Studio | `google/gemini-2.0-flash` |
| `opencode-vertex` | Google Vertex AI | `google-vertex/gemini-2.0-flash` |
| `opencode-groq` | Groq | `groq/llama-3.3-70b-versatile` |
| `opencode-deepseek` | DeepSeek | `deepseek/deepseek-chat` |
| `opencode-mistral` | Mistral AI | `mistral/mistral-large-latest` |
| `opencode-xai` | xAI (Grok) | `xai/grok-2` |
| `opencode-moonshot` | Moonshot | `moonshot/kimi-k2` |
| `opencode-openrouter` | OpenRouter | `openrouter/anthropic/claude-3.5-sonnet` |
| `opencode-bedrock` | Amazon Bedrock | `amazon-bedrock/anthropic.claude-3-5-sonnet` |
| `opencode-azure` | Azure OpenAI | `azure-openai/gpt-4o` |
| `opencode-ollama` | Ollama (local) | `ollama/llama3` |
| `opencode-together` | Together AI | `together/meta-llama/Llama-3-70b` |
| `opencode-fireworks` | Fireworks AI | `fireworks/llama-v3p1-70b` |
| `opencode-cohere` | Cohere | `cohere/command-r-plus` |
| `opencode-copilot` | GitHub Copilot | `copilot/gpt-4o` |
| `opencode-deepinfra` | Deep Infra | `deep-infra/meta-llama/Llama-3.3-70B` |
| `opencode-huggingface` | Hugging Face | `huggingface/Qwen/Qwen2.5-Coder-32B` |
| `opencode-cerebras` | Cerebras | `cerebras/llama3.3-70b` |
| `opencode-sambanova` | SambaNova | `sambanova/Meta-Llama-3.3-70B` |
| `opencode-perplexity` | Perplexity | `perplexity/sonar-pro` |
| `opencode-ai21` | AI21 Labs | `ai21/jamba-1.5-large` |
| `opencode-replicate` | Replicate | `replicate/meta/llama-3-70b-instruct` |
| `opencode-cloudflare` | Cloudflare Workers AI | `cloudflare/@cf/meta/llama-3.1-8b` |
| `opencode-baseten` | Baseten | `baseten/llama-3-70b` |
| `opencode-cortecs` | Cortecs | `cortecs/llama-3-70b` |
| `opencode-scaleway` | Scaleway | `scaleway/llama-3-70b` |
| `opencode-ovhcloud` | OVHcloud | `ovhcloud/llama-3-70b` |
| `opencode-vercel` | Vercel AI Gateway | `vercel/gpt-4o` |
| `opencode-helicone` | Helicone | `helicone/gpt-4o` |
| `opencode-ionet` | IO.NET | `io-net/llama-3-70b` |
| `opencode-nebius` | Nebius | `nebius/llama-3-70b` |
| `opencode-zenmux` | ZenMux | `zenmux/gpt-4o` |
| `opencode-sap` | SAP AI Core | `sap-ai-core/gpt-4o` |
| `opencode-azure-cognitive` | Azure Cognitive Services | `azure-cognitive-services/gpt-4o` |
| `opencode-venice` | Venice AI | `venice-ai/llama-3-70b` |
| `opencode-ollama-cloud` | Ollama Cloud | `ollama-cloud/llama3` |
| `opencode-llamacpp` | llama.cpp | `llama.cpp/local-model` |

See `examples/agents.toml` for the complete list.

Example agent chain using aliases:
```toml
[agent_chain]
# Try free/cheap providers first, then premium
developer = ["opencode-zen-glm", "opencode-groq", "opencode-deepseek", "opencode-direct-claude", "claude"]
reviewer = ["opencode-groq", "opencode-zen-claude", "claude"]
```

### Provider-Level Fallback (OpenCode)

For cost optimization, Ralph can try different models *within* the same agent before falling back to the next agent. This works with OpenCode's multi-provider support.

```toml
# In ~/.config/ralph-workflow.toml
[agents.opencode]
cmd = "opencode run"
output_flag = "--format json"
model_flag = "-m zai/glm-4.7"  # Default to Z.AI Direct

[agent_chain]
developer = ["opencode", "claude"]
reviewer = ["opencode", "claude"]

# Provider fallback: try these models in order within opencode
[agent_chain.provider_fallback]
opencode = [
  "-m zai/glm-4.7",                 # Z.AI Direct (try first)
  "-m opencode/glm-4.7-free",       # OpenCode Zen free tier (backup)
  "-m anthropic/claude-sonnet-4",   # Direct API (last resort)
]
```

When `zai/glm-4.7` hits rate limits or token exhaustion, Ralph automatically tries `opencode/glm-4.7-free` (Zen free tier), then direct Anthropic API, then finally falls back to the `claude` agent.

**Override provider or model via CLI or environment:**

```bash
# Override just the provider (uses agent's model with new provider)
ralph --developer-provider anthropic
ralph --developer-provider opencode

# Override the full model string
ralph --developer-model "-m opencode/claude-sonnet-4"

# Environment variables
RALPH_DEVELOPER_PROVIDER=anthropic ralph
RALPH_DEVELOPER_MODEL="-m opencode/glm-4.7-free" ralph
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

### Isolation Mode

By default, Ralph Workflow runs in isolation mode, which clears NOTES.md and ISSUES.md between runs. Disable this to preserve these files:

```bash
ralph --no-isolation
```

### Git Workflow and Commits

Ralph Workflow automatically creates git commits throughout the development process:

- **Per-iteration commits**: After each development iteration, if meaningful changes are detected, Ralph creates a commit with an auto-generated message based on the actual changes.
- **Per-review-cycle commits**: After each review-fix cycle, if fixes were applied, Ralph creates a commit with an auto-generated message.
- **Agent isolation**: AI agents are not aware of git operations. Only the orchestrator handles git commits, ensuring clean separation of concerns.
- **Cumulative diffs for reviewers**: Reviewers receive the cumulative diff from the start of the pipeline (stored in `.agent/start_commit`), not per-commit diffs.

**Reset the start commit** (establishes a new baseline for reviewer diffs):

```bash
ralph --reset-start-commit
```

**Skipping commits**: Commits are only created when there are meaningful changes (whitespace-only changes are skipped).

### Plumbing Commands

For scripting and CI/CD:

```bash
# Generate commit message without committing
ralph --generate-commit-msg

# Show the generated message
ralph --show-commit-msg

# Apply commit using generated message
ralph --apply-commit

# Validate setup without running agents
ralph --dry-run

# Resume from last checkpoint
ralph --resume
```

## Files Ralph Workflow Creates

All working files live in `.agent/`:

```
.agent/
├── STATUS.md          # Current status
├── NOTES.md           # Agent notes/scratchpad
├── ISSUES.md          # Issues found during review
├── PLAN.md            # Current iteration plan (deleted after each iteration)
├── commit-message.txt # Generated commit message
├── checkpoint.json    # Pipeline checkpoint (for --resume)
├── last_prompt.txt    # Last prompt sent to agent
├── start_commit       # Baseline commit for cumulative diffs (persists across runs)
└── logs/              # Agent run logs
```

Add to `.gitignore` if you don't want these tracked:
```
.agent/
```

## Troubleshooting

### Common Issues

| Problem | Solution |
|---------|----------|
| "Not a git repository" | Run Ralph Workflow inside a git repo |
| "Agent not found" | Install the agent CLI and ensure it's on your PATH. Ralph Workflow shows installation hints. |
| Garbled/broken output | Set `json_parser = "generic"` for that agent |
| Rate limit errors | Ralph Workflow auto-retries with backoff. Configure fallback agents for faster recovery. |
| Network/connection errors | Check internet, firewall, VPN. Ralph Workflow auto-retries network issues. |
| Authentication errors | Run `<agent> auth` to authenticate, or check your API key. See below for provider-specific guidance. |
| No commit created | Ensure there are meaningful changes; if LLM message generation fails, Ralph uses a fallback commit message and still commits via libgit2 |
| Nothing happening | Try `ralph --debug` to see what's going on |

### OpenCode Provider Authentication

OpenCode supports many providers, each with its own authentication. Use `ralph --list-providers` for the built-in guidance list.

**Common Provider Authentication:**

| Provider | Authentication |
|----------|----------------|
| OpenCode Zen (`opencode/*`) | `opencode auth login` → "OpenCode Zen" |
| Z.AI Direct (`zai/*`) | `opencode auth login` → "Z.AI" |
| Z.AI Coding Plan (`zai/*`) | `opencode auth login` → "Z.AI Coding Plan" |
| Anthropic (`anthropic/*`) | Set `ANTHROPIC_API_KEY` |
| OpenAI (`openai/*`) | Set `OPENAI_API_KEY` |
| Google (`google/*`) | Set `GOOGLE_GENERATIVE_AI_API_KEY` |
| Groq (`groq/*`) | Set `GROQ_API_KEY` |
| DeepSeek (`deepseek/*`) | Set `DEEPSEEK_API_KEY` |
| OpenRouter (`openrouter/*`) | Set `OPENROUTER_API_KEY` |
| Ollama (`ollama/*`) | Run `ollama serve` locally |

**Cloud providers requiring additional setup:**
- **Google Vertex AI**: `gcloud auth application-default login` + set `GOOGLE_VERTEX_PROJECT`
- **Amazon Bedrock**: `aws configure` with appropriate IAM permissions
- **Azure OpenAI**: Set `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_DEPLOYMENT`

**Example authentication error messages:**

```
Error: Authentication failed for OpenCode Zen provider
→ Run: opencode auth login → select 'OpenCode Zen'

Error: Authentication failed for Anthropic provider
→ Run: opencode auth anthropic (set ANTHROPIC_API_KEY)

Error: Authentication failed for Groq provider
→ Run: opencode auth groq (set GROQ_API_KEY)
```

**Agent naming convention:**

Ralph uses a clear naming convention for OpenCode provider aliases:
- `opencode-zen-*` - Routes through OpenCode's Zen gateway (e.g., `opencode-zen-glm`)
- `opencode-zai-*` - Direct Z.AI API access (e.g., `opencode-zai-glm`)
- `opencode-zai-*-codingplan` - Z.AI Coding Plan tier (auth selection; e.g., `opencode-zai-glm-codingplan`)
- `opencode-direct-*` - Direct provider API access (e.g., `opencode-direct-claude`)
- `opencode-{provider}` - Shorthand for common providers (e.g., `opencode-groq`, `opencode-deepseek`)

### Diagnostic Commands

```bash
# Full diagnostic report
ralph --diagnose

# List all available providers and their configuration
ralph --list-providers

# List all configured agents
ralph --list-agents

# List only installed agents
ralph --list-available-agents

# Debug mode to see raw agent output
ralph --debug
```

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

### Can I use Ralph Workflow at work / in my Fortune 500 company?

**Yes, absolutely.** Ralph Workflow is a CLI tool you run locally. Using it doesn't affect the license of your code in any way.

### Does the AGPL license apply to code I generate with Ralph Workflow?

**No.** The AGPL-3.0 license covers *only the Ralph Workflow tool itself* — the Rust source code in this repository. It does **not** apply to:

- Code generated by AI agents that Ralph Workflow orchestrates
- Your PROMPT.md files
- Your project's source code
- Any output, commits, or artifacts Ralph Workflow creates in your repository

The code you create with Ralph Workflow is entirely yours, under whatever license you choose.

### Common AGPL Misconceptions for CLI Tools

| Misconception | Reality |
|---------------|---------|
| "Using an AGPL tool makes my code AGPL" | ❌ False. AGPL covers the tool, not its output. Using `gcc` (GPL) doesn't make your C code GPL. Same principle. |
| "I can't use AGPL tools in a corporate environment" | ❌ False. You can use Ralph Workflow freely. You only need to share source if you *modify and distribute Ralph itself*. |
| "AI-generated code inherits Ralph Workflow's license" | ❌ False. The AI agents (Claude, Codex, etc.) generate the code, not Ralph. Ralph just orchestrates. |
| "My company's legal team will reject this" | Show them this FAQ! Ralph Workflow is a local dev tool like `make` or `git`. |

### What would require me to share source code?

Only if you **modify Ralph Workflow itself** and **distribute your modified version** (or provide it as a network service). Normal usage — running Ralph to build your projects — requires nothing from you.

### Is there a commercial/enterprise license available?

For now, no. The AGPL is the only license. But again, you can freely use Ralph Workflow in any commercial setting without concern. If you need a different license for redistribution purposes, open an issue.

### TL;DR

**Use Ralph Workflow anywhere. Your code stays yours. The AGPL only covers Ralph's source code, not anything you create with it.**

## Contributing

Contributions welcome!

1. Fork the repository
2. Create a feature branch
3. Run tests: `cargo test`
4. Run lints: `cargo clippy && cargo fmt --check`
5. Submit a pull request

## License

AGPL-3.0. See [LICENSE](LICENSE).
