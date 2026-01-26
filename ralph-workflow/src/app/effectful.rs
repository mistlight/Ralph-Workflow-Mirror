//! Effectful app operations that use AppEffect handlers.
//!
//! This module provides functions that execute CLI operations via an
//! [`AppEffectHandler`], enabling testing without real side effects.
//!
//! # Architecture
//!
//! Each function in this module:
//! 1. Takes an `AppEffectHandler` reference
//! 2. Executes effects through the handler
//! 3. Returns strongly-typed results
//!
//! In production, use [`RealAppEffectHandler`] for actual I/O.
//! In tests, use [`MockAppEffectHandler`] to verify behavior without side effects.
//!
//! # Example
//!
//! ```ignore
//! use ralph_workflow::app::effectful::handle_reset_start_commit;
//! use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
//!
//! // Test without real git or filesystem
//! let mut handler = MockAppEffectHandler::new()
//!     .with_head_oid("abc123");
//!
//! let result = handle_reset_start_commit(&mut handler, None);
//! assert!(result.is_ok());
//! ```

use super::effect::{AppEffect, AppEffectHandler, AppEffectResult};
use std::path::PathBuf;

/// Handle the `--reset-start-commit` command using effects.
///
/// This function resets the `.agent/start_commit` file to track the
/// merge-base with the default branch (or HEAD if on main/master).
///
/// # Arguments
///
/// * `handler` - The effect handler to execute operations through
/// * `working_dir_override` - Optional directory override (for testing)
///
/// # Returns
///
/// Returns the OID that was written to the start_commit file, or an error.
///
/// # Effects Emitted
///
/// 1. `SetCurrentDir` - If working_dir_override is provided
/// 2. `GitRequireRepo` - Validates git repository exists
/// 3. `GitGetRepoRoot` - Gets the repository root path
/// 4. `SetCurrentDir` - Changes to repo root (if no override)
/// 5. `GitResetStartCommit` - Resets the start commit reference
pub fn handle_reset_start_commit<H: AppEffectHandler>(
    handler: &mut H,
    working_dir_override: Option<&PathBuf>,
) -> Result<String, String> {
    // Effect 1: Set CWD if override provided
    if let Some(dir) = working_dir_override {
        match handler.execute(AppEffect::SetCurrentDir { path: dir.clone() }) {
            AppEffectResult::Ok => {}
            AppEffectResult::Error(e) => return Err(e),
            other => return Err(format!("unexpected result from SetCurrentDir: {other:?}")),
        }
    }

    // Effect 2: Validate git repo
    match handler.execute(AppEffect::GitRequireRepo) {
        AppEffectResult::Ok => {}
        AppEffectResult::Error(e) => return Err(e),
        other => return Err(format!("unexpected result from GitRequireRepo: {other:?}")),
    }

    // Effect 3: Get repo root and set CWD
    let repo_root = match handler.execute(AppEffect::GitGetRepoRoot) {
        AppEffectResult::Path(p) => p,
        AppEffectResult::Error(e) => return Err(e),
        other => return Err(format!("unexpected result from GitGetRepoRoot: {other:?}")),
    };

    // Effect 4: Set CWD to repo root if no override was provided
    if working_dir_override.is_none() {
        match handler.execute(AppEffect::SetCurrentDir { path: repo_root }) {
            AppEffectResult::Ok => {}
            AppEffectResult::Error(e) => return Err(e),
            other => return Err(format!("unexpected result from SetCurrentDir: {other:?}")),
        }
    }

    // Effect 5: Reset start commit
    match handler.execute(AppEffect::GitResetStartCommit) {
        AppEffectResult::String(oid) => Ok(oid),
        AppEffectResult::Error(e) => Err(e),
        other => Err(format!(
            "unexpected result from GitResetStartCommit: {other:?}"
        )),
    }
}

/// Save the starting commit at pipeline start using effects.
///
/// This records the current HEAD (or merge-base on feature branches) to
/// `.agent/start_commit` for incremental diff generation.
///
/// # Returns
///
/// Returns the OID that was saved, or an error.
pub fn save_start_commit<H: AppEffectHandler>(handler: &mut H) -> Result<String, String> {
    match handler.execute(AppEffect::GitSaveStartCommit) {
        AppEffectResult::String(oid) => Ok(oid),
        AppEffectResult::Error(e) => Err(e),
        other => Err(format!(
            "unexpected result from GitSaveStartCommit: {other:?}"
        )),
    }
}

/// Check if the current branch is main/master using effects.
///
/// # Returns
///
/// Returns `true` if on main or master branch, `false` otherwise.
pub fn is_on_main_branch<H: AppEffectHandler>(handler: &mut H) -> Result<bool, String> {
    match handler.execute(AppEffect::GitIsMainBranch) {
        AppEffectResult::Bool(b) => Ok(b),
        AppEffectResult::Error(e) => Err(e),
        other => Err(format!("unexpected result from GitIsMainBranch: {other:?}")),
    }
}

