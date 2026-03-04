# Testing Guide (Canonical)

**Single source of truth for all test strategy, rules, and patterns.**
Read before writing or modifying any test.

---

## Test Pyramid

```
         ▲ git2-system-tests (tests/system_tests/)
         │ Real git, real filesystem, libgit2 — serial (#[serial] required)
         │ NOT in CI — run manually only
         │
        ██ process-system-tests (tests/process_system_tests/)
        │  Real OS processes / PATH — NO libgit2 — parallel
        │  NOT in CI — run manually only
        │
       ████ Integration tests (cargo test -p ralph-workflow-tests --test integration_tests)
       │    MemoryWorkspace + MockProcessExecutor, no real I/O — parallel
       │    Target: < 60 s wall-clock
       │
   ████████ Unit tests (cargo test -p ralph-workflow --lib)
            Pure reducers, domain logic, parsers — parallel
            Target: subsecond per test, total < 10 s
```

### Tier summary

| Tier | Binary | Parallelism | CI? | Run command |
|------|--------|-------------|-----|-------------|
| Unit | `ralph-workflow --lib` | parallel | yes | `cargo test -p ralph-workflow --lib --all-features` |
| Integration | `integration_tests` | parallel | yes | `cargo test -p ralph-workflow-tests --test integration_tests` |
| Process system | `process-system-tests` | parallel | **no** | `cargo test -p ralph-workflow-tests --test process-system-tests` |
| git2 system | `git2-system-tests` | serial (libgit2) | **no** | `cargo test -p ralph-workflow-tests --test git2-system-tests` |

---

## Parallelism Rules

**All tests are parallel by default.**
The only justified exception is the `git2-system-tests` binary, where `#[serial]` is required on every test because concurrent `git2::Repository` drops from multiple threads trigger thread-unsafe libgit2 global shutdown (SIGABRT).

| Location | `#[serial]` | Reason |
|----------|-------------|--------|
| `ralph-workflow/src/` unit tests | **BANNED** | Use env-injection |
| `tests/integration_tests/` | **BANNED** | Use MemoryWorkspace / MockProcessExecutor |
| `tests/process_system_tests/` | **BANNED** | Use module-local Mutex for PATH mutation |
| `tests/system_tests/` | **REQUIRED** | libgit2 global reference counter (SIGABRT) |

> **Note on `signal_cleanup`:** The `signal_cleanup` module resides in `system_tests/` (not `process_system_tests/`) because its tests call `init_git_repo()` to set up a real git repository for SIGINT/cleanup verification. This is a direct libgit2 dependency, so `#[serial]` is required and the serial binary is the correct location.

If a test seems to require `#[serial]`, refactor the implementation first (dependency injection, env-injection pattern) before escalating to a system test.

---

## Env-Injection Pattern

The most common `#[serial]` cause: production code calling `std::env::var` directly.

**BEFORE — requires `#[serial]`:**
```rust
#[test]
#[serial]
fn test_cloud_disabled_by_default() {
    std::env::remove_var("RALPH_CLOUD_MODE");
    let cfg = CloudConfig::from_env();
    assert!(!cfg.enabled);
}
```

**AFTER — parallel-safe, no `#[serial]`:**
```rust
#[test]
fn test_cloud_disabled_by_default() {
    let cfg = CloudConfig::from_env_fn(|_| None);
    assert!(!cfg.enabled);
}

#[test]
fn test_cloud_enabled_with_full_env() {
    let env = [("RALPH_CLOUD_MODE", "true"), ("RALPH_CLOUD_API_URL", "https://x"), ...];
    let cfg = CloudConfig::from_env_fn(|k| env.iter().find(|(key, _)| *key == k).map(|(_, v)| (*v).to_string()));
    assert!(cfg.enabled);
}
```

**Production pattern** — always provide both forms:
```rust
pub fn from_env_fn(get: impl Fn(&str) -> Option<String>) -> Self { /* impl */ }
pub fn from_env() -> Self { Self::from_env_fn(|k| std::env::var(k).ok()) }
```

For unit tests (`src/`), use `MemoryConfigEnvironment::with_env_var()` — no `#[serial]` needed.

---

## Test Doubles

Use the right double. Never mock domain logic — only mock at architectural boundaries.

| Double | When | Codebase Example |
|--------|------|-----------------|
| **Fake** | Working implementation, in-memory | `MemoryWorkspace` |
| **Stub** | Returns canned values | `MockProcessExecutor` with preconfigured results |
| **Spy** | Records calls for assertion | `TestPrinter` capturing output |
| **Mock** | Pre-programmed expectations | `MockAppEffectHandler` |
| **Dummy** | Placeholder, never used | Empty `MemoryWorkspace` for pure reducer tests |

