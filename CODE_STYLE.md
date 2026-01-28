# Code Style Guide

## Important Rules for this Project
- A file should at most be 1000 lines long, if it's longer, it's time to break it up. It is also a sign of smelly code and refactor is in order
- A file should do one conceptual job. If you need a paragraph to explain what the file does, it’s doing too much.
- If you need comments to explain what the code does, rewrite it.
- If nesting goes past 3–4 levels, refactor.
- Prefer early returns over deep if trees.
- Function should be at most 100 lines long but if it's longer than 50 lines you should start considering refactoring, if it's like barely passing lint at 97 your PR may not be accepted and you will be asked to refactor.
- Avoid clever code. Boring is good.

## Architecture

Ralph uses an **event-sourced reducer architecture**. See [effect-system.md](docs/architecture/effect-system.md).

```
State → Orchestrator → Effect → Handler → Event → Reducer → State
```

| Component | Pure? | Role |
|-----------|-------|------|
| `PipelineState` | Yes | Immutable progress snapshot |
| `reduce()` | Yes | `(State, Event) → State` |
| `determine_next_effect()` | Yes | `State → Effect` |
| `EffectHandler` | No | Executes effects, produces events |

**Business logic → reducers/orchestration (pure). I/O → handlers (impure).**

### Two Effect Layers

| Layer | When | Filesystem |
|-------|------|------------|
| `AppEffect` | Before repo root known | `std::fs` directly |
| `Effect` | After repo root known | `ctx.workspace` |

Never mix. AppEffect cannot use Workspace; Effect cannot use `std::fs`.

---

## Design Principles

- **High cohesion**: Code that changes together lives together
- **Single responsibility**: One job per module/type
- **Explicit boundaries**: Separate domain, orchestration, I/O, CLI
- **Safe APIs**: Types encode invariants, hard to misuse
- **Minimal surface**: Private by default

---

## Code Guidelines

| Aspect | Rule |
|--------|------|
| Function size | < 30 lines |
| Module size | < 300 lines |
| Test file size | < 1000 lines |
| Nesting depth | Max 3 levels |
| Magic numbers | Extract to named constants |
| Abbreviations | Only universal (`ctx`, `cfg`) |

- Early returns over nested conditionals
- `Result` + `?` with context; no `unwrap()`/`expect()` in production
- DRY, but duplication beats wrong abstraction

---

## Comments

**Comments explain *why*, not *what*.**

| Required | Forbidden |
|----------|-----------|
| Module-level `//!`: purpose, when to use | Restating code |
| Public items `///`: what, params, errors | Commented-out code |
| Non-obvious logic: why this approach | TODO without issue number |
| Workarounds: link to issue | |

```rust
/// Executes the next pipeline effect based on current state.
///
/// # Errors
/// Returns error if effect execution fails (agent crash, I/O error).
pub fn execute_next(state: &PipelineState, handler: &mut impl EffectHandler) -> Result<PipelineEvent>
```

Comments must stand alone without external docs.

---

## Dead Code

Dead code = not referenced by production, only by tests, "for future use", unused feature flags.

Handle by: delete it, implement the feature now, gate behind active feature flag, move to `examples/`.

**Never `#[allow(dead_code)]`** - see [AGENTS.md](AGENTS.md).

---

## Testing

Three tiers with strict boundaries:

| Tier | Command | What | Mocks? |
|------|---------|------|--------|
| Unit | `cargo test -p ralph-workflow --lib` | Pure logic | None needed |
| Integration | `cargo test -p ralph-workflow-tests` | Component interactions | `MemoryWorkspace`, `MockProcessExecutor` |
| System | `cargo test -p ralph-workflow-tests --test ralph-workflow-system-tests` | Real filesystem/git | None (real I/O) |

See [INTEGRATION_TESTS.md](tests/INTEGRATION_TESTS.md), [SYSTEM_TESTS.md](tests/system_tests/SYSTEM_TESTS.md).

### Rules

- **Black-box**: Test through public APIs, assert observable outcomes
- **Behavior over implementation**: Tests survive internal refactors
- **Mock at boundaries only**: Filesystem, network, processes - never domain logic
- **Fix implementation, not tests**: Unless expected behavior intentionally changed

### Workspace Abstraction

| Forbidden | Required |
|-----------|----------|
| `std::fs::read_to_string()` | `workspace.read()` |
| `std::fs::write()` | `workspace.write()` |
| `path.exists()` | `workspace.exists()` |

Exceptions: `WorkspaceFs` impl, `RealAppEffectHandler`, bootstrap code.

---

## Principles

- Tests don't legitimize production code - if code exists only for tests, delete both
- Good tests protect behavior, not implementation
- Dead code is liability, not asset
- Prefer deletion over suppression
- Pure logic is testable logic - push I/O to boundaries
