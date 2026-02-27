# Required Verification (before PR/completion)

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

# Lint main crate (all targets: lib, tests, benchmarks, examples)
# Note: Enforces clippy::all, clippy::pedantic, clippy::nursery
# via #![deny(...)] attributes in lib.rs and main.rs
# (clippy::cargo is not enabled as it flags ecosystem-level dependency conflicts)
cargo clippy -p ralph-workflow --all-targets --all-features -- -D warnings

# Lint integration tests
# Note: Enforces clippy::all, clippy::pedantic, clippy::nursery
# via #![deny(...)] attributes in tests/integration_tests/main.rs and tests/system_tests/main.rs
# (clippy::cargo is not enabled as it flags ecosystem-level dependency conflicts)
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings

# Lint test helpers
# Enforces clippy::all, clippy::pedantic, clippy::nursery
# via #![deny(...)] attributes in test-helpers/src/lib.rs
cargo clippy -p test-helpers --all-targets -- -D warnings

# Unit tests
cargo test -p ralph-workflow --lib --all-features

# Integration tests
cargo test -p ralph-workflow-tests

# Memory safety verification (bounded growth, thread cleanup, Arc patterns)
bash scripts/verify_memory_safety.sh
# (Use `--verbose` to print progress/output on failure debugging.)

# Performance regression verification (long-running tests, checkpoint size, etc.)
bash scripts/ci_performance_regression.sh
# (Use `--verbose` to print progress/output on failure debugging.)

# Per-run logging tests (when changing logging infrastructure)
cargo test -p ralph-workflow-tests logging_per_run

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

For dylint details/troubleshooting, see `docs/tooling/dylint.md`.
