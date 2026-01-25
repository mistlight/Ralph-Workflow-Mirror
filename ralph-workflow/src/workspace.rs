//! Workspace filesystem abstraction for explicit path resolution.
//!
//! This module provides the `WorkspaceFs` struct that eliminates CWD dependencies
//! by making all path operations explicit relative to the repository root.
//!
//! # Problem
//!
//! The codebase previously relied on `std::env::set_current_dir()` to set the
//! process CWD to the repository root, then used relative paths (`.agent/`,
//! `PROMPT.md`, etc.) throughout. This caused:
//!
//! - Test flakiness when tests ran in parallel (CWD is process-global)
//! - Background thread bugs when CWD changed after thread started
//! - Poor testability without complex CWD manipulation
//!
//! # Solution
//!
//! `WorkspaceFs` holds the repository root path and provides methods for:
//! - Getting paths to well-known files (`.agent/PLAN.md`, `PROMPT.md`, etc.)
//! - Performing file operations relative to the repo root
//! - Resolving relative paths to absolute paths (for agent prompts)
//!
//! # Example
//!
//! ```ignore
//! use ralph_workflow::workspace::WorkspaceFs;
//! use std::path::PathBuf;
//!
//! let ws = WorkspaceFs::new(PathBuf::from("/path/to/repo"));
//!
//! // Get paths to well-known files
//! let plan = ws.plan_md();  // /path/to/repo/.agent/PLAN.md
//! let prompt = ws.prompt_md();  // /path/to/repo/PROMPT.md
//!
//! // Perform file operations
//! ws.write(".agent/test.txt", "content")?;
//! let content = ws.read(".agent/test.txt")?;
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Workspace filesystem abstraction that resolves all paths relative to repo_root.
///
/// This eliminates CWD dependencies by making all path operations explicit.
/// All methods that accept paths treat them as relative to the repository root.
#[derive(Debug, Clone)]
pub struct WorkspaceFs {
    root: PathBuf,
}

impl WorkspaceFs {
    /// Create a new workspace filesystem rooted at the given path.
    ///
    /// # Arguments
    ///
    /// * `repo_root` - The repository root directory (typically discovered via git)
    pub fn new(repo_root: PathBuf) -> Self {
        Self { root: repo_root }
    }

    /// Get the repository root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    // =========================================================================
    // Agent directory paths
    // =========================================================================

    /// Path to the `.agent` directory.
    pub fn agent_dir(&self) -> PathBuf {
        self.root.join(".agent")
    }

    /// Path to the `.agent/logs` directory.
    pub fn agent_logs(&self) -> PathBuf {
        self.root.join(".agent/logs")
    }

    /// Path to the `.agent/tmp` directory.
    pub fn agent_tmp(&self) -> PathBuf {
        self.root.join(".agent/tmp")
    }

    /// Path to the `.agent/xsd` directory (XSD schemas for agent validation).
    pub fn agent_xsd(&self) -> PathBuf {
        self.root.join(".agent/xsd")
    }

    // =========================================================================
    // Well-known file paths
    // =========================================================================

    /// Path to `.agent/PLAN.md`.
    pub fn plan_md(&self) -> PathBuf {
        self.root.join(".agent/PLAN.md")
    }

    /// Path to `.agent/ISSUES.md`.
    pub fn issues_md(&self) -> PathBuf {
        self.root.join(".agent/ISSUES.md")
    }

    /// Path to `.agent/STATUS.md`.
    pub fn status_md(&self) -> PathBuf {
        self.root.join(".agent/STATUS.md")
    }

    /// Path to `.agent/NOTES.md`.
    pub fn notes_md(&self) -> PathBuf {
        self.root.join(".agent/NOTES.md")
    }

    /// Path to `.agent/commit-message.txt`.
    pub fn commit_message(&self) -> PathBuf {
        self.root.join(".agent/commit-message.txt")
    }

    /// Path to `.agent/checkpoint.json`.
    pub fn checkpoint(&self) -> PathBuf {
        self.root.join(".agent/checkpoint.json")
    }

