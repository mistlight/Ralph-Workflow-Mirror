# AGENTS.md

ALWAYS USE test-driven-development!

This repository welcomes automated code assistants ("agents") and human contributors.
Follow these rules so changes stay safe, consistent, and easy to review.

## Priorities (in order)

1. **Correctness** - tests pass, behavior matches intent
2. **Maintainability** - clear code, no magic
3. **Consistency** - follow existing patterns, rustfmt/clippy clean
4. **Small diffs** - avoid drive-by refactors

If instructions conflict with other files (e.g., `CONTRIBUTING.md`), follow the **stricter** rule.

See **[CODE_STYLE.md](CODE_STYLE.md)** for design principles and testing philosophy.

---

## File Creation Rules

- **NO temporary .md files** in root or doc folders
- **NO new files** in root/doc directories unless explicitly about documentation
- **DO** update outdated documentation when encountered
- **ALL temporary files MUST go in `tmp/` at the repo root** (gitignored); use a unique subdir like `tmp/ralph-workflow-*` if needed

---

## External Dependencies

Never assume API behavior. Research order:
1. Use context7
2. If that fails, check official docs via playwright

---

## YOLO Mode (CRITICAL)

All agents MUST run with YOLO mode enabled (`--dangerously-skip-permissions` for Claude CLI, `--yes` for Aider).

**Why:** Ralph is a fully automated pipeline. All roles (Developer, Reviewer, Commit) write XML to `.agent/tmp/`. Without write permissions, the XSD retry mechanism fails.

**Configuration:** Every agent needs `yolo_flag` in `agents.toml`:
- Claude CLI: `--dangerously-skip-permissions`
- Aider: `--yes`
- Claude Code: Usually no flag needed

---

## Integration Tests (CRITICAL)

**Read [tests/INTEGRATION_TESTS.md](tests/INTEGRATION_TESTS.md) before touching integration tests.**

**Principles:**
- Test **observable behavior**, not implementation details
- Mock only at **architectural boundaries** (filesystem, network, external APIs)
- NEVER use `cfg!(test)` branches or test-only flags in production code
- When tests fail, fix implementation (unless expected behavior changed)

**Common mistakes:**
- Mocking internal functions (only mock external dependencies)
- Testing private details (test through public APIs)
- Adding `#[cfg(test)]` in production (use dependency injection)
- Updating tests because implementation changed (only if behavior changed)

**Required patterns:**
- Parser tests: `TestPrinter` from `ralph_workflow::json_parser::printer`
- File operations: `MemoryWorkspace` (NO `TempDir`, NO `std::fs::*`)
- Process execution: `MockProcessExecutor` (NO real process spawning)

---

## Workspace Trait (CRITICAL)

ALL filesystem operations MUST use the `Workspace` trait. Direct `std::fs::*` is FORBIDDEN.

| FORBIDDEN | REQUIRED |
|-----------|----------|
| `std::fs::read_to_string(path)` | `workspace.read(path)` |
| `std::fs::write(path, content)` | `workspace.write(path, content)` |
| `std::fs::create_dir_all(path)` | `workspace.create_dir_all(path)` |
| `std::fs::read_dir(path)` | `workspace.read_dir(path)` |
| `path.exists()` | `workspace.exists(path)` |
| `std::fs::remove_file(path)` | `workspace.remove(path)` |

### Implementation

```rust
// Production code - accepts workspace via dependency injection
fn my_function(workspace: &dyn Workspace) -> Result<()> {
    let content = workspace.read(Path::new(".agent/config.toml"))?;
    workspace.write(Path::new(".agent/output.txt"), "result")?;
    Ok(())
}

// Tests - use MemoryWorkspace
#[test]
fn test_my_function() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/config.toml", "key = value");
    
    my_function(&workspace).unwrap();
    
    assert!(workspace.was_written(".agent/output.txt"));
}
```

### Exceptions

The ONLY acceptable uses of `std::fs` are:
1. Inside `WorkspaceFs` implementation itself (the production `Workspace` impl)
2. Bootstrap code that discovers the repo root before `Workspace` is created

### Documented Exceptions

