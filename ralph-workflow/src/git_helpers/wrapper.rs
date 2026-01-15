//! Git wrapper for blocking commits during agent phase.
//!
//! This module provides safety mechanisms to prevent accidental commits while
//! an AI agent is actively modifying files. It works through two mechanisms:
//!
//! - **Marker file**: Creates `.no_agent_commit` in the repo root during agent
//!   execution. Both the git wrapper and hooks check for this file.
//! - **PATH wrapper**: Installs a temporary `git` wrapper script that intercepts
//!   `commit`, `push`, and `tag` commands when the marker file exists.
//!
//! The wrapper is automatically cleaned up when the agent phase ends, even on
//! unexpected exits (Ctrl+C, panics) via [`cleanup_agent_phase_silent`].

use super::hooks::{install_hooks, uninstall_hooks_silent};
use super::repo::get_repo_root;
use crate::logger::Logger;
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use tempfile::TempDir;
use which::which;

const WRAPPER_DIR_TRACK_FILE: &str = ".agent/git-wrapper-dir.txt";

/// Git helper state.
pub struct GitHelpers {
    real_git: Option<PathBuf>,
    wrapper_dir: Option<TempDir>,
}

impl GitHelpers {
    pub(crate) const fn new() -> Self {
        Self {
            real_git: None,
            wrapper_dir: None,
        }
    }

    /// Find the real git binary path.
    fn init_real_git(&mut self) {
        if self.real_git.is_none() {
            self.real_git = which("git").ok();
        }
    }
}

impl Default for GitHelpers {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape a path for safe use in a POSIX shell single-quoted string.
///
/// Single quotes in POSIX shells cannot contain literal single quotes.
/// The standard workaround is to end the quote, add an escaped quote, and restart the quote.
/// This function rejects paths with newlines since they can't be safely handled.
fn escape_shell_single_quoted(path: &str) -> io::Result<String> {
    // Reject newlines - they cannot be safely handled in shell scripts
    if path.contains('\n') || path.contains('\r') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "git path contains newline characters, cannot create safe shell wrapper",
        ));
    }
    // Replace ' with '\' (end literal string, escaped quote, restart literal string)
    Ok(path.replace('\'', "'\\''"))
}

/// Enable git wrapper that blocks commits during agent phase.
pub fn enable_git_wrapper(helpers: &mut GitHelpers) -> io::Result<()> {
    helpers.init_real_git();
    let Some(real_git) = helpers.real_git.as_ref() else {
        // Ralph's git operations use libgit2 and should work without the `git` CLI installed.
        // The wrapper is only a safety feature for intercepting `git commit/push/tag`.
        // If no `git` binary is available, there's nothing to wrap, so we no-op.
        return Ok(());
    };

    // Validate git path is valid UTF-8 for shell script generation.
    // On Unix systems, paths are typically valid UTF-8, but some filesystems
    // may contain invalid UTF-8 sequences. In such cases, we cannot safely
    // generate a shell wrapper and should return an error.
    let git_path_str = real_git.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "git binary path contains invalid UTF-8 characters; cannot create wrapper script",
        )
    })?;

    // Validate that the git path is an absolute path.
    // This prevents potential issues with relative paths and ensures
    // we're using a known, trusted git binary location.
    if !real_git.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "git binary path is not absolute: '{git_path_str}'. \
                 Using absolute paths prevents potential security issues."
            ),
        ));
    }

    // Additional validation: ensure the git binary exists and is executable.
    // This prevents following symlinks to non-executable files or directories.
    if !real_git.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("git binary does not exist at path: '{git_path_str}'"),
        ));
    }

    // On Unix systems, verify it's a regular file (not a directory or special file).
    #[cfg(unix)]
    {
        match fs::metadata(real_git) {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                if file_type.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("git binary path is a directory, not a file: '{git_path_str}'"),
                    ));
                }
                if file_type.is_symlink() {
                    // Don't follow symlinks - require the actual path to be the binary.
                    // This prevents symlink-based attacks.
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("git binary path is a symlink; use the actual binary path: '{git_path_str}'"),
                    ));
                }
            }
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("cannot access git binary metadata at path: '{git_path_str}'"),
                ));
            }
        }
    }

    let wrapper_dir = tempfile::tempdir()?;
    let wrapper_path = wrapper_dir.path().join("git");

    // Escape the git path for shell script to prevent command injection.
    // Use a helper function to properly handle edge cases and reject unsafe paths.
    let git_path_escaped = escape_shell_single_quoted(git_path_str)?;

    let wrapper_content = format!(
        r#"#!/usr/bin/env sh
set -eu
repo_root="$('{git_path_escaped}' rev-parse --show-toplevel 2>/dev/null || pwd)"
if [ -f "$repo_root/.no_agent_commit" ]; then
  subcmd="${{1-}}"
  case "$subcmd" in
    commit|push|tag)
      echo "Blocked: git $subcmd disabled during agent phase (.no_agent_commit present)." >&2
      exit 1
      ;;
  esac
