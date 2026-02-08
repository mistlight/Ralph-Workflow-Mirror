//! Workspace filesystem abstraction for explicit path resolution.
//!
//! This module provides the [`Workspace`] trait and implementations that eliminate
//! CWD dependencies by making all path operations explicit relative to the repository root.
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
//! The [`Workspace`] trait defines the interface for file operations, with two implementations:
//!
//! - [`WorkspaceFs`] - Production implementation using the real filesystem
//! - `MemoryWorkspace` - Test implementation with in-memory storage (available with `test-utils` feature)
//!
//! # Well-Known Paths
//!
//! This module defines constants for all Ralph artifact paths:
//!
//! - [`AGENT_DIR`] - `.agent/` directory
//! - [`PLAN_MD`] - `.agent/PLAN.md`
//! - [`ISSUES_MD`] - `.agent/ISSUES.md`
//! - [`PROMPT_MD`] - `PROMPT.md` (repository root)
//! - [`CHECKPOINT_JSON`] - `.agent/checkpoint.json`
//!
//! The [`Workspace`] trait provides convenience methods for these paths (e.g., [`Workspace::plan_md`]).
//!
//! # Production Example
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
//! ws.write(Path::new(".agent/test.txt"), "content")?;
//! let content = ws.read(Path::new(".agent/test.txt"))?;
//! ```
//!
//! # Testing with MemoryWorkspace
//!
//! The `test-utils` feature enables `MemoryWorkspace` for integration tests:
//!
//! ```ignore
//! use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
//! use std::path::Path;
//!
//! // Create a test workspace with pre-populated files
//! let ws = MemoryWorkspace::new_test()
//!     .with_file("PROMPT.md", "# Task: Add logging")
//!     .with_file(".agent/PLAN.md", "1. Add log statements");
//!
//! // Verify file operations
//! assert!(ws.exists(Path::new("PROMPT.md")));
//! assert_eq!(ws.read(Path::new("PROMPT.md"))?, "# Task: Add logging");
//!
//! // Write and verify
//! ws.write(Path::new(".agent/output.txt"), "result")?;
//! assert!(ws.was_written(".agent/output.txt"));
//! ```
//!
//! # See Also
//!
//! - [`crate::executor::ProcessExecutor`] - Similar abstraction for process execution

// ============================================================================
// Well-known path constants
// ============================================================================

/// The `.agent` directory where Ralph stores all artifacts.
pub const AGENT_DIR: &str = ".agent";

/// The `.agent/tmp` directory for temporary files.
pub const AGENT_TMP: &str = ".agent/tmp";

// AGENT_LOGS constant removed - use RunLogContext for per-run log directories.

/// Path to the implementation plan file.
pub const PLAN_MD: &str = ".agent/PLAN.md";

/// Path to the issues file from code review.
pub const ISSUES_MD: &str = ".agent/ISSUES.md";

/// Path to the status file.
pub const STATUS_MD: &str = ".agent/STATUS.md";

/// Path to the notes file.
pub const NOTES_MD: &str = ".agent/NOTES.md";

/// Path to the commit message file.
pub const COMMIT_MESSAGE_TXT: &str = ".agent/commit-message.txt";

/// Path to the checkpoint file for resume support.
pub const CHECKPOINT_JSON: &str = ".agent/checkpoint.json";

/// Path to the start commit tracking file.
pub const START_COMMIT: &str = ".agent/start_commit";

/// Path to the review baseline tracking file.
pub const REVIEW_BASELINE_TXT: &str = ".agent/review_baseline.txt";

/// Path to the prompt file in repository root.
pub const PROMPT_MD: &str = "PROMPT.md";

/// Path to the prompt backup file.
pub const PROMPT_BACKUP: &str = ".agent/PROMPT.md.backup";

/// Path to the agent config file.
pub const AGENT_CONFIG_TOML: &str = ".agent/config.toml";

/// Path to the agents registry file.
pub const AGENTS_TOML: &str = ".agent/agents.toml";

// PIPELINE_LOG constant removed - use RunLogContext::pipeline_log() for per-run log paths.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// ============================================================================
// DirEntry - abstraction for directory entries
// ============================================================================

/// A directory entry returned by `Workspace::read_dir`.
///
/// This abstracts `std::fs::DirEntry` to allow in-memory implementations.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The path of this entry (relative to workspace root).
    path: PathBuf,
    /// Whether this entry is a file.
    is_file: bool,
    /// Whether this entry is a directory.
    is_dir: bool,
    /// Optional modification time (for sorting by recency).
    modified: Option<std::time::SystemTime>,
}

