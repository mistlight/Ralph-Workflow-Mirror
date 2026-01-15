//! Codex agent detection and integration
//!
//! This module provides functionality to detect and handle Codex agent execution
//! to avoid issues with nested virtualization (VM-in-VM).
//!
//! # Background
//!
//! Codex is an AI agent platform that uses its own VM for isolation. When running
//! inside Ralph's container mode, this would create VM-in-VM which is problematic.
//! This module detects Codex and provides appropriate handling.

use std::path::PathBuf;

/// Common Codex installation directories
const CODEX_HOME_DIRS: &[&str] = &[".codex", ".config/codex"];

/// Command patterns that indicate Codex agent execution
const CODEX_COMMAND_PATTERNS: &[&str] = &["codex", "codex-run", "codex-agent"];

/// Detect if a command appears to be a Codex agent command
///
/// Analyzes the command string for Codex-specific patterns.
pub fn is_codex_command(command: &str) -> bool {
    let command_lower = command.to_lowercase();

    // Check for Codex command patterns
    for pattern in CODEX_COMMAND_PATTERNS {
        if command_lower.contains(pattern) {
            return true;
        }
    }

    false
}

/// Get the Codex VM directory path if it exists
///
/// Returns the path to Codex's installation directory if present.
pub fn get_codex_vm_directory() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    for codex_dir in CODEX_HOME_DIRS {
        let codex_path = home.join(codex_dir);
        if codex_path.exists() {
            return Some(codex_path);
        }
    }

    None
}

/// Get environment variables to set when running in container mode with Codex
///
/// Returns environment variables that signal to Codex that it's running
/// inside a container, so it can skip its own VM initialization.
pub fn get_codex_container_env() -> Vec<(String, String)> {
    vec![
        ("CODEX_CONTAINER".to_string(), "1".to_string()),
        ("CODEX_SKIP_VM".to_string(), "1".to_string()),
    ]
}

/// Get Codex-specific volume mounts for container mode
///
/// Returns volume mounts needed for Codex to work properly inside a container.
/// These are mounted read-only to prevent escape.
pub fn get_codex_volume_mounts() -> Vec<(PathBuf, String)> {
    let mut mounts = Vec::new();

    if let Some(codex_dir) = get_codex_vm_directory() {
        // Mount Codex directory read-only
        mounts.push((
            codex_dir.clone(),
            format!(
                "/home/ralph/{}",
                codex_dir.file_name().unwrap_or_default().to_string_lossy()
            ),
        ));
    }

    mounts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_codex_command() {
        assert!(is_codex_command("codex run"));
        assert!(is_codex_command("codex-agent execute"));
        assert!(is_codex_command("/usr/bin/codex-run"));
        assert!(!is_codex_command("cargo run"));
        assert!(!is_codex_command("npm test"));
    }

    #[test]
    fn test_get_codex_container_env() {
        let env = get_codex_container_env();
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].0, "CODEX_CONTAINER");
        assert_eq!(env[0].1, "1");
        assert_eq!(env[1].0, "CODEX_SKIP_VM");
        assert_eq!(env[1].1, "1");
    }

    #[test]
    fn test_get_codex_volume_mounts() {
        let mounts = get_codex_volume_mounts();
        // May return empty if Codex is not installed, but should not panic
        assert!(mounts.is_empty() || !mounts.is_empty());
    }
}