**Required doubles:**

| Operation | Use | Never use |
|-----------|-----|-----------|
| File I/O | `MemoryWorkspace` | `TempDir`, `std::fs::*` |
| Process execution | `MockProcessExecutor` | `std::process::Command` |
| Parser output | `TestPrinter` / `VirtualTerminal` | Direct string inspection |

---

## Architecture Boundaries

Ralph uses two effect layers:

| Layer | Mock With | Tests |
|-------|-----------|-------|
| `AppEffect` (CLI setup) | `MockAppEffectHandler` | integration tests |
| `Effect` (pipeline) | `MemoryWorkspace` + `MockProcessExecutor` | integration tests |
| Real OS / libgit2 | none (real implementations) | system tests only |

---

## AAA Structure

Every test: one behavior, Arrange-Act-Assert.

```rust
// Arrange
let state = PipelineState::initial(5, 2);
let event = PipelineEvent::developer_exhausted();

// Act
let new_state = reduce(state, event);

// Assert
assert_eq!(new_state.phase, PipelinePhase::Review);
```

Keep Arrange short. If setup exceeds 10 lines, extract a named builder helper.

---

## Test Naming

Name tests by **observable behavior**, not implementation:

| ✅ Good | ❌ Avoid |
|---------|---------|
| `test_agent_fallback_after_retry_exhaustion` | `test_internal_counter_updates` |
| `test_pipeline_transitions_to_failure` | `test_buffer_management` |
| `test_parser_streams_deltas_to_terminal` | `test_cache_size_tracking` |

---

## Length Assertions

Length assertions are acceptable **only when combined with content checks**:

```rust
// ✅ CORRECT — count + content
let logs = logger.get_logs();
assert_eq!(logs.len(), 2, "should buffer two writes");
assert!(logs[0].contains("Partial line"));
assert!(logs[1].contains("Another line"));

// ❌ WRONG — count without content
assert_eq!(logger.get_logs().len(), 2);
```

If content checks already verify correctness (e.g., indexing), the length check is redundant — remove it.

---

## Common Anti-Patterns

| Anti-pattern | Fix |
|--------------|-----|
| `#[serial]` in integration/unit tests | Use env-injection or MemoryWorkspace |
| `std::env::set_var` / `remove_var` in tests | Use env-injection pattern |
| `cfg!(test)` in production code | Use dependency injection |
| `TempDir` / `std::fs::*` in integration tests | Use `MemoryWorkspace` |
| `std::process::Command` in integration tests | Use `MockProcessExecutor` |
| Testing private fields / internal state | Test through public APIs |
| Asserting `.len()` without content | Add content assertions |
| Test updates when implementation changes | Only update tests when behavior changes |

---

## Observable vs. Internal State

Ralph's public state fields (`PipelineState`, `AgentChainState`) are part of the observable contract — they are persisted in checkpoints and drive behavior. Testing them is testing observable behavior.

**Observable (test freely):** public fields, fields in checkpoint JSON, counters that enforce behavioral bounds, phase transitions.
**Internal (do not test):** private fields, transient state not in checkpoints, buffer/cache sizes, implementation helpers.

---

## Flaky Test Policy

A flaky test fails non-deterministically. Flaky tests must not remain in gating paths.

### Protocol

1. **Fix** the root cause (inject clock/random seed, use TempDir isolation, apply env-injection).
2. **Quarantine** if the fix is non-trivial — open a GitHub issue first, then annotate:

```rust
#[test]
#[ignore = "flaky: https://github.com/org/repo/issues/N — timing-sensitive signal delivery"]
fn test_something_timing_sensitive() { ... }
```

3. **Resolve** the quarantine issue within one sprint.

**Rules:**
- Every `#[ignore]` attribute must include a `https://` URL (enforced by `scripts/audit_tests.sh`).
- A `#[ignore]` without a URL will fail the audit.
- Do not let quarantined tests accumulate.

**Common root causes:** `std::env::var` races → env-injection; real filesystem → `MemoryWorkspace`; real time → injectable clock; process-global singletons → dependency injection.

---

## Compliance Verification

Run `bash scripts/audit_tests.sh` before every PR.

See `docs/agents/verification.md` for the complete pre-PR command list.

**Key audit checks:**
- No `cfg!(test)` in integration tests
- No `TempDir` / `std::fs::*` in integration tests
- No `std::process::Command` in integration tests
- No `#[serial]` in integration tests or `src/` unit tests
- No `std::env::set_var` / `remove_var` in integration tests
- No `#[serial]` in `process_system_tests/`
- No `git2::` imports in `process_system_tests/`
- No `#[ignore]` without a `https://` issue URL