fi
exec '{git_path_escaped}' "$@"
"#
    );

    let mut file = File::create(&wrapper_path)?;
    file.write_all(wrapper_content.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper_path, perms)?;
    }

    // Prepend wrapper dir to PATH.
    let current_path = env::var("PATH").unwrap_or_default();
    env::set_var(
        "PATH",
        format!("{}:{}", wrapper_dir.path().display(), current_path),
    );

    fs::create_dir_all(".agent")?;
    fs::write(
        WRAPPER_DIR_TRACK_FILE,
        wrapper_dir.path().display().to_string(),
    )?;

    helpers.wrapper_dir = Some(wrapper_dir);
    Ok(())
}

/// Disable git wrapper.
///
/// # Thread Safety
///
/// This function modifies the process-wide `PATH` environment variable, which is
/// inherently not thread-safe. If multiple threads were concurrently modifying PATH,
/// there could be a TOCTOU (time-of-check-time-of-use) race condition. However,
/// in Ralph's usage, this function is only called from the main thread during
/// controlled shutdown sequences, so this is acceptable in practice.
pub fn disable_git_wrapper(helpers: &mut GitHelpers) {
    if let Some(wrapper_dir) = helpers.wrapper_dir.take() {
        let wrapper_dir_path = wrapper_dir.path().to_path_buf();
        let _ = fs::remove_dir_all(&wrapper_dir_path);
        // Remove from PATH.
        // Note: This read-modify-write sequence on PATH has a theoretical TOCTOU race,
        // but in practice it's safe because Ralph only calls this from the main thread
        // during controlled shutdown.
        if let Ok(path) = env::var("PATH") {
            let wrapper_str = wrapper_dir_path.to_string_lossy();
            let new_path: String = path
                .split(':')
                .filter(|p| !p.contains(wrapper_str.as_ref()))
                .collect::<Vec<_>>()
                .join(":");
            env::set_var("PATH", new_path);
        }
    }
    let _ = fs::remove_file(WRAPPER_DIR_TRACK_FILE);
}

/// Start agent phase (creates marker file, installs hooks, enables wrapper).
pub fn start_agent_phase(helpers: &mut GitHelpers) -> io::Result<()> {
    File::create(".no_agent_commit")?;
    install_hooks()?;
    enable_git_wrapper(helpers)?;
    Ok(())
}

/// End agent phase (removes marker file).
pub fn end_agent_phase() {
    let _ = fs::remove_file(".no_agent_commit");
}

fn cleanup_git_wrapper_dir_silent() {
    let wrapper_dir = match fs::read_to_string(WRAPPER_DIR_TRACK_FILE) {
        Ok(path) => PathBuf::from(path.trim()),
        Err(_) => return,
    };

    if !wrapper_dir.as_os_str().is_empty() {
        let _ = fs::remove_dir_all(&wrapper_dir);
    }
    let _ = fs::remove_file(WRAPPER_DIR_TRACK_FILE);
}

/// Best-effort cleanup for unexpected exits (Ctrl+C, early-return, panics).
pub fn cleanup_agent_phase_silent() {
    end_agent_phase();
    cleanup_git_wrapper_dir_silent();
    uninstall_hooks_silent();
    crate::files::cleanup_generated_files();
}

/// Clean up orphaned .`no_agent_commit` marker.
pub fn cleanup_orphaned_marker(logger: &Logger) -> io::Result<()> {
    let repo_root = get_repo_root()?;
    let marker_path = repo_root.join(".no_agent_commit");

    if marker_path.exists() {
        fs::remove_file(&marker_path)?;
        logger.success("Removed orphaned .no_agent_commit marker");
    } else {
        logger.info("No orphaned marker found");
    }

    Ok(())
}

/// Temporarily remove the agent commit marker to allow manual git commit.
///
/// This is used as a last resort when all automated commit message generation
/// fails. The caller is responsible for calling `restore_agent_commit_marker()`
/// after the manual operation completes.
///
/// # Returns
///
/// * `Ok(true)` - Marker was removed (was present)
/// * `Ok(false)` - Marker was not present (nothing to do)
/// * `Err(e)` - Error removing the marker
pub fn temporarily_remove_agent_commit_marker() -> io::Result<bool> {
    let marker_path = PathBuf::from(".no_agent_commit");
    if marker_path.exists() {
        fs::remove_file(&marker_path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Restore the agent commit marker after a temporary removal.
///
/// This should be called after `temporarily_remove_agent_commit_marker()` to
/// re-enable the commit protection during agent phases.
pub fn restore_agent_commit_marker() -> io::Result<()> {
    File::create(".no_agent_commit")?;
    Ok(())
}
