//! Configuration environment abstraction.
//!
//! This module provides the [`ConfigEnvironment`] trait that abstracts all
//! external side effects needed for configuration operations:
//! - Environment variable access (for path resolution)
//! - Filesystem operations (for reading/writing config files)
//!
//! # Design Philosophy
//!
//! Configuration types like `UnifiedConfig` should be pure data structures.
//! All side effects (env vars, file I/O) are injected through this trait,
//! making the code testable without mocking globals.
//!
//! # Dependency Injection
//!
//! Production code uses [`RealConfigEnvironment`] which reads from actual
//! environment variables and performs real filesystem operations. Tests use
//! [`MemoryConfigEnvironment`] with in-memory storage for both.
//!
//! # Example
//!
//! ```ignore
//! use crate::config::{ConfigEnvironment, RealConfigEnvironment, MemoryConfigEnvironment};
//!
//! // Production: uses real env vars and filesystem
//! let env = RealConfigEnvironment;
//! let config_path = env.unified_config_path();
//!
//! // Testing: uses in-memory storage
//! let env = MemoryConfigEnvironment::new()
//!     .with_unified_config_path("/test/config/ralph-workflow.toml")
//!     .with_prompt_path("/test/repo/PROMPT.md")
//!     .with_file("/test/repo/PROMPT.md", "# Goal\nTest");
//! ```

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Trait for configuration environment access.
///
/// This trait abstracts all external side effects needed for configuration:
/// - Path resolution (which may depend on environment variables)
/// - File existence checks
/// - File reading and writing
/// - Directory creation
///
/// By injecting this trait, configuration code becomes pure and testable.
pub trait ConfigEnvironment: Send + Sync {
    /// Get the path to the unified config file.
    ///
    /// In production, this returns `~/.config/ralph-workflow.toml` or
    /// `$XDG_CONFIG_HOME/ralph-workflow.toml` if the env var is set.
    ///
    /// Returns `None` if the path cannot be determined (e.g., no home directory).
    fn unified_config_path(&self) -> Option<PathBuf>;

    /// Get the path to the local config file.
    ///
    /// In production, this returns `.agent/ralph-workflow.toml` relative to CWD.
    /// Tests may override this to use a different path.
    ///
    /// Returns `None` if local config is not supported or path cannot be determined.
    fn local_config_path(&self) -> Option<PathBuf> {
        Some(PathBuf::from(".agent/ralph-workflow.toml"))
    }

    /// Get the path to the PROMPT.md file.
    ///
    /// In production, this returns `./PROMPT.md` (relative to current directory).
    /// Tests may override this to use a different path.
    fn prompt_path(&self) -> PathBuf {
        PathBuf::from("PROMPT.md")
    }

    /// Check if a file exists at the given path.
    fn file_exists(&self, path: &Path) -> bool;

    /// Read the contents of a file.
    fn read_file(&self, path: &Path) -> io::Result<String>;

    /// Write content to a file, creating parent directories if needed.
    fn write_file(&self, path: &Path, content: &str) -> io::Result<()>;

    /// Create directories recursively.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Get the root of the current git worktree, if running inside one.
    ///
    /// Returns `None` if not in a git repository or in a bare repository.
    /// This is used to resolve local config paths relative to the worktree root
    /// instead of the current working directory.
    fn worktree_root(&self) -> Option<PathBuf> {
        None // Default implementation for backwards compatibility
    }
}

/// Production implementation of [`ConfigEnvironment`].
///
/// Uses real environment variables and filesystem operations:
/// - Reads `XDG_CONFIG_HOME` for config path resolution
/// - Uses `std::fs` for all file operations
#[derive(Debug, Default, Clone, Copy)]
pub struct RealConfigEnvironment;

impl ConfigEnvironment for RealConfigEnvironment {
    fn unified_config_path(&self) -> Option<PathBuf> {
        super::unified::unified_config_path()
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_file(&self, path: &Path, content: &str) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn worktree_root(&self) -> Option<PathBuf> {
        git2::Repository::discover(".")
            .ok()
            .and_then(|repo| repo.workdir().map(PathBuf::from))
    }

    fn local_config_path(&self) -> Option<PathBuf> {
        // Try worktree root first, fall back to default behavior
        self.worktree_root()
            .map(|root| root.join(".agent/ralph-workflow.toml"))
            .or_else(|| Some(PathBuf::from(".agent/ralph-workflow.toml")))
    }
}

/// In-memory implementation of [`ConfigEnvironment`] for testing.
///
/// Provides complete isolation from the real environment:
/// - Injected paths instead of environment variables
/// - In-memory file storage instead of real filesystem
///
/// # Example
///
/// ```ignore
/// use crate::config::MemoryConfigEnvironment;
///
/// let env = MemoryConfigEnvironment::new()
///     .with_unified_config_path("/test/config/ralph-workflow.toml")
///     .with_prompt_path("/test/repo/PROMPT.md")
///     .with_file("/test/repo/existing.txt", "content");
///
/// // Write a file
/// env.write_file(Path::new("/test/new.txt"), "new content")?;
///
/// // Verify it was written
/// assert!(env.was_written(Path::new("/test/new.txt")));
/// assert_eq!(env.get_file(Path::new("/test/new.txt")), Some("new content".to_string()));
/// ```
#[derive(Debug, Clone, Default)]
pub struct MemoryConfigEnvironment {
    unified_config_path: Option<PathBuf>,
    prompt_path: Option<PathBuf>,
    local_config_path: Option<PathBuf>,
    worktree_root: Option<PathBuf>,
    /// In-memory file storage.
    files: Arc<RwLock<HashMap<PathBuf, String>>>,
    /// Directories that have been created.
    dirs: Arc<RwLock<std::collections::HashSet<PathBuf>>>,
}

impl MemoryConfigEnvironment {
    /// Create a new memory environment with no paths configured.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the unified config path.
    #[must_use]
    pub fn with_unified_config_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.unified_config_path = Some(path.into());
        self
    }

