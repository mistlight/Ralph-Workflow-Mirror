# Agents and Prompting Architecture

This document explains how Ralph selects and runs AI agents, how prompts are built, and where streaming output parsing fits.

## Agent Registry and Configuration

Agents are configured and discovered through `AgentRegistry`:

- Registry + lookup: `ralph-workflow/src/agents/registry.rs`
- Agent config types: `ralph-workflow/src/agents/config/`
- JSON parser selection: `ralph-workflow/src/agents/parser.rs` (via `JsonParserType`)

Inputs (in increasing priority):

1. Built-in agent defaults (claude, codex, opencode, etc.)
2. Unified config (`~/.config/ralph-workflow.toml`)
3. Environment overrides (`RALPH_*`)
4. Programmatic registration (tests / embedding)

Key config fields you will see in code:

- command + flags (including a provider-specific "output as JSON stream" flag)
- `yolo_flag` / non-interactive flags (required for unattended runs)
- `can_commit` (used to validate that the chosen agent can safely operate)
- `json_parser` / `JsonParserType` (how NDJSON is interpreted)

## Agent Chains, Retries, and Fallback

The pipeline is built around *agent chains* (fallback lists):

- Chain config: `ralph-workflow/src/agents/fallback/`
- Reducer-managed position/cycle: `ralph-workflow/src/reducer/state/` (agent chain fields)

Important principle:

- Retry/fallback is reducer-visible state, not hidden loops in handlers.

Error classification used by fallback policy lives in:

- `ralph-workflow/src/agents/error.rs`
- `ralph-workflow/src/reducer/fault_tolerant_executor.rs`

## Process Execution Boundary

Spawning external agent CLIs is an architectural boundary:

- Trait: `ProcessExecutor` (`ralph-workflow/src/executor/executor_trait.rs`)
- Production: `RealProcessExecutor` (`ralph-workflow/src/executor/real.rs`)
- Tests: `MockProcessExecutor` (`ralph-workflow/src/executor/mock.rs`, behind `test-utils`)

This boundary makes agent execution deterministic and testable without spawning real processes.

## Prompt Generation

There are two distinct template systems:

1. PROMPT.md "work guides" (end-user templates)
   - Embedded templates: `ralph-workflow/src/templates/mod.rs`
   - Source files: `ralph-workflow/templates/prompts/*.md`
2. Agent prompts (system prompts used at runtime)
   - Prompt engine: `ralph-workflow/src/prompts/`
   - Text templates: `ralph-workflow/prompts/templates/`

Agent prompts use a small template language (variables, partials) and are rendered with context derived from pipeline state, config, and captured artifacts.

## Where Streaming Output Parsing Hooks In

Agent CLIs typically emit streaming NDJSON. Ralph:

- spawns the agent process through `ProcessExecutor`
- parses NDJSON through provider-specific parsers
- renders output based on terminal capabilities

Code locations:

- Streaming parser core: `ralph-workflow/src/json_parser/`
- Provider parsers: `ralph-workflow/src/json_parser/{claude,codex,gemini,opencode}/`
- Contract enforcement + dedup: `ralph-workflow/src/json_parser/streaming_state/`

See `streaming-and-parsers.md` for the detailed contract.
