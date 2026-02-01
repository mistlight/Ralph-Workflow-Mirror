# Effect System Architecture

This document defines the effect system and reducer architecture used in Ralph for managing side effects and application state. This architecture is **non-negotiable** and must be followed by all code changes.

## Design Philosophy

Ralph separates **pure logic** from **side effects** to enable testability:

- **Pure functions** compute state transitions and determine what effects to execute
- **Effect handlers** execute side effects (I/O, git, filesystem) and report results
- **Reducers** orchestrate the flow: state → effect → event → new state

This separation allows testing business logic without real I/O by injecting mock handlers.

## Overview

Ralph uses two distinct effect systems that operate at different layers of the application:

| Layer | Effect Type | Handler | Filesystem Access | When Used |
|-------|-------------|---------|-------------------|-----------|
| CLI | `AppEffect` | `AppEffectHandler` | `std::fs` directly | Before repo root known |
| Pipeline | `Effect` | `EffectHandler` | `ctx.workspace` | After repo root known |

These layers are **strictly separated** and must not be mixed.

## Layer 1: AppEffect (CLI Layer)

### Location

- `ralph-workflow/src/app/effect.rs` - Effect enum and trait
- `ralph-workflow/src/app/effect_handler.rs` - Real implementation
- `ralph-workflow/src/app/mock_effect_handler.rs` - Test implementation

### Purpose

Handles side effects during CLI initialization, **before** the repository root is discovered and before a `Workspace` can be created.

### Characteristics

1. **No Workspace access** - Cannot use `Workspace` trait because it doesn't exist yet
2. **No PhaseContext** - Operates outside the pipeline context
3. **Direct std::fs** - `RealAppEffectHandler` uses `std::fs` directly
4. **Own mock filesystem** - `MockAppEffectHandler` has `HashMap<PathBuf, String>`

### Handler Signature

```rust
pub trait AppEffectHandler {
    fn execute(&mut self, effect: AppEffect) -> AppEffectResult;
}
```

Note: No context parameter. The handler operates standalone.

### Operations

```rust
pub enum AppEffect {
    // Working Directory
    SetCurrentDir { path: PathBuf },

    // Filesystem (uses std::fs in RealAppEffectHandler)
    WriteFile { path: PathBuf, content: String },
    ReadFile { path: PathBuf },
    DeleteFile { path: PathBuf },
    CreateDir { path: PathBuf },
    PathExists { path: PathBuf },
    SetReadOnly { path: PathBuf, readonly: bool },

    // Git primitives
    GitRequireRepo,
    GitGetRepoRoot,
    GitGetHeadOid,
    GitDiff,
    GitDiffFrom { start_oid: String },
    GitDiffFromStart,
    GitSnapshot,
    GitAddAll,
    GitCommit { message: String, user_name: Option<String>, user_email: Option<String> },
    GitSaveStartCommit,
    GitResetStartCommit,
    GitRebaseOnto { upstream_branch: String },
    GitGetConflictedFiles,
    GitContinueRebase,
    GitAbortRebase,
    GitGetDefaultBranch,
    GitIsMainBranch,

    // Environment
    GetEnvVar { name: String },
    SetEnvVar { name: String, value: String },

    // Logging
    LogInfo { message: String },
    LogSuccess { message: String },
    LogWarn { message: String },
    LogError { message: String },
}
```

### When to Use

- CLI argument handling (`--version`, `--help`, `--diagnose`, `--init`)
- Repository discovery (`GitRequireRepo`, `GitGetRepoRoot`)
- Pre-pipeline validation
- Any operation **before** `WorkspaceFs` can be created

### Testing

Use `MockAppEffectHandler`:

```rust
#[test]
fn test_cli_operation() {
    let mut handler = MockAppEffectHandler::new()
        .with_file("PROMPT.md", "# Goal\n...")
        .with_head_oid("abc123");
    
    // Execute CLI operation
    run_ralph_cli_with_handler(&["--diagnose"], executor, config, &mut handler).unwrap();
    
    // Verify effects
    assert!(handler.was_executed(&AppEffect::GitRequireRepo));
}
```

**No TempDir needed** - `MockAppEffectHandler` has its own in-memory filesystem.

## Layer 2: Effect (Pipeline Layer)

### Location

- `ralph-workflow/src/reducer/effect.rs` - Effect enum and trait
- `ralph-workflow/src/reducer/handler.rs` - Main implementation
- `ralph-workflow/src/reducer/mock_effect_handler.rs` - Test implementation

### Purpose

Handles side effects during pipeline execution, **after** the repository root is known and `WorkspaceFs` has been created.

