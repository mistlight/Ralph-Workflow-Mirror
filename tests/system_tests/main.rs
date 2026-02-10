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
mod rebase;
