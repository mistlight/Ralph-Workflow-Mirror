# System Tests

> **WARNING: DO NOT ADD NEW SYSTEM TESTS WITHOUT EXPLICIT APPROVAL**
>
> System tests are a **LAST RESORT**, not a dumping ground for tests that are
> hard to mock. Before adding ANY new system test, you MUST:
>
> 1. **Write an RFC** explaining why the test cannot use `MemoryWorkspace` and mocks
> 2. **Get explicit user approval** for adding the system test
> 3. **Verify the test is testing a boundary function**, not application logic
>
> If you're testing CLI behavior, pipeline logic, or application features, those
> belong in integration tests with proper mocking. System tests are ONLY for
> testing the actual boundary implementations (e.g., `WorkspaceFs`, `git2` wrappers).

System tests verify behavior that requires real filesystem and git operations.
These tests are **NOT** part of the CI pipeline and run separately as sanity checks.

## When to Use System Tests

System tests are appropriate ONLY for testing **boundary implementations**:
- `WorkspaceFs` implementation (the real filesystem `Workspace` impl)
- Direct `git2` wrapper functions that interact with real repos
- File permission/symlink edge cases that cannot be simulated
- Cross-platform filesystem behavior differences

System tests are **NOT** appropriate for:
- CLI behavior testing (use integration tests with `MemoryWorkspace`)
- Pipeline logic testing (use integration tests with mocks)
- Application features (use integration tests)
- Anything that can be tested with `MemoryWorkspace` + `MockProcessExecutor`

## Allowed Patterns

| Pattern | Usage |
|---------|-------|
| `TempDir` | Create isolated test directories |
| `std::fs::*` | Real filesystem operations |
| `git2` | Real git repository operations |
| `test_helpers::init_git_repo` | Initialize real repos |

## Running System Tests

```bash
# Run system tests (not part of CI)
cargo test -p ralph-workflow-system-tests

# Run with verbose output
cargo test -p ralph-workflow-system-tests -- --nocapture

# Run specific test module
cargo test -p ralph-workflow-system-tests -- rebase::edge_cases
```

## NOT Allowed

- Process spawning (`std::process::Command`) - use `MockProcessExecutor`
- Network calls - mock HTTP traits
- Tests over 1000 lines - split into focused modules

## Relationship to Integration Tests

Integration tests in `tests/integration_tests/` must use:
- `MemoryWorkspace` instead of `TempDir`
- `MockProcessExecutor` instead of real processes
- No `std::fs::*` calls

System tests are the exception where real filesystem operations are necessary
for testing git behavior that cannot be mocked.

## Test Organization

```
tests/system_tests/
├── main.rs              # Test harness entry point
├── test_timeout.rs      # Timeout wrapper (shared with integration tests)
├── SYSTEM_TESTS.md      # This file
├── rebase/              # Real git rebase tests
│   ├── mod.rs
│   ├── edge_cases/
│   ├── category1_failure_modes.rs
│   └── ...
├── git/                 # Real git operation tests
│   └── mod.rs
└── workspace_fs/        # WorkspaceFs implementation tests
    └── mod.rs
```

## Timeout Requirement

All system tests MUST use `with_default_timeout()` or `with_timeout()` wrapper
to prevent indefinite hangs, same as integration tests.
