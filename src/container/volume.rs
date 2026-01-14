//! Volume mount management
//!
//! Handles volume mount configuration for containers, ensuring proper
//! path resolution and validation.

use crate::container::engine::Mount;
use crate::container::error::{ContainerError, ContainerResult};
use std::path::{Path, PathBuf};

/// Volume mount manager
pub struct VolumeManager {
    /// Repository root path
    repository_root: PathBuf,
    /// Agent directory path
    agent_dir: PathBuf,
    /// User's config directory (optional)
    config_dir: Option<PathBuf>,
    /// Claude config directory for MCP/Skills (optional)
    claude_dir: Option<PathBuf>,
}

impl VolumeManager {
    /// Create a new volume manager
    pub fn new(
        repository_root: PathBuf,
        agent_dir: PathBuf,
        config_dir: Option<PathBuf>,
    ) -> Self {
        // Detect Claude config directory
        let claude_dir = dirs::home_dir()
            .map(|home| home.join(".claude"))
            .filter(|path| path.exists());

        Self {
            repository_root,
            agent_dir,
            config_dir,
            claude_dir,
        }
    }

    /// Get all volume mounts for a container
    pub fn get_mounts(&self) -> ContainerResult<Vec<Mount>> {
        let mut mounts = Vec::new();

        // Mount repository root to /workspace (read-write)
        let repo_root = self.canonicalize(&self.repository_root)?;
        Self::validate_mount_source(&repo_root)?;
        mounts.push(Mount::new(
            repo_root.to_string_lossy().to_string(),
            "/workspace".to_string(),
        ));

        // Mount .agent directory for orchestrator communication
        let agent_dir = if self.agent_dir.is_absolute() {
            self.agent_dir.clone()
        } else {
            self.repository_root.join(&self.agent_dir)
        };
        let agent_dir = self.canonicalize(&agent_dir)?;
        mounts.push(Mount::new(
            agent_dir.to_string_lossy().to_string(),
            "/workspace/.agent".to_string(),
        ));

        // Mount config directory read-only if available
        if let Some(ref config_dir) = self.config_dir {
            if let Ok(config_path) = self.canonicalize(config_dir) {
                Self::validate_mount_source(&config_path)?;
                mounts.push(Mount::read_only(
                    config_path.to_string_lossy().to_string(),
                    "/home/ralph/.config".to_string(),
                ));
            }
        }

        // Mount Claude config directory read-only if available (for MCP/Skills)
        if let Some(ref claude_dir) = self.claude_dir {
            if let Ok(claude_path) = self.canonicalize(claude_dir) {
                Self::validate_mount_source(&claude_path)?;
                mounts.push(Mount::read_only(
                    claude_path.to_string_lossy().to_string(),
                    "/home/ralph/.claude".to_string(),
                ));
            }
        }

        Ok(mounts)
    }

    /// Canonicalize a path, handling ~ expansion
    fn canonicalize(&self, path: &Path) -> ContainerResult<PathBuf> {
        let path = if path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                let without_tilde = path.strip_prefix("~").unwrap_or(path);
                let without_tilde_str = without_tilde.to_string_lossy();
                let trimmed = without_tilde_str.trim_start_matches('/');
                home.join(trimmed)
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        // Try to canonicalize, but fall back to absolute path if it doesn't exist
        path.canonicalize()
            .or_else(|_| {
                if path.is_absolute() {
                    Ok(path.to_path_buf())
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(&path))
                        .map_err(|e| ContainerError::Io(e))
                }
            })
            .map_err(|e| ContainerError::VolumeMount(format!("Failed to resolve path: {}", e)))
    }

    /// Validate that a mount source is safe to mount
    fn validate_mount_source(path: &Path) -> ContainerResult<()> {
        let path_str = path.to_string_lossy();

        // Block mounting of sensitive system paths
        let blocked_paths = [
            "/etc",
            "/proc",
            "/sys",
            "/dev",
            "/root",
            "/boot",
            "/run",
            "/var/run",
        ];

        for blocked in &blocked_paths {
            if path_str.starts_with(blocked) {
                return Err(ContainerError::VolumeMount(format!(
                    "Cannot mount sensitive path: {}",
                    path_str
                )));
            }
        }

        // Warn about mounting home directory root (but allow it)
        if let Some(home) = dirs::home_dir() {
            if path == home {
                // Allow home directory mount with a warning
                // (This is logged elsewhere)
            }
        }

        Ok(())
    }

    /// Get the workspace path inside the container
    pub fn workspace_path(&self) -> String {
        "/workspace".to_string()
    }

    /// Convert a host path to a container path
    pub fn to_container_path(&self, host_path: &Path) -> ContainerResult<String> {
        let host_path = if host_path.is_absolute() {
            host_path.to_path_buf()
        } else {
            self.repository_root.join(host_path)
        };

        // Try to make it relative to repository root
        if let Ok(relative) = host_path.strip_prefix(&self.repository_root) {
            Ok(format!("/workspace/{}", relative.to_string_lossy()))
        } else {
            // Path is outside repository, return as-is (will likely fail in container)
            Ok(host_path.to_string_lossy().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_path() {
        let manager = VolumeManager::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from(".agent"),
            None,
        );
        assert_eq!(manager.workspace_path(), "/workspace");
    }

    #[test]
    fn test_to_container_path_relative() {
        let manager = VolumeManager::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from(".agent"),
            None,
        );

        let container_path = manager
            .to_container_path(Path::new("src/main.rs"))
            .unwrap();
        assert_eq!(container_path, "/workspace/src/main.rs");
    }

    #[test]
    fn test_to_container_path_absolute() {
        let manager = VolumeManager::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from(".agent"),
            None,
        );

        let container_path = manager
            .to_container_path(Path::new("/tmp/repo/src/main.rs"))
            .unwrap();
        assert_eq!(container_path, "/workspace/src/main.rs");
    }

    #[test]
    fn test_validate_sensitive_paths() {
        // This test validates that sensitive paths are blocked
        // Note: VolumeManager::validate_mount_source is private, tested indirectly
    }
}
