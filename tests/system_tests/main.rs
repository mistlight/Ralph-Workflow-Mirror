// DO NOT CHANGE THESE CLIPPY SETTINGS, YOU MUST REFACTOR INSTEAD, EVEN IF IT TAKES YOU 100 YEARS
// Note: unsafe_code is not denied in test code because tests may require unsafe blocks for
// low-level testing (e.g., signal handling, timezone manipulation). All unsafe blocks must
// have proper safety documentation explaining why they are safe.
//
// Note: clippy::cargo is not enabled because it flags transitive dependency version conflicts
// (e.g., bitflags 1.3.2 from inotify vs 2.10.0 from other crates) which are ecosystem-level
// issues outside our control and don't reflect code quality problems.
#![deny(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
//! System tests - real filesystem and git operations.
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
//! NOTE: The `signal_cleanup` module is an exception: it validates Ctrl+C/SIGINT cleanup
//! semantics end-to-end (real signal delivery, file permission bits, hook restoration) which
//! cannot be exercised via `MockProcessExecutor` or `MemoryWorkspace` because the interrupt
//! handler and exit code are process-global OS interactions.
//!
//! These tests are **NOT** part of CI. Run manually as sanity checks.
//! See `SYSTEM_TESTS.md` for full guidelines.
//!
//! # Running System Tests
//!
//! ```bash
//! cargo test -p ralph-workflow-system-tests
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

mod common;
mod test_timeout;

// Test modules using real filesystem/git operations
mod agents;
mod deduplication;
mod git;
mod prompt_permissions;
mod rebase;
mod signal_cleanup;