    /// Path to `.agent/start_commit`.
    pub fn start_commit(&self) -> PathBuf {
        self.root.join(".agent/start_commit")
    }

    /// Path to `.agent/review_baseline.txt`.
    pub fn review_baseline(&self) -> PathBuf {
        self.root.join(".agent/review_baseline.txt")
    }

    /// Path to `PROMPT.md` in the repository root.
    pub fn prompt_md(&self) -> PathBuf {
        self.root.join("PROMPT.md")
    }

    /// Path to `.agent/PROMPT.md.backup`.
    pub fn prompt_backup(&self) -> PathBuf {
        self.root.join(".agent/PROMPT.md.backup")
    }

    /// Path to `.agent/config.toml`.
    pub fn agent_config(&self) -> PathBuf {
        self.root.join(".agent/config.toml")
    }

    /// Path to `.agent/agents.toml`.
    pub fn agents_toml(&self) -> PathBuf {
        self.root.join(".agent/agents.toml")
    }

    /// Path to `.agent/logs/pipeline.log`.
    pub fn pipeline_log(&self) -> PathBuf {
        self.root.join(".agent/logs/pipeline.log")
    }

    // =========================================================================
    // Dynamic path helpers
    // =========================================================================

    /// Path to an XSD schema file in `.agent/tmp/`.
    ///
    /// # Arguments
    ///
    /// * `name` - Schema name without extension (e.g., "plan", "issues")
    pub fn xsd_path(&self, name: &str) -> PathBuf {
        self.root.join(format!(".agent/tmp/{}.xsd", name))
    }

    /// Path to an XML file in `.agent/tmp/`.
    ///
    /// # Arguments
    ///
    /// * `name` - File name without extension (e.g., "plan", "issues")
    pub fn xml_path(&self, name: &str) -> PathBuf {
        self.root.join(format!(".agent/tmp/{}.xml", name))
    }

    /// Path to a log file in `.agent/logs/`.
    ///
    /// # Arguments
    ///
    /// * `name` - Log file name (with or without extension)
    pub fn log_path(&self, name: &str) -> PathBuf {
        self.root.join(format!(".agent/logs/{}", name))
    }

    // =========================================================================
    // File operations
    // =========================================================================

