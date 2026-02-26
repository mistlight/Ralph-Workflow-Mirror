//! Real implementation of `AppEffectHandler`.
//!
//! This handler executes actual side effects for production use.
//! It provides concrete implementations for all [`AppEffect`] variants
//! by delegating to the appropriate system calls or internal modules.

use super::effect::{AppEffect, AppEffectHandler, AppEffectResult, CommitResult};
use std::path::{Path, PathBuf};

/// Real effect handler that executes actual side effects.
///
/// This implementation performs real I/O operations including:
/// - Filesystem operations via `std::fs`
/// - Git operations via `crate::git_helpers`
/// - Environment variable access via `std::env`
/// - Working directory changes via `std::env::set_current_dir`
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::app::effect_handler::RealAppEffectHandler;
/// use ralph_workflow::app::effect::{AppEffect, AppEffectHandler};
///
/// let mut handler = RealAppEffectHandler::new();
/// let result = handler.execute(AppEffect::PathExists {
///     path: PathBuf::from("Cargo.toml"),
/// });
/// ```
pub struct RealAppEffectHandler {
    /// Optional workspace root for relative path resolution.
    ///
    /// When set, relative paths in effects are resolved against this root.
    /// When `None`, paths are used as-is (relative to current working directory).
    workspace_root: Option<PathBuf>,
}

impl RealAppEffectHandler {
    /// Create a new handler without a workspace root.
    ///
    /// Paths will be used as-is, relative to the current working directory.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            workspace_root: None,
        }
    }

    /// Create a new handler with a specific workspace root.
    ///
    /// All relative paths in effects will be resolved against this root.
    ///
    /// # Arguments
    ///
    /// * `root` - The workspace root directory for path resolution.
    #[must_use]
    pub const fn with_workspace_root(root: PathBuf) -> Self {
        Self {
            workspace_root: Some(root),
        }
    }

    /// Resolve a path against the workspace root if set.
    ///
    /// If the path is absolute, it is returned as-is.
    /// If the path is relative and a workspace root is set, the path is
    /// joined to the workspace root.
    /// If the path is relative and no workspace root is set, the path is
    /// returned as-is.
    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ref root) = self.workspace_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
    }
}