impl DirEntry {
    /// Create a new directory entry.
    pub fn new(path: PathBuf, is_file: bool, is_dir: bool) -> Self {
        Self {
            path,
            is_file,
            is_dir,
            modified: None,
        }
    }

    /// Create a new directory entry with modification time.
    pub fn with_modified(
        path: PathBuf,
        is_file: bool,
        is_dir: bool,
        modified: std::time::SystemTime,
    ) -> Self {
        Self {
            path,
            is_file,
            is_dir,
            modified: Some(modified),
        }
    }

    /// Get the path of this entry.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if this entry is a file.
    pub fn is_file(&self) -> bool {
        self.is_file
    }

    /// Check if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Get the file name of this entry.
    pub fn file_name(&self) -> Option<&std::ffi::OsStr> {
        self.path.file_name()
    }

    /// Get the modification time of this entry, if available.
    pub fn modified(&self) -> Option<std::time::SystemTime> {
        self.modified
    }
}

// ============================================================================
// Workspace Trait
// ============================================================================

/// Trait defining the workspace filesystem interface.
///
/// This trait abstracts file operations relative to a repository root, allowing
/// for both real filesystem access (production) and in-memory storage (testing).
pub trait Workspace: Send + Sync {
    /// Get the repository root path.
    fn root(&self) -> &Path;

    // =========================================================================
    // File operations
    // =========================================================================

    /// Read a file relative to the repository root.
    fn read(&self, relative: &Path) -> io::Result<String>;

    /// Read a file as bytes relative to the repository root.
    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>>;

    /// Write content to a file relative to the repository root.
    /// Creates parent directories if they don't exist.
    fn write(&self, relative: &Path, content: &str) -> io::Result<()>;

    /// Write bytes to a file relative to the repository root.
    /// Creates parent directories if they don't exist.
    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()>;

    /// Append bytes to a file relative to the repository root.
    /// Creates the file if it doesn't exist. Creates parent directories if needed.
    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()>;

    /// Check if a path exists relative to the repository root.
    fn exists(&self, relative: &Path) -> bool;

    /// Check if a path is a file relative to the repository root.
    fn is_file(&self, relative: &Path) -> bool;

    /// Check if a path is a directory relative to the repository root.
    fn is_dir(&self, relative: &Path) -> bool;

    /// Remove a file relative to the repository root.
    fn remove(&self, relative: &Path) -> io::Result<()>;

    /// Remove a file if it exists, silently succeeding if it doesn't.
    fn remove_if_exists(&self, relative: &Path) -> io::Result<()>;

    /// Remove a directory and all its contents relative to the repository root.
    ///
    /// Similar to `std::fs::remove_dir_all`, this removes a directory and everything inside it.
    /// Returns an error if the directory doesn't exist.
    fn remove_dir_all(&self, relative: &Path) -> io::Result<()>;

    /// Remove a directory and all its contents if it exists, silently succeeding if it doesn't.
    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()>;

    /// Create a directory and all parent directories relative to the repository root.
    fn create_dir_all(&self, relative: &Path) -> io::Result<()>;

    /// List entries in a directory relative to the repository root.
    ///
    /// Returns a vector of `DirEntry`-like information for each entry.
    /// For production, this wraps `std::fs::read_dir`.
    /// For testing, this returns entries from the in-memory filesystem.
    fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>>;

    /// Rename/move a file from one path to another relative to the repository root.
    ///
    /// This is used for backup rotation where files are moved to new names.
    /// Returns an error if the source file doesn't exist.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Write content to a file atomically using temp file + rename pattern.
    ///
    /// This ensures the file is either fully written or not written at all,
    /// preventing partial writes or corruption from crashes/interruptions.
    ///
    /// # Implementation details
    ///
    /// - `WorkspaceFs`: Uses `tempfile::NamedTempFile` in the same directory,
    ///   writes content, syncs to disk, then atomically renames to target.
    ///   On Unix, temp file has mode 0600 for security.
    /// - `MemoryWorkspace`: Just calls `write()` since in-memory operations
    ///   are inherently atomic (no partial state possible).
    ///
    /// # When to use
    ///
    /// Use `write_atomic()` for critical files where corruption would be problematic:
    /// - XML outputs (issues.xml, plan.xml, commit_message.xml)
    /// - Agent artifacts (PLAN.md, commit-message.txt)
    /// - Any file that must not have partial content
    ///
    /// Use regular `write()` for:
    /// - Log files (append-only, partial is acceptable)
    /// - Temporary/debug files
    /// - Files where performance matters more than atomicity
    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()>;

