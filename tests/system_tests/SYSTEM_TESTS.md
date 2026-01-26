# System Tests

System tests verify behavior that requires real filesystem and git operations.
These tests are **NOT** part of the CI pipeline and run separately as sanity checks.

## When to Use System Tests

Use system tests ONLY for:
- Real git operations (rebase, merge, conflict resolution)
- `WorkspaceFs` implementation testing
- File permission/symlink edge cases
- Cross-platform filesystem behavior

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