The following specific uses of `std::fs` are acceptable and do not need refactoring:

| Location | Reason |
|----------|--------|
| `workspace.rs` (`WorkspaceFs`) | This IS the production filesystem implementation |
| `app/effect_handler.rs` (`RealAppEffectHandler`) | This IS the production effect handler |
| `config/path_resolver.rs` (`RealConfigEnvironment`) | Production config environment implementation |
| `ralph-workflow/src/agents/opencode_api/cache.rs` (`RealCacheEnvironment`) | Production cache implementation |
| `git_helpers/rebase.rs` | Operating on `.git/` directory internals |
| `git_helpers/hooks.rs` | Bootstrap operation on `.git/hooks/` (see module docs) |
| `files/protection/monitoring.rs` | Atomic file open for TOCTOU security |
| `files/io/agent_files.rs` (CWD functions) | CLI plumbing commands before workspace available |
| `checkpoint/file_state.rs` (CWD-relative impl functions) | CLI-layer code before workspace available |
| `logger/output.rs` (legacy mode logging) | Legacy mode logging for CLI layer code before workspace available |
| `config/unified.rs` (`load_from_path`) | Convenience method for config loading when ConfigEnvironment not available |

All other production code MUST use the Workspace trait.

**When you see `std::fs` in production code outside these exceptions, it MUST be refactored to use `Workspace`.**

---

## No `#[allow(dead_code)]`

**Never** introduce or keep `#[allow(dead_code)]` (or deprecated code). Consider Dead Code removal to be EVEN MORE IMPORTANT! than the actual current task. It is very important no dead code exists.

Handle dead code by:
- Making it used
- Implementing the feature now (no time constraints)
- Gating behind a feature flag
- Moving to `examples/` or `benches/`
- Deleting it

---

## Required Verification (before PR/completion)

Run git rebase on main if on feature branch. All commands must produce **NO OUTPUT**:

```bash
# Check for forbidden allow/expect attributes (aka. NOTHING IS ALLOWED HERE so this should produce NO OUTPUT)
rg -n -U --pcre2 '(?m)^\s*#\s*!?\[\s*(?:(?:allow|expect)\s*\(|cfg_attr\s*\((?:[^()]|\([^()]*\))*?,\s*(?:allow|expect)\s*\()' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# Integration test compliance
./tests/integration_tests/compliance_check.sh

# No test flags in production code (DO NOT MODIFY THIS SCRIPT)
./tests/integration_tests/no_test_flags_check.sh

# Format check
cargo fmt --all --check

# Lint main crate
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings

# Lint integration tests
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings

# Unit tests
cargo test -p ralph-workflow --lib --all-features

# Integration tests
cargo test -p ralph-workflow-tests

# Release build
cargo build --release

# Custom lints (dylint) - check for files exceeding line limits
# This runs the file_too_long lint from lints/file_too_long
#
# IMPORTANT:
# - Running dylint against the `ralph` binary target can fail the build because the binary uses
#   `#![deny(warnings)]` (warnings become hard errors).
# - Run the lint against the `ralph-workflow` *library* target instead.
#
# Recommended (library target only):
make dylint
# or:
cargo dylint -p ralph-workflow --lib file_too_long -- --lib
```

**If ANY command produces output, FIX IT before continuing.** No ignored tests allowed.

---

## Custom Lints (dylint)

This repository uses [dylint](https://github.com/trailofbits/dylint) for custom Rust lints.

### Available Lints

| Lint | Description |
|------|-------------|
| `file_too_long` | Warns when a source file exceeds 500 lines (consider refactoring) or 1000 lines (MUST refactor) |

### Running Lints

```bash
# Run all custom lints
cargo dylint --all

# Run a specific lint (recommended: library target only)
make dylint
# or:
cargo dylint -p ralph-workflow --lib file_too_long -- --lib
```

### Developing Lints

Custom lints are in the `lints/` directory. Each lint is a separate crate that compiles to a dynamic library.

To build and test a lint:
```bash
cd lints/file_too_long
cargo +nightly test
```

**Note:** Dylint lints require nightly Rust due to use of rustc internals.