    /// Set a file to read-only permissions.
    ///
    /// This is a best-effort operation for protecting files like PROMPT.md backups.
    /// On Unix, sets permissions to 0o444.
    /// On Windows, sets the readonly flag.
    /// In-memory implementations may no-op since permissions aren't relevant for testing.
    ///
    /// Returns Ok(()) on success or if the file doesn't exist (nothing to protect).
    /// Returns Err only if the file exists but permissions cannot be changed.
    fn set_readonly(&self, relative: &Path) -> io::Result<()>;

    /// Set a file to writable permissions.
    ///
    /// Reverses the effect of `set_readonly`.
    /// On Unix, sets permissions to 0o644.
    /// On Windows, clears the readonly flag.
    /// In-memory implementations may no-op since permissions aren't relevant for testing.
    ///
    /// Returns Ok(()) on success or if the file doesn't exist.
    fn set_writable(&self, relative: &Path) -> io::Result<()>;

    // =========================================================================
    // Path resolution (default implementations)
    // =========================================================================

    /// Resolve a relative path to an absolute path.
    fn absolute(&self, relative: &Path) -> PathBuf {
        self.root().join(relative)
    }

    /// Resolve a relative path to an absolute path as a string.
    fn absolute_str(&self, relative: &str) -> String {
        self.root().join(relative).display().to_string()
    }

    // =========================================================================
    // Well-known paths (default implementations)
    // =========================================================================

    /// Path to the `.agent` directory.
    fn agent_dir(&self) -> PathBuf {
        self.root().join(AGENT_DIR)
    }

    /// Path to the `.agent/logs` directory.
    ///
    /// Path to the `.agent/tmp` directory.
    fn agent_tmp(&self) -> PathBuf {
        self.root().join(AGENT_TMP)
    }

    /// Path to `.agent/PLAN.md`.
    fn plan_md(&self) -> PathBuf {
        self.root().join(PLAN_MD)
    }

    /// Path to `.agent/ISSUES.md`.
    fn issues_md(&self) -> PathBuf {
        self.root().join(ISSUES_MD)
    }

    /// Path to `.agent/STATUS.md`.
    fn status_md(&self) -> PathBuf {
        self.root().join(STATUS_MD)
    }

    /// Path to `.agent/NOTES.md`.
    fn notes_md(&self) -> PathBuf {
        self.root().join(NOTES_MD)
    }

    /// Path to `.agent/commit-message.txt`.
    fn commit_message(&self) -> PathBuf {
        self.root().join(COMMIT_MESSAGE_TXT)
    }

    /// Path to `.agent/checkpoint.json`.
    fn checkpoint(&self) -> PathBuf {
        self.root().join(CHECKPOINT_JSON)
    }

    /// Path to `.agent/start_commit`.
    fn start_commit(&self) -> PathBuf {
        self.root().join(START_COMMIT)
    }

    /// Path to `.agent/review_baseline.txt`.
    fn review_baseline(&self) -> PathBuf {
        self.root().join(REVIEW_BASELINE_TXT)
    }

    /// Path to `PROMPT.md` in the repository root.
    fn prompt_md(&self) -> PathBuf {
        self.root().join(PROMPT_MD)
    }

    /// Path to `.agent/PROMPT.md.backup`.
    fn prompt_backup(&self) -> PathBuf {
        self.root().join(PROMPT_BACKUP)
    }

    /// Path to `.agent/config.toml`.
    fn agent_config(&self) -> PathBuf {
        self.root().join(AGENT_CONFIG_TOML)
    }

    /// Path to `.agent/agents.toml`.
    fn agents_toml(&self) -> PathBuf {
        self.root().join(AGENTS_TOML)
    }

    /// Path to an XSD schema file in `.agent/tmp/`.
    fn xsd_path(&self, name: &str) -> PathBuf {
        self.root().join(format!(".agent/tmp/{}.xsd", name))
    }

    /// Path to an XML file in `.agent/tmp/`.
    fn xml_path(&self, name: &str) -> PathBuf {
        self.root().join(format!(".agent/tmp/{}.xml", name))
    }

    /// Path to a log file in `.agent/logs/`.
    fn log_path(&self, name: &str) -> PathBuf {
        self.root().join(format!(".agent/logs/{}", name))
    }
}

// ============================================================================
// Production Implementation: WorkspaceFs
// ============================================================================

include!("workspace/workspace_fs.rs");

// ============================================================================
// Test Implementation: MemoryWorkspace
// ============================================================================

#[cfg(any(test, feature = "test-utils"))]
pub mod memory_workspace;

#[cfg(any(test, feature = "test-utils"))]
pub use memory_workspace::MemoryWorkspace;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    include!("workspace/tests.rs");
}
