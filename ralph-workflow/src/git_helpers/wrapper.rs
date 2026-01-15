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
use which::which;

const WRAPPER_DIR_TRACK_FILE: &str = ".agent/git-wrapper-dir.txt";

/// Git helper state.
pub struct GitHelpers {
    _private: (),
}

impl GitHelpers {
    pub(crate) const fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for GitHelpers {
    fn default() -> Self {
        Self::new()
    }
}

/// Enable git wrapper that blocks commits during agent phase.
pub fn enable_git_wrapper(_helpers: &mut GitHelpers) -> io::Result<()> {
    let Some(real_git) = which("git").ok() else {
        // Ralph's git operations use libgit2 and should work without the `git` CLI installed.
        // The wrapper is only a safety feature for intercepting `git commit/push/tag`.
        // If no `git` binary is available, there's nothing to wrap, so we no-op.
        return Ok(());
    };

    // Validate the git path doesn't contain shell metacharacters that could cause injection
    let git_path_str = real_git.to_string_lossy();
    if git_path_str.contains('\0') || git_path_str.contains('\n') || git_path_str.contains('\r') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git path contains invalid characters for shell script",
        ));
    }
    // Check for shell metacharacters that could be used for injection
    let shell_metachars = [
        '$', '`', '\\', '"', '\'', ';', '&', '|', '(', ')', '<', '>', '[', ']', '{', '}', '!', '*',
        '?', '~', '#', ' ',
    ];
    for c in &shell_metachars {
        if git_path_str.contains(*c) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Git path contains shell metacharacter '{c}' which is unsafe for shell script"
                ),
            ));
        }
    }

    let wrapper_dir = ::tempfile::tempdir()?;
    let wrapper_path = wrapper_dir.path().join("git");

    // Properly escape the git path for shell script to prevent command injection.
    // Replace single quotes with '\'' (end quote, escaped quote, start quote) and
    // wrap the entire path in single quotes.
    let git_path_escaped = real_git
        .to_str()
        .expect("git path must be valid UTF-8")
        .replace('\'', "'\\''");

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

    // Keep wrapper_dir alive until the function returns via TempDir's Drop
    std::mem::forget(wrapper_dir);
    Ok(())
}

/// Disable git wrapper.
pub fn disable_git_wrapper(_helpers: &mut GitHelpers) {
    let wrapper_dir = match fs::read_to_string(WRAPPER_DIR_TRACK_FILE) {
        Ok(path) => PathBuf::from(path.trim()),
        Err(_) => return,
    };

    if !wrapper_dir.as_os_str().is_empty() {
        let _ = fs::remove_dir_all(&wrapper_dir);
        // Remove from PATH.
        if let Ok(path) = env::var("PATH") {
            let wrapper_str = wrapper_dir.to_string_lossy();
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
    crate::common::utils::cleanup_generated_files();
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
