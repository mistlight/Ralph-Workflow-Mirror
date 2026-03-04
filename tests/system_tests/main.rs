// DO NOT CHANGE THESE CLIPPY SETTINGS, YOU MUST REFACTOR INSTEAD, EVEN IF IT TAKES YOU 100 YEARS
// Note: unsafe_code is not denied in test code because tests may require unsafe blocks for
// low-level testing (e.g., signal handling, timezone manipulation). All unsafe blocks must
// have proper safety documentation explaining why they are safe.
//
// Note: clippy::cargo is not enabled because it flags transitive dependency version conflicts
// (e.g., bitflags 1.3.2 from inotify vs 2.10.0 from other crates) which are ecosystem-level
// issues outside our control and don't reflect code quality problems.
#![deny(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
//! git2-system-tests: real git repository operations.
//!
//! ALL tests in this binary use `git2::Repository` or `init_git_repo()`, which
//! wraps libgit2 — a C library with a global reference counter. Concurrent drops
//! of `Repository` objects across threads cause SIGABRT. For this reason ALL
//! tests in this binary carry `#[serial]`.
//!
//! This is NOT a design smell in Ralph's code; it is an inherent libgit2
//! constraint. Tests that do not touch libgit2 belong in `process-system-tests`.
//!
//! # WARNING: DO NOT ADD NEW SYSTEM TESTS WITHOUT APPROVAL
//!
//! System tests are a **LAST RESORT**. Before adding ANY new test here:
//! 1. Write an RFC explaining why `MemoryWorkspace` + mocks won't work
//! 2. Get explicit user approval
//! 3. Verify you're testing a BOUNDARY function, not application logic
//!
//! If testing CLI/pipeline behavior, use integration tests with proper mocking.
//!
//! NOTE: The `signal_cleanup` module validates Ctrl+C/SIGINT cleanup semantics
//! end-to-end (real signal delivery, file permission bits, hook restoration). It
//! uses `init_git_repo()` for setup and therefore remains in this serial binary.
//!
//! These tests are **NOT** part of CI. Run manually as sanity checks.
//! See `SYSTEM_TESTS.md` for full guidelines and `docs/agents/testing-guide.md`
//! for the canonical testing strategy.
//!
//! # Running System Tests
//!
//! ```bash
//! cargo test -p ralph-workflow-tests --test git2-system-tests
//! ```
//!
//! # When to Use System Tests
//!
//! ONLY for testing **boundary implementations**:
//! - `WorkspaceFs` (the real filesystem `Workspace` impl)
//! - Direct `git2` wrapper functions
//! - File permission/symlink edge cases
//!
//! # NOT For
//!
//! - CLI behavior (use integration tests)
//! - Pipeline logic (use integration tests)
//! - Anything testable with `MemoryWorkspace` + `MockProcessExecutor`
//! - Tests that do not use libgit2 (use `process-system-tests` instead)

mod common;
mod test_timeout;

// Test modules using real git2/libgit2 operations (require #[serial])
// NOTE: agents and deduplication moved to process-system-tests binary (no libgit2 needed)
mod git;
mod prompt_permissions;
mod rebase;
mod signal_cleanup;