### Characteristics

1. **Has Workspace access** - Via `ctx.workspace` in `PhaseContext`
2. **Has PhaseContext** - Full access to config, registry, logger, etc.
3. **Uses ctx.workspace** - `MainEffectHandler` uses workspace for file ops
4. **Returns PipelineEvent** - Effects produce events for state machine

### Handler Signature

```rust
pub trait EffectHandler<'ctx> {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext<'_>) -> Result<PipelineEvent>;
}
```

Note: Has `PhaseContext` parameter which includes `workspace: &dyn Workspace`.

### Operations

```rust
pub enum Effect {
    // Agent operations
    AgentInvocation { role: AgentRole, agent: String, model: Option<String>, prompt: String },
    InitializeAgentChain { role: AgentRole },

    // Phase operations
    GeneratePlan { iteration: u32 },
    RunDevelopmentIteration { iteration: u32 },
    RunReviewPass { pass: u32 },
    RunFixAttempt { pass: u32 },

    // Git operations (high-level)
    RunRebase { phase: RebasePhase, target_branch: String },
    ResolveRebaseConflicts { strategy: ConflictStrategy },

    // Commit operations
    PrepareCommitPrompt,
    InvokeCommitAgent,
    ExtractCommitXml,
    ValidateCommitXml,
    ApplyCommitMessageOutcome,
    ArchiveCommitXml,
    CreateCommit { message: String },
    SkipCommit { reason: String },

    // Pipeline management
    ValidateFinalState,
    SaveCheckpoint { trigger: CheckpointTrigger },
    CleanupContext,
    RestorePromptPermissions,
}
```

### When to Use

- All pipeline phase execution
- Operations that need `Workspace`
- Operations that produce `PipelineEvent`s
- Phase boundary operations (like `RestorePromptPermissions`)

### Testing

Use `MemoryWorkspace` via `PhaseContext`:

```rust
#[test]
fn test_cleanup_context() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "# Plan")
        .with_file(".agent/ISSUES.md", "# Issues");
    
    let mut ctx = create_test_phase_context(&workspace);
    let mut handler = MainEffectHandler::new(state);
    
    handler.cleanup_context(&mut ctx).unwrap();
    
    assert!(!workspace.exists(Path::new(".agent/PLAN.md")));
}
```

**No TempDir needed** - `MemoryWorkspace` is in-memory.

## Layer 3: Workspace (Filesystem Abstraction)

### Location

- `ralph-workflow/src/workspace.rs`

### Purpose

Abstracts filesystem operations relative to the repository root.

### Implementations

| Type | Usage | Storage |
|------|-------|---------|
| `WorkspaceFs` | Production | Real filesystem via `std::fs` |
| `MemoryWorkspace` | Testing | In-memory `HashMap` |

### Access

Workspace is accessed **only** through `PhaseContext`:

```rust
pub struct PhaseContext<'a> {
    pub workspace: &'a dyn Workspace,
    // ... other fields
}
```

### Operations

```rust
pub trait Workspace: Send + Sync {
    fn root(&self) -> &Path;
    fn read(&self, relative: &Path) -> io::Result<String>;
    fn write(&self, relative: &Path, content: &str) -> io::Result<()>;
    fn exists(&self, relative: &Path) -> bool;
    fn remove(&self, relative: &Path) -> io::Result<()>;
    fn create_dir_all(&self, relative: &Path) -> io::Result<()>;
    fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    // ... more operations
}
```

## Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────────┐
│                          CLI Entry Point                              │
│  main() -> cli::run() -> app::run()                                  │
└──────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────────┐
│                    AppEffect Layer (app/effect.rs)                    │
│                                                                       │
│  Purpose: Pre-pipeline CLI operations                                │
│  Filesystem: std::fs directly (NO Workspace)                         │
│  Context: NONE                                                       │
│                                                                       │
│  ┌─────────────────────┐    ┌─────────────────────┐                 │
│  │ RealAppEffectHandler│    │ MockAppEffectHandler │                 │
│  │   uses std::fs      │    │   HashMap<Path,Str>  │                 │
│  └─────────────────────┘    └─────────────────────┘                 │
│                                                                       │
│  Key operation: GitGetRepoRoot -> discovers repo root                │
└──────────────────────────────────────────────────────────────────────┘
                                   │
                                   │ repo root discovered
                                   │ WorkspaceFs created
                                   ▼