impl Default for RealAppEffectHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl AppEffectHandler for RealAppEffectHandler {
    fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
        match effect {
            // =========================================================================
            // Working Directory Effects
            // =========================================================================
            AppEffect::SetCurrentDir { path } => {
                let resolved = self.resolve_path(&path);
                match std::env::set_current_dir(&resolved) {
                    Ok(()) => AppEffectResult::Ok,
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to set current directory to '{}': {}",
                        resolved.display(),
                        e
                    )),
                }
            }

            // =========================================================================
            // Filesystem Effects
            // =========================================================================
            AppEffect::WriteFile { path, content } => {
                let resolved = self.resolve_path(&path);
                // Ensure parent directories exist
                if let Some(parent) = resolved.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return AppEffectResult::Error(format!(
                            "Failed to create parent directories for '{}': {}",
                            resolved.display(),
                            e
                        ));
                    }
                }
                match std::fs::write(&resolved, content) {
                    Ok(()) => AppEffectResult::Ok,
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to write file '{}': {}",
                        resolved.display(),
                        e
                    )),
                }
            }

            AppEffect::ReadFile { path } => {
                let resolved = self.resolve_path(&path);
                match std::fs::read_to_string(&resolved) {
                    Ok(content) => AppEffectResult::String(content),
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to read file '{}': {}",
                        resolved.display(),
                        e
                    )),
                }
            }

            AppEffect::DeleteFile { path } => {
                let resolved = self.resolve_path(&path);
                match std::fs::remove_file(&resolved) {
                    Ok(()) => AppEffectResult::Ok,
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to delete file '{}': {}",
                        resolved.display(),
                        e
                    )),
                }
            }

            AppEffect::CreateDir { path } => {
                let resolved = self.resolve_path(&path);
                match std::fs::create_dir_all(&resolved) {
                    Ok(()) => AppEffectResult::Ok,
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to create directory '{}': {}",
                        resolved.display(),
                        e
                    )),
                }
            }

            AppEffect::PathExists { path } => {
                let resolved = self.resolve_path(&path);
                AppEffectResult::Bool(resolved.exists())
            }

            AppEffect::SetReadOnly { path, readonly } => {
                let resolved = self.resolve_path(&path);
                match std::fs::metadata(&resolved) {
                    Ok(metadata) => {
                        let mut permissions = metadata.permissions();
                        permissions.set_readonly(readonly);
                        match std::fs::set_permissions(&resolved, permissions) {
                            Ok(()) => AppEffectResult::Ok,
                            Err(e) => AppEffectResult::Error(format!(
                                "Failed to set permissions on '{}': {}",
                                resolved.display(),
                                e
                            )),
                        }
                    }
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to get metadata for '{}': {}",
                        resolved.display(),
                        e
                    )),
                }
            }

            // =========================================================================
            // Git Effects
            // =========================================================================
            AppEffect::GitRequireRepo => match crate::git_helpers::require_git_repo() {
                Ok(()) => AppEffectResult::Ok,
                Err(e) => AppEffectResult::Error(format!("Not in a git repository: {e}")),
            },

            AppEffect::GitGetRepoRoot => match crate::git_helpers::get_repo_root() {
                Ok(root) => AppEffectResult::Path(root),
                Err(e) => AppEffectResult::Error(format!("Failed to get repository root: {e}")),
            },

            AppEffect::GitGetHeadOid => match crate::git_helpers::get_current_head_oid() {
                Ok(oid) => AppEffectResult::String(oid),
                Err(e) => AppEffectResult::Error(format!("Failed to get HEAD OID: {e}")),
            },

            AppEffect::GitDiff => match crate::git_helpers::git_diff() {
                Ok(diff) => AppEffectResult::String(diff),
                Err(e) => AppEffectResult::Error(format!("Failed to get git diff: {e}")),
            },

            AppEffect::GitDiffFrom { start_oid } => {
                match crate::git_helpers::git_diff_from(&start_oid) {
                    Ok(diff) => AppEffectResult::String(diff),
                    Err(e) => AppEffectResult::Error(format!(
                        "Failed to get git diff from '{start_oid}': {e}"
                    )),
                }
            }

            AppEffect::GitDiffFromStart => match crate::git_helpers::get_git_diff_from_start() {
                Ok(diff) => AppEffectResult::String(diff),
                Err(e) => {
                    AppEffectResult::Error(format!("Failed to get diff from start commit: {e}"))
                }
            },

            AppEffect::GitSnapshot => match crate::git_helpers::git_snapshot() {
                Ok(snapshot) => AppEffectResult::String(snapshot),
                Err(e) => AppEffectResult::Error(format!("Failed to create git snapshot: {e}")),
            },

            AppEffect::GitAddAll => match crate::git_helpers::git_add_all() {
                Ok(staged) => AppEffectResult::Bool(staged),
                Err(e) => AppEffectResult::Error(format!("Failed to stage all changes: {e}")),
            },

            AppEffect::GitCommit {
                message,
                user_name,
                user_email,
            } => {
                match crate::git_helpers::git_commit(
                    &message,
                    user_name.as_deref(),
                    user_email.as_deref(),
                    None, // No executor needed for basic commit
                ) {
                    Ok(Some(oid)) => {
                        AppEffectResult::Commit(CommitResult::Success(oid.to_string()))
                    }
                    Ok(None) => AppEffectResult::Commit(CommitResult::NoChanges),
                    Err(e) => AppEffectResult::Error(format!("Failed to create commit: {e}")),
                }
            }

            AppEffect::GitSaveStartCommit => match crate::git_helpers::save_start_commit() {
                Ok(()) => AppEffectResult::Ok,
                Err(e) => AppEffectResult::Error(format!("Failed to save start commit: {e}")),
            },

            AppEffect::GitResetStartCommit => match crate::git_helpers::reset_start_commit() {
                Ok(result) => AppEffectResult::String(result.oid),
                Err(e) => AppEffectResult::Error(format!("Failed to reset start commit: {e}")),
            },

            AppEffect::GitRebaseOnto { upstream_branch: _ } => {
                // Rebase operations require a process executor which we don't have here.
                // This effect should be handled by a higher-level component that has
                // access to a ProcessExecutor.
                AppEffectResult::Error(
                    "GitRebaseOnto requires executor injection - use pipeline runner".to_string(),
                )
            }

            AppEffect::GitGetConflictedFiles => match crate::git_helpers::get_conflicted_files() {
                Ok(files) => AppEffectResult::StringList(files),
                Err(e) => AppEffectResult::Error(format!("Failed to get conflicted files: {e}")),
            },

            AppEffect::GitContinueRebase => {
                // Continue rebase requires a process executor.
                AppEffectResult::Error(
                    "GitContinueRebase requires executor injection - use pipeline runner"
                        .to_string(),
                )
            }

            AppEffect::GitAbortRebase => {
                // Abort rebase requires a process executor.
                AppEffectResult::Error(
                    "GitAbortRebase requires executor injection - use pipeline runner".to_string(),
                )
            }

            AppEffect::GitGetDefaultBranch => match crate::git_helpers::get_default_branch() {
                Ok(branch) => AppEffectResult::String(branch),
                Err(e) => AppEffectResult::Error(format!("Failed to get default branch: {e}")),
            },

            AppEffect::GitIsMainBranch => match crate::git_helpers::is_main_or_master_branch() {
                Ok(is_main) => AppEffectResult::Bool(is_main),
                Err(e) => AppEffectResult::Error(format!("Failed to check branch: {e}")),
            },

            // =========================================================================
            // Environment Effects
            // =========================================================================
            AppEffect::GetEnvVar { name } => match std::env::var(&name) {
                Ok(value) => AppEffectResult::String(value),
                Err(std::env::VarError::NotPresent) => {
                    AppEffectResult::Error(format!("Environment variable '{name}' not set"))
                }
                Err(std::env::VarError::NotUnicode(_)) => AppEffectResult::Error(format!(
                    "Environment variable '{name}' contains invalid Unicode"
                )),
            },

            AppEffect::SetEnvVar { name, value } => {
                std::env::set_var(&name, &value);
                AppEffectResult::Ok
            }

            // =========================================================================
            // Logging Effects
            // =========================================================================
            // Logging is handled elsewhere (by the logger), so these are no-ops.
            // The effect system captures them for testing/recording purposes.
            AppEffect::LogInfo { message: _ }
            | AppEffect::LogSuccess { message: _ }
            | AppEffect::LogWarn { message: _ }
            | AppEffect::LogError { message: _ } => AppEffectResult::Ok,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_handler_default() {
        let handler = RealAppEffectHandler::default();
        assert!(handler.workspace_root.is_none());
    }

    #[test]
    fn test_real_handler_with_workspace_root() {
        let root = PathBuf::from("/some/path");
        let handler = RealAppEffectHandler::with_workspace_root(root.clone());
        assert_eq!(handler.workspace_root, Some(root));
    }

    #[test]
    fn test_resolve_path_absolute() {
        let handler = RealAppEffectHandler::with_workspace_root(PathBuf::from("/workspace"));
        let absolute = PathBuf::from("/absolute/path");
        assert_eq!(handler.resolve_path(&absolute), absolute);
    }

    #[test]
    fn test_resolve_path_relative_with_root() {
        let handler = RealAppEffectHandler::with_workspace_root(PathBuf::from("/workspace"));
        let relative = PathBuf::from("relative/path");
        assert_eq!(
            handler.resolve_path(&relative),
            PathBuf::from("/workspace/relative/path")
        );
    }

    #[test]
    fn test_resolve_path_relative_without_root() {
        let handler = RealAppEffectHandler::new();
        let relative = PathBuf::from("relative/path");
        assert_eq!(handler.resolve_path(&relative), relative);
    }

    #[test]
    fn test_path_exists_effect() {
        let mut handler = RealAppEffectHandler::new();
        // Test with a path that definitely exists (this file's directory)
        let result = handler.execute(AppEffect::PathExists {
            path: PathBuf::from("."),
        });
        assert!(matches!(result, AppEffectResult::Bool(true)));
    }

    #[test]
    fn test_path_not_exists_effect() {
        let mut handler = RealAppEffectHandler::new();
        let result = handler.execute(AppEffect::PathExists {
            path: PathBuf::from("/nonexistent/path/that/should/not/exist/12345"),
        });
        assert!(matches!(result, AppEffectResult::Bool(false)));
    }

    #[test]
    fn test_get_env_var_effect() {
        let mut handler = RealAppEffectHandler::new();
        // PATH should always be set
        let result = handler.execute(AppEffect::GetEnvVar {
            name: "PATH".to_string(),
        });
        assert!(matches!(result, AppEffectResult::String(_)));
    }

    #[test]
    fn test_get_env_var_not_set() {
        let mut handler = RealAppEffectHandler::new();
        let result = handler.execute(AppEffect::GetEnvVar {
            name: "DEFINITELY_NOT_SET_ENV_VAR_12345".to_string(),
        });
        assert!(matches!(result, AppEffectResult::Error(_)));
    }

    #[test]
    fn test_set_env_var_effect() {
        let mut handler = RealAppEffectHandler::new();
        let var_name = "TEST_RALPH_ENV_VAR_12345";

        // Set the variable
        let result = handler.execute(AppEffect::SetEnvVar {
            name: var_name.to_string(),
            value: "test_value".to_string(),
        });
        assert!(matches!(result, AppEffectResult::Ok));

        // Verify it was set
        assert_eq!(std::env::var(var_name).ok(), Some("test_value".to_string()));

        // Clean up
        std::env::remove_var(var_name);
    }

    #[test]
    fn test_logging_effects_are_noops() {
        let mut handler = RealAppEffectHandler::new();

        let effects = vec![
            AppEffect::LogInfo {
                message: "test".to_string(),
            },
            AppEffect::LogSuccess {
                message: "test".to_string(),
            },
            AppEffect::LogWarn {
                message: "test".to_string(),
            },
            AppEffect::LogError {
                message: "test".to_string(),
            },
        ];

        for effect in effects {
            let result = handler.execute(effect);
            assert!(
                matches!(result, AppEffectResult::Ok),
                "Logging effect should return Ok"
            );
        }
    }

    #[test]
    fn test_rebase_effects_require_executor() {
        let mut handler = RealAppEffectHandler::new();

        let result = handler.execute(AppEffect::GitRebaseOnto {
            upstream_branch: "main".to_string(),
        });
        assert!(matches!(result, AppEffectResult::Error(_)));

        let result = handler.execute(AppEffect::GitContinueRebase);
        assert!(matches!(result, AppEffectResult::Error(_)));

        let result = handler.execute(AppEffect::GitAbortRebase);
        assert!(matches!(result, AppEffectResult::Error(_)));
    }
}
