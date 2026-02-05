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
# - The Makefile automatically ensures nightly toolchain's cargo is used for driver builds,
#   even when system cargo (Homebrew/apt) is stable.
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

### Environment Variables for Sandboxed Environments

The `make dylint` target respects standard Rust environment variables:

| Variable | Purpose | Example |
|----------|---------|---------|
| `CARGO_HOME` | Override cargo cache/bin location | `/tmp/cargo-cache` |
| `RUSTUP_HOME` | Override rustup installation location | `/tmp/rustup-home` |
| `DYLINT_DRIVER_PATH` | Override dylint driver cache location | `/tmp/dylint-drivers` |

For hermetic builds or CI environments with restricted HOME:

```bash
# Example: Run dylint in a sandboxed environment
export CARGO_HOME=/writable/path/cargo
export RUSTUP_HOME=/writable/path/rustup
export DYLINT_DRIVER_PATH=/writable/path/drivers
make dylint
```

### Known Issues

**dylint_driver build failure (v3.5.1 and later):**

If you encounter an error like:
```
error: environment variable `RUSTUP_TOOLCHAIN` not defined at compile time
```

This is a known upstream bug in dylint_driver (v3.5.1, v5.0.0, and potentially other versions) that occurs when cargo-dylint rebuilds the driver. The driver build script requires the `RUSTUP_TOOLCHAIN` environment variable to be set at compile time using `env!()`, but cargo-dylint explicitly unsets it when spawning the driver build subprocess (`env -u RUSTUP_TOOLCHAIN cargo build`).

**Solution implemented in `make dylint`:**

The `make dylint` target implements a multi-layered approach to ensure the dylint driver is always built with the nightly toolchain:

1. **Environment validation:** Checks that CARGO_HOME, RUSTUP_HOME, and DYLINT_DRIVER_PATH are writable
2. **Toolchain bootstrapping:** Installs rustup (if missing) and nightly toolchain with required components (rustc-dev, llvm-tools-preview)
3. **Cargo wrapper script:** Creates a temporary wrapper script that exports RUSTUP_TOOLCHAIN=nightly and CARGO variable before exec'ing the real nightly cargo
4. **PATH manipulation:** Prepends the wrapper directory and nightly bin directory to PATH, ensuring the wrapper is found first
5. **Environment export:** Exports RUSTUP_TOOLCHAIN, RUSTC, CARGO, and all cache location variables
6. **Validation:** Verifies that `cargo` resolves to the wrapper script, warning if PATH resolution fails

**How the wrapper works:**

When cargo-dylint runs `env -u RUSTUP_TOOLCHAIN cargo build` to rebuild the driver, it:
1. Unsets RUSTUP_TOOLCHAIN in the subprocess environment
2. Searches PATH for the `cargo` binary
3. Finds and executes the wrapper script (first in PATH)
4. Wrapper exports RUSTUP_TOOLCHAIN=nightly
5. Wrapper exports CARGO variable pointing to nightly cargo (additional safety mechanism)
6. Wrapper execs the real nightly cargo with RUSTUP_TOOLCHAIN set

This approach works around cargo-dylint's explicit unsetting of RUSTUP_TOOLCHAIN, addressing the E0554 failure mode where cargo-dylint rebuilds its driver using a stable toolchain. The CARGO environment variable export provides an additional fallback if PATH resolution somehow fails.

**Limitations:**

This Makefile fix cannot fully eliminate upstream failures where cargo-dylint (or the driver build) requires additional environment variables or pre-provisioned components in strictly offline/sandboxed environments.

### Troubleshooting `make dylint`

**Symptom:** E0554 error during dylint driver build

```
error[E0554]: `#![feature]` may not be used on the stable release channel
```

**Cause:** Driver build used stable cargo instead of nightly

**Solution:** Verify nightly toolchain is installed with required components:
```bash
rustup toolchain install nightly --profile minimal
rustup component add rustc-dev llvm-tools-preview --toolchain nightly
```

If the issue persists, use the verbose mode to debug PATH resolution:
```bash
make dylint-verbose
```

---

**Symptom:** "cannot create required directory" error

```
error: cannot create required directory: /path/to/dir
```

**Cause:** HOME or cache directories are not writable

**Solution:** Set writable locations explicitly:
```bash
export CARGO_HOME=/tmp/cargo
export RUSTUP_HOME=/tmp/rustup
export DYLINT_DRIVER_PATH=/tmp/drivers
make dylint
```

---

**Symptom:** Network errors during toolchain/component installation

```
error: failed to install nightly toolchain
```

**Cause:** Offline environment cannot fetch toolchains

**Solution:** Pre-install nightly with components before running make dylint:
```bash
# In an online environment, install required toolchain and components
rustup toolchain install nightly --profile minimal
rustup component add rustc-dev llvm-tools-preview --toolchain nightly

# Install cargo-dylint globally
cargo install cargo-dylint dylint-link

# Now `make dylint` will work offline
```

---

**Symptom:** "dylint-driver" not found or not functional

```
Warning: command failed: "~/.dylint_drivers/nightly-*/dylint-driver" "-V"
```

**Cause:** Corrupted or mismatched dylint driver cache

**Solution:** Clean the driver cache and rebuild:
```bash
rm -rf ~/.dylint_drivers
make dylint
```

---

**Symptom:** Warning about cargo not resolving to wrapper

```
warning: cargo resolves to /usr/local/bin/cargo instead of /tmp/xyz/cargo
Continuing anyway, but this may cause issues...
```

**Cause:** System PATH configuration or shell aliases override the wrapper

**Solution:** Check for shell aliases or functions that override cargo:
```bash
# Check for cargo alias or function
type cargo

# If an alias exists, unalias it temporarily
unalias cargo

# Run make dylint again
make dylint
```

---

**Debugging with dylint-verbose:**

To see detailed information about PATH, cargo resolution, and toolchain selection:
```bash
make dylint-verbose
```

This will display:
- CARGO_HOME, RUSTUP_HOME, DYLINT_DRIVER_PATH locations
- PATH resolution (first 3 entries)
- Wrapper script path and contents
- Which cargo binary is being used (via `command -v` and `which`)
- RUSTUP_TOOLCHAIN, RUSTC, and CARGO environment variables
- Nightly toolchain bin directory location

Use this output to diagnose PATH resolution issues or verify the nightly toolchain is correctly configured.