┌──────────────────────────────────────────────────────────────────────┐
│                   Effect Layer (reducer/effect.rs)                    │
│                                                                       │
│  Purpose: Pipeline execution operations                              │
│  Filesystem: ctx.workspace (Workspace trait)                         │
│  Context: PhaseContext with workspace, config, registry, etc.        │
│                                                                       │
│  ┌─────────────────────┐    ┌─────────────────────┐                 │
│  │ MainEffectHandler   │    │ MockEffectHandler    │                 │
│  │   ctx.workspace     │    │   synthetic events   │                 │
│  └─────────────────────┘    └─────────────────────┘                 │
└──────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌──────────────────────────────────────────────────────────────────────┐
│                    Workspace Layer (workspace.rs)                     │
│                                                                       │
│  Purpose: Filesystem abstraction relative to repo root               │
│                                                                       │
│  ┌─────────────────────┐    ┌─────────────────────┐                 │
│  │ WorkspaceFs         │    │ MemoryWorkspace      │                 │
│  │   wraps std::fs     │    │   HashMap storage    │                 │
│  └─────────────────────┘    └─────────────────────┘                 │
└──────────────────────────────────────────────────────────────────────┘
```

## Rules (Non-Negotiable)

### 1. AppEffect Cannot Use Workspace

```rust
// WRONG - AppEffectHandler has no access to Workspace
impl AppEffectHandler for MyHandler {
    fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
        let workspace = ???;  // ERROR: No workspace available
    }
}

// CORRECT - RealAppEffectHandler uses std::fs (this is the ONLY place besides WorkspaceFs)
impl AppEffectHandler for RealAppEffectHandler {
    fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
        match effect {
            AppEffect::ReadFile { path } => {
                std::fs::read_to_string(path)  // Direct std::fs allowed HERE ONLY
            }
        }
    }
}
```

### 2. Effect Must Use ctx.workspace

```rust
// WRONG - Using std::fs in EffectHandler
impl EffectHandler for MyHandler {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext) -> Result<PipelineEvent> {
        std::fs::write(".agent/PLAN.md", content);  // ERROR: Bypass workspace
    }
}

// CORRECT - Use ctx.workspace
impl EffectHandler for MainEffectHandler {
    fn execute(&mut self, effect: Effect, ctx: &mut PhaseContext) -> Result<PipelineEvent> {
        ctx.workspace.write(Path::new(".agent/PLAN.md"), content)?;  // Via workspace
    }
}
```

### 3. Never Mix Mock Systems

```rust
// WRONG - Trying to share state between layers
let app_handler = MockAppEffectHandler::new().with_file("test.txt", "content");
let workspace = MemoryWorkspace::new_test();
// These DO NOT share filesystem state!

// CORRECT - Test each layer separately
// CLI layer test:
let mut handler = MockAppEffectHandler::new().with_file("test.txt", "content");
run_cli_with_handler(&mut handler);

// Pipeline layer test:
let workspace = MemoryWorkspace::new_test().with_file("test.txt", "content");
let ctx = create_context(&workspace);
run_phase(&ctx);
```

### 4. Tests Must Use Mock Systems (NO raw std::fs)

**Tests MUST use the same effect systems as production code:**

| Test Type | Use This | NOT This |
|-----------|----------|----------|
| CLI/Pre-pipeline tests | `MockAppEffectHandler` | `std::fs`, `TempDir` |
| Pipeline tests | `MemoryWorkspace` | `std::fs`, `TempDir` |
| Cross-layer integration | `TempDir` acceptable | - |

```rust
// CLI-only test: Use MockAppEffectHandler
#[test]
fn test_cli_diagnose() {
    let mut handler = MockAppEffectHandler::new()
        .with_file("PROMPT.md", "content")
        .with_head_oid("abc123");
    run_ralph_cli_with_handler(&["--diagnose"], &mut handler);
    // NO TempDir, NO std::fs
}

// Pipeline-only test: Use MemoryWorkspace
#[test]
fn test_cleanup_context() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/PLAN.md", "content");
    let ctx = create_context(&workspace);
    // NO TempDir, NO std::fs
}

// ONLY for tests that exercise BOTH layers together
#[test]
fn test_full_workflow() {
    let dir = TempDir::new().unwrap();  // Acceptable - crosses both layers
}
```

**Why no raw std::fs in tests?**
- Tests should exercise the same code paths as production
- If production uses `AppEffect`, tests should use `MockAppEffectHandler`
- If production uses `workspace`, tests should use `MemoryWorkspace`
- Raw `std::fs` in tests means you're not testing the effect system

### 5. Phase Boundaries Use Effect/Event

Operations at phase boundaries (transitions between pipeline phases) must use the `Effect`/`PipelineEvent` pattern:

```rust
// WRONG - Direct call at phase boundary
fn finalize_pipeline(ctx: &mut PhaseContext) {
    make_prompt_writable(ctx.workspace);  // Direct call
}

