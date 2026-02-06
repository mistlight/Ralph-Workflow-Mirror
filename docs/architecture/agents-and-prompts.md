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

### Workspace-Rooted Path Resolution

All prompt generation functions accept a `&dyn Workspace` parameter and use `workspace.absolute_str()` to generate absolute paths for output files. This ensures prompts embed paths rooted at the workspace directory, not the process's current working directory (`std::env::current_dir()`).

**Why this matters:** In multi-worktree setups or isolation mode, the process CWD may differ from the workspace root. Using CWD-based paths would cause agents to write output XML to the wrong directory, triggering XSD retry loops that cannot converge.

**Implementation:**
- All `prompt_*_xml_with_*` functions take `workspace: &dyn Workspace`
- XSD retry functions check for missing schema files and emit diagnostics including workspace root
- Effect handlers pass `ctx.workspace` to prompt generation functions

### Template Rendering Error Handling

Prompt file writes in effect handlers are non-fatal. If template rendering or file writes fail:

1. A warning is logged with the error details
2. The pipeline continues advancing to the next effect
3. Loop recovery mechanisms ensure convergence even if prompts are malformed

This prevents template errors from terminating the pipeline. The loop detection system will trigger recovery if the same effect repeats too many times.

**Key principle:** Pipeline advancement must never be blocked by prompt generation failures. The reducer's loop recovery system handles convergence.

### Agent Chain Normalization

Immediately before each `Invoke*Agent` effect execution, the effect handler normalizes agent chain state to ensure deterministic agent selection:

- Verify current role matches expected role for the phase
- Apply session ID policy based on retry mode:
  - XSD retry: preserve session ID (same conversation)
  - Same-agent retry: clear session ID (fresh conversation)
  - Normal invocation: use session policy from reducer state

This normalization ensures checkpoint replay produces identical agent selection given the same reducer state, which is critical for deterministic recovery.

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