    /// Set the local config path.
    #[must_use]
    pub fn with_local_config_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.local_config_path = Some(path.into());
        self
    }

    /// Set the PROMPT.md path.
    #[must_use]
    pub fn with_prompt_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.prompt_path = Some(path.into());
        self
    }

    /// Pre-populate a file in memory.
    #[must_use]
    pub fn with_file<P: Into<PathBuf>, S: Into<String>>(self, path: P, content: S) -> Self {
        let path = path.into();
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment files lock")
            .insert(path, content.into());
        self
    }

    /// Set the worktree root path for testing git worktree scenarios.
    #[must_use]
    pub fn with_worktree_root<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.worktree_root = Some(path.into());
        self
    }

    /// Get the contents of a file (for test assertions).
    pub fn get_file(&self, path: &Path) -> Option<String> {
        self.files.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment files lock")
            .get(path).cloned()
    }

    /// Check if a file was written (for test assertions).
    pub fn was_written(&self, path: &Path) -> bool {
        self.files.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment files lock")
            .contains_key(path)
    }
}

impl ConfigEnvironment for MemoryConfigEnvironment {
    fn unified_config_path(&self) -> Option<PathBuf> {
        self.unified_config_path.clone()
    }

    fn local_config_path(&self) -> Option<PathBuf> {
        // If explicit local_config_path was set, use it (for legacy tests)
        if let Some(ref path) = self.local_config_path {
            return Some(path.clone());
        }

        // Otherwise, use worktree root if available
        self.worktree_root()
            .map(|root| root.join(".agent/ralph-workflow.toml"))
            .or_else(|| Some(PathBuf::from(".agent/ralph-workflow.toml")))
    }

    fn prompt_path(&self) -> PathBuf {
        self.prompt_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("PROMPT.md"))
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.files.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment files lock")
            .contains_key(path)
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment files lock")
            .get(path)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", path.display()),
                )
            })
    }

    fn write_file(&self, path: &Path, content: &str) -> io::Result<()> {
        // Simulate creating parent directories
        if let Some(parent) = path.parent() {
            self.dirs.write()
                .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment dirs lock")
                .insert(parent.to_path_buf());
        }
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment files lock")
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.dirs.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryConfigEnvironment dirs lock")
            .insert(path.to_path_buf());
        Ok(())
    }

    fn worktree_root(&self) -> Option<PathBuf> {
        self.worktree_root.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_environment_returns_path() {
        let env = RealConfigEnvironment;
        // Should return Some path (unless running in weird environment without home dir)
        let path = env.unified_config_path();
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("ralph-workflow.toml"));
        }
    }

    #[test]
    fn test_memory_environment_with_custom_paths() {
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/custom/config.toml")
            .with_prompt_path("/custom/PROMPT.md");

        assert_eq!(
            env.unified_config_path(),
            Some(PathBuf::from("/custom/config.toml"))
        );
        assert_eq!(env.prompt_path(), PathBuf::from("/custom/PROMPT.md"));
    }

    #[test]
    fn test_memory_environment_default_prompt_path() {
        let env = MemoryConfigEnvironment::new();
        assert_eq!(env.prompt_path(), PathBuf::from("PROMPT.md"));
    }

    #[test]
    fn test_memory_environment_no_unified_config() {
        let env = MemoryConfigEnvironment::new();
        assert_eq!(env.unified_config_path(), None);
    }

    #[test]
    fn test_memory_environment_file_operations() {
        let env = MemoryConfigEnvironment::new();
        let path = Path::new("/test/file.txt");

        // File doesn't exist initially
        assert!(!env.file_exists(path));

        // Write file
        env.write_file(path, "test content").unwrap();

        // File now exists
        assert!(env.file_exists(path));
        assert_eq!(env.read_file(path).unwrap(), "test content");
        assert!(env.was_written(path));
    }

    #[test]
    fn test_memory_environment_with_prepopulated_file() {
        let env =
            MemoryConfigEnvironment::new().with_file("/test/existing.txt", "existing content");

        assert!(env.file_exists(Path::new("/test/existing.txt")));
        assert_eq!(
            env.read_file(Path::new("/test/existing.txt")).unwrap(),
            "existing content"
        );
    }

    #[test]
    fn test_memory_environment_read_nonexistent_file() {
        let env = MemoryConfigEnvironment::new();
        let result = env.read_file(Path::new("/nonexistent"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_memory_environment_with_worktree_root() {
        let env = MemoryConfigEnvironment::new().with_worktree_root("/test/worktree");

        assert_eq!(env.worktree_root(), Some(PathBuf::from("/test/worktree")));
        assert_eq!(
            env.local_config_path(),
            Some(PathBuf::from("/test/worktree/.agent/ralph-workflow.toml"))
        );
    }

    #[test]
    fn test_memory_environment_without_worktree_root() {
        let env = MemoryConfigEnvironment::new();

        assert_eq!(env.worktree_root(), None);
        assert_eq!(
            env.local_config_path(),
            Some(PathBuf::from(".agent/ralph-workflow.toml"))
        );
    }

    #[test]
    fn test_memory_environment_explicit_local_path_overrides_worktree() {
        let env = MemoryConfigEnvironment::new()
            .with_worktree_root("/test/worktree")
            .with_local_config_path("/custom/path/config.toml");

        // Explicit local_config_path should take precedence
        assert_eq!(
            env.local_config_path(),
            Some(PathBuf::from("/custom/path/config.toml"))
        );
    }
}