// CORRECT - Effect/Event pattern
// 1. Define effect
pub enum Effect {
    RestorePromptPermissions,
}

// 2. Handler executes via workspace
fn restore_prompt_permissions(&self, ctx: &mut PhaseContext) -> Result<PipelineEvent> {
    make_prompt_writable_with_workspace(ctx.workspace);
    Ok(PipelineEvent::PromptPermissionsRestored)
}

// 3. State machine transitions on event
fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    match event {
        PipelineEvent::PromptPermissionsRestored => PipelineState {
            phase: PipelinePhase::Complete,
            ..state
        }
    }
}
```

### 6. Within-Phase Operations Use ctx.workspace Directly

Operations inside a phase (not at boundaries) can use `ctx.workspace` directly:

```rust
// CORRECT - Within-phase file operation
fn run_development_iteration(ctx: &mut PhaseContext) {
    // Reading plan during development - within phase, use workspace directly
    let plan = ctx.workspace.read(Path::new(".agent/PLAN.md"))?;
    
    // Writing status during development - within phase, use workspace directly  
    ctx.workspace.write(Path::new(".agent/status.txt"), "running")?;
}
```

## Reducer Architecture

The pipeline layer uses a **reducer pattern** to manage complex state transitions. This pattern separates concerns into distinct components.

### Core Concepts

**State**: Immutable snapshot of pipeline progress (current phase, iteration counts, flags).

**Event**: Notification that something happened (agent completed, review passed, commit created).

**Effect**: Description of a side effect to execute (run agent, create commit, cleanup files).

**Reducer**: Pure function that computes new state from current state and event.

**Orchestrator**: Determines which effect to execute based on current state.

**Handler**: Executes effects and produces events.

### Data Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Event Loop                                   │
│                                                                      │
│   ┌─────────┐    ┌─────────────┐    ┌─────────┐    ┌──────────┐   │
│   │  State  │───▶│ Orchestrator│───▶│  Effect │───▶│  Handler │   │
│   └─────────┘    └─────────────┘    └─────────┘    └──────────┘   │
│        ▲                                                  │         │
│        │                                                  ▼         │
│        │         ┌─────────────┐                   ┌──────────┐    │
│        └─────────│   Reducer   │◀──────────────────│   Event  │    │
│                  └─────────────┘                   └──────────┘    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

1. **Orchestrator** examines current state, returns next effect to execute
2. **Handler** executes the effect, returns an event describing the outcome
3. **Reducer** computes new state from current state + event
4. Loop continues until orchestrator returns "complete"

### Why This Pattern?

**Testability**: Each component can be tested in isolation:
- Test reducer with synthetic events (no I/O)
- Test orchestrator with various states (no I/O)
- Test handler with mock workspace (controlled I/O)

**Predictability**: State transitions are deterministic given the same events.

**Debuggability**: Event log shows exactly what happened and why.

**Resumability**: State can be serialized (checkpointed) and resumed later.

### Components

#### PipelineState

Tracks pipeline progress:
- Current phase (Planning, Development, Review, Commit, etc.)
- Iteration counters
- Completion flags
- Error state

State is **immutable** - reducers return new state instances.

#### PipelineEvent

Signals outcomes:
- `PlanGenerated` - Planning phase completed
- `DevelopmentIterationCompleted` - One dev iteration done
- `ReviewCompleted { passed: bool }` - Review finished
- `CommitCreated { oid: String }` - Commit succeeded
- `PromptPermissionsRestored` - Finalization done

Events are **facts** - they describe what happened, not commands.

#### Effect

Describes operations to perform:
- `GeneratePlan` - Run planning phase
- `RunDevelopmentIteration` - Execute one dev iteration
- `RunReviewPass` - Execute review
- `CreateCommit` - Make a git commit
- `RestorePromptPermissions` - Restore file permissions

Effects are **intentions** - they describe what should happen.

#### Orchestrator

Pure function: `State → Option<Effect>`

Decides what to do next based on state. Returns `None` when complete.

#### Reducer

Pure function: `(State, Event) → State`

Computes state transitions. No side effects.

#### EffectHandler

Impure: `(Effect, Context) → Event`

Executes effects and reports results. This is where I/O happens.

### Testing Strategy

**Unit test reducers** with synthetic events:

```rust
#[test]
fn test_review_passed_advances_to_commit() {
    let state = PipelineState { phase: Phase::Review, ... };
    let event = PipelineEvent::ReviewCompleted { passed: true };
    
    let new_state = reduce(state, event);
    
    assert_eq!(new_state.phase, Phase::Commit);
}
```

**Unit test orchestrator** with various states:

```rust
#[test]
fn test_orchestrator_starts_development_after_planning() {
    let state = PipelineState { phase: Phase::Development, iteration: 1, ... };
    
    let effect = orchestrate(&state);
    
    assert!(matches!(effect, Some(Effect::RunDevelopmentIteration { iteration: 1 })));
}
```

**Integration test handlers** with mock workspace:

```rust
#[test]
fn test_handler_creates_commit() {
    let workspace = MemoryWorkspace::new_test();
    let mut ctx = create_context(&workspace);
    let mut handler = MainEffectHandler::new();
    
    let event = handler.execute(Effect::CreateCommit { message: "test" }, &mut ctx)?;
    
    assert!(matches!(event, PipelineEvent::CommitCreated { .. }));
}
```

### Checkpoint and Resume

State is serializable, enabling:
- **Interruption handling**: Save state on Ctrl+C
- **Resume after crash**: Reload checkpoint and continue
- **Debugging**: Inspect saved state to understand failures

Checkpoints store:
- `PipelineState` (current progress)
- `RunContext` (iteration history)
- `ExecutionHistory` (detailed event log)

---
## Where std::fs is Allowed

### Primary Locations (Handler Implementations)

| File | Purpose |
|------|---------|
| `app/effect_handler.rs` | `RealAppEffectHandler` - implements `AppEffect` operations |
| `workspace.rs` | `WorkspaceFs` - implements `Workspace` trait |

### Domain-Specific Abstractions

These files contain "Real*" implementations that wrap `std::fs` behind traits:

| File | Abstraction | Purpose |
|------|-------------|---------|
| `config/path_resolver.rs` | `RealConfigEnvironment` | Config file loading |
| `agents/opencode_api/cache.rs` | `RealCacheEnvironment` | API catalog caching |

### Legitimate Exceptions

| File | Reason |
|------|--------|
| `files/protection/monitoring.rs` | Background thread monitoring real filesystem changes |
| `git_helpers/hooks.rs` | Hook installation operates on `.git/hooks/` which is outside workspace root by design |
| `git_helpers/wrapper.rs` | Creates temp directory for PATH manipulation (must be real filesystem path) |
| `git_helpers/rebase.rs` | Operates on `.git/` internals (rebase state, worktree config) |
| `logger/output.rs` | `with_log_file()` is for CLI layer (pre-workspace); `with_workspace_log()` exists for pipeline |
| `checkpoint/file_state.rs` | `capture_file_impl()`/`validate_file_impl()` called from CLI layer before workspace exists |

### Crate-Internal Methods (Not Called From Production Code)

The following files have `std::fs` functions that are crate-internal for legacy/external use.
Production code already uses `_with_workspace` variants:

- `files/io/integrity.rs` - `write_file_atomic()`, `verify_file_not_corrupted()`, `check_filesystem_ready()` are crate-internal; production uses `*_with_workspace` variants

**EVERYWHERE ELSE: std::fs is FORBIDDEN**

- Pre-pipeline code → Must use `AppEffect` or domain-specific environment trait
- Pipeline code → Must use `ctx.workspace`  
- Tests → Must use `MockAppEffectHandler`, `MemoryWorkspace`, or `Memory*Environment`

---


## Summary

| Question | Answer |
|----------|--------|
| Where is std::fs allowed? | In handler impls (`RealAppEffectHandler`, `WorkspaceFs`), domain abstractions (`Real*Environment`), and documented exceptions |
| Can AppEffect use Workspace? | **NO** - Workspace doesn't exist at CLI layer |
| Can Effect use std::fs? | **NO** - Must use ctx.workspace |
| Can tests use std::fs? | **NO** - Must use `MockAppEffectHandler`, `MemoryWorkspace`, or `Memory*Environment` |
| Can MockAppEffectHandler and MemoryWorkspace share state? | **NO** - Separate systems |
| When is TempDir acceptable? | **Only** for tests crossing both layers |
| When to use Effect/Event vs direct workspace? | Phase boundaries = Effect/Event, within-phase = direct |
| What does the reducer do? | Pure state transitions: `(State, Event) → State` |
| What does the orchestrator do? | Decides next effect: `State → Option<Effect>` |
| What does the handler do? | Executes effects: `(Effect, Context) → Event` |
| Why separate these? | Testability - test logic without I/O |