    /// Read a file relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn read(&self, relative: impl AsRef<Path>) -> io::Result<String> {
        fs::read_to_string(self.root.join(relative))
    }

    /// Read a file as bytes relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn read_bytes(&self, relative: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        fs::read(self.root.join(relative))
    }

    /// Write content to a file relative to the repository root.
    ///
    /// Creates parent directories if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    /// * `content` - Content to write
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn write(&self, relative: impl AsRef<Path>, content: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    /// Write bytes to a file relative to the repository root.
    ///
    /// Creates parent directories if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    /// * `content` - Bytes to write
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn write_bytes(&self, relative: impl AsRef<Path>, content: &[u8]) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    /// Check if a path exists relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    pub fn exists(&self, relative: impl AsRef<Path>) -> bool {
        self.root.join(relative).exists()
    }

    /// Check if a path is a file relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    pub fn is_file(&self, relative: impl AsRef<Path>) -> bool {
        self.root.join(relative).is_file()
    }

    /// Check if a path is a directory relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    pub fn is_dir(&self, relative: impl AsRef<Path>) -> bool {
        self.root.join(relative).is_dir()
    }

    /// Remove a file relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be removed.
    pub fn remove(&self, relative: impl AsRef<Path>) -> io::Result<()> {
        fs::remove_file(self.root.join(relative))
    }

    /// Remove a file if it exists, silently succeeding if it doesn't.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    ///
    /// # Errors
    ///
    /// Returns an error only if the file exists but cannot be removed.
    pub fn remove_if_exists(&self, relative: impl AsRef<Path>) -> io::Result<()> {
        let path = self.root.join(relative);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Create a directory and all parent directories relative to the repository root.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn create_dir_all(&self, relative: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir_all(self.root.join(relative))
    }

    /// Resolve a relative path to an absolute path.
    ///
    /// This is used when generating prompts for agents, where absolute paths
    /// are needed so agents can reference files correctly.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    pub fn absolute(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    /// Resolve a relative path to an absolute path as a string.
    ///
    /// Convenience method for use in prompt generation.
    ///
    /// # Arguments
    ///
    /// * `relative` - Path relative to the repository root
    pub fn absolute_str(&self, relative: impl AsRef<Path>) -> String {
        self.root.join(relative).display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_workspace_fs_root() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());
        assert_eq!(ws.root(), dir.path());
    }

    #[test]
    fn test_workspace_fs_agent_paths() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        assert_eq!(ws.agent_dir(), dir.path().join(".agent"));
        assert_eq!(ws.agent_logs(), dir.path().join(".agent/logs"));
        assert_eq!(ws.agent_tmp(), dir.path().join(".agent/tmp"));
        assert_eq!(ws.plan_md(), dir.path().join(".agent/PLAN.md"));
        assert_eq!(ws.issues_md(), dir.path().join(".agent/ISSUES.md"));
        assert_eq!(
            ws.commit_message(),
            dir.path().join(".agent/commit-message.txt")
        );
        assert_eq!(ws.checkpoint(), dir.path().join(".agent/checkpoint.json"));
        assert_eq!(ws.start_commit(), dir.path().join(".agent/start_commit"));
        assert_eq!(ws.prompt_md(), dir.path().join("PROMPT.md"));
    }

    #[test]
    fn test_workspace_fs_dynamic_paths() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        assert_eq!(ws.xsd_path("plan"), dir.path().join(".agent/tmp/plan.xsd"));
        assert_eq!(
            ws.xml_path("issues"),
            dir.path().join(".agent/tmp/issues.xml")
        );
        assert_eq!(
            ws.log_path("agent.log"),
            dir.path().join(".agent/logs/agent.log")
        );
    }

    #[test]
    fn test_workspace_fs_read_write() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        // Write creates parent directories
        ws.write(".agent/test/nested/file.txt", "hello world")
            .unwrap();
        assert!(ws.exists(".agent/test/nested/file.txt"));

        // Read returns content
        let content = ws.read(".agent/test/nested/file.txt").unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_workspace_fs_exists() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        assert!(!ws.exists("nonexistent.txt"));

        ws.write("test.txt", "content").unwrap();
        assert!(ws.exists("test.txt"));
        assert!(ws.is_file("test.txt"));
        assert!(!ws.is_dir("test.txt"));

        ws.create_dir_all("subdir").unwrap();
        assert!(ws.exists("subdir"));
        assert!(ws.is_dir("subdir"));
        assert!(!ws.is_file("subdir"));
    }

    #[test]
    fn test_workspace_fs_remove() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        ws.write("to_delete.txt", "content").unwrap();
        assert!(ws.exists("to_delete.txt"));

        ws.remove("to_delete.txt").unwrap();
        assert!(!ws.exists("to_delete.txt"));
    }

    #[test]
    fn test_workspace_fs_remove_if_exists() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        // Should succeed even if file doesn't exist
        ws.remove_if_exists("nonexistent.txt").unwrap();

        // Should remove existing file
        ws.write("to_delete.txt", "content").unwrap();
        ws.remove_if_exists("to_delete.txt").unwrap();
        assert!(!ws.exists("to_delete.txt"));
    }

    #[test]
    fn test_workspace_fs_absolute() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        let abs = ws.absolute(".agent/tmp/plan.xml");
        assert_eq!(abs, dir.path().join(".agent/tmp/plan.xml"));

        let abs_str = ws.absolute_str(".agent/tmp/plan.xml");
        assert_eq!(
            abs_str,
            dir.path().join(".agent/tmp/plan.xml").display().to_string()
        );
    }

    #[test]
    fn test_workspace_fs_read_bytes_write_bytes() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        let data = vec![0u8, 1, 2, 3, 255];
        ws.write_bytes("binary.bin", &data).unwrap();

        let read_data = ws.read_bytes("binary.bin").unwrap();
        assert_eq!(read_data, data);
    }
}