/// Get the current HEAD OID using effects.
///
/// # Returns
///
/// Returns the 40-character hex OID of HEAD, or an error.
pub fn get_head_oid<H: AppEffectHandler>(handler: &mut H) -> Result<String, String> {
    match handler.execute(AppEffect::GitGetHeadOid) {
        AppEffectResult::String(oid) => Ok(oid),
        AppEffectResult::Error(e) => Err(e),
        other => Err(format!("unexpected result from GitGetHeadOid: {other:?}")),
    }
}

/// Validate that we're in a git repository using effects.
///
/// # Returns
///
/// Returns `Ok(())` if in a git repo, error otherwise.
pub fn require_repo<H: AppEffectHandler>(handler: &mut H) -> Result<(), String> {
    match handler.execute(AppEffect::GitRequireRepo) {
        AppEffectResult::Ok => Ok(()),
        AppEffectResult::Error(e) => Err(e),
        other => Err(format!("unexpected result from GitRequireRepo: {other:?}")),
    }
}

/// Get the repository root path using effects.
///
/// # Returns
///
/// Returns the absolute path to the repository root.
pub fn get_repo_root<H: AppEffectHandler>(handler: &mut H) -> Result<PathBuf, String> {
    match handler.execute(AppEffect::GitGetRepoRoot) {
        AppEffectResult::Path(p) => Ok(p),
        AppEffectResult::Error(e) => Err(e),
        other => Err(format!("unexpected result from GitGetRepoRoot: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::mock_effect_handler::MockAppEffectHandler;

    #[test]
    fn test_reset_start_commit_emits_correct_effects() {
        let mut handler = MockAppEffectHandler::new();

        let result = handle_reset_start_commit(&mut handler, None);

        assert!(result.is_ok());
        let captured = handler.captured();
        assert!(
            captured
                .iter()
                .any(|e| matches!(e, AppEffect::GitRequireRepo)),
            "should emit GitRequireRepo"
        );
        assert!(
            captured
                .iter()
                .any(|e| matches!(e, AppEffect::GitGetRepoRoot)),
            "should emit GitGetRepoRoot"
        );
        assert!(
            captured
                .iter()
                .any(|e| matches!(e, AppEffect::GitResetStartCommit)),
            "should emit GitResetStartCommit"
        );
    }

    #[test]
    fn test_reset_start_commit_with_working_dir() {
        let mut handler = MockAppEffectHandler::new();
        let dir = PathBuf::from("/test/dir");

        let result = handle_reset_start_commit(&mut handler, Some(&dir));

        assert!(result.is_ok());
        let captured = handler.captured();
        assert!(
            captured
                .iter()
                .any(|e| matches!(e, AppEffect::SetCurrentDir { path } if path == &dir)),
            "should emit SetCurrentDir with the override path"
        );
    }

    #[test]
    fn test_reset_start_commit_fails_without_repo() {
        let mut handler = MockAppEffectHandler::new().without_repo();

        let result = handle_reset_start_commit(&mut handler, None);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("git repository"));
    }

    #[test]
    fn test_save_start_commit_returns_oid() {
        let expected_oid = "abc123def456";
        let mut handler = MockAppEffectHandler::new().with_head_oid(expected_oid);

        let result = save_start_commit(&mut handler);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_oid);
    }

    #[test]
    fn test_is_on_main_branch_true() {
        let mut handler = MockAppEffectHandler::new().on_main_branch();

        let result = is_on_main_branch(&mut handler);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_is_on_main_branch_false() {
        let mut handler = MockAppEffectHandler::new(); // default is not on main

        let result = is_on_main_branch(&mut handler);

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_get_head_oid() {
        let expected = "1234567890abcdef1234567890abcdef12345678";
        let mut handler = MockAppEffectHandler::new().with_head_oid(expected);

        let result = get_head_oid(&mut handler);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_require_repo_success() {
        let mut handler = MockAppEffectHandler::new();

        let result = require_repo(&mut handler);

        assert!(result.is_ok());
    }

    #[test]
    fn test_require_repo_failure() {
        let mut handler = MockAppEffectHandler::new().without_repo();

        let result = require_repo(&mut handler);

        assert!(result.is_err());
    }

    #[test]
    fn test_get_repo_root() {
        let mut handler = MockAppEffectHandler::new();

        let result = get_repo_root(&mut handler);

        assert!(result.is_ok());
        // Default mock CWD is "/"
        assert_eq!(result.unwrap(), PathBuf::from("/"));
    }
}
