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
//! - [`MemoryWorkspace`] - Test implementation with in-memory storage (available with `test-utils` feature)
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
//! The `test-utils` feature enables [`MemoryWorkspace`] for integration tests:
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

/// The `.agent/logs` directory for agent logs.
pub const AGENT_LOGS: &str = ".agent/logs";

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

/// Path to the pipeline log file.
pub const PIPELINE_LOG: &str = ".agent/logs/pipeline.log";

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
    fn agent_logs(&self) -> PathBuf {
        self.root().join(AGENT_LOGS)
    }

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

    /// Path to `.agent/logs/pipeline.log`.
    fn pipeline_log(&self) -> PathBuf {
        self.root().join(PIPELINE_LOG)
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

/// Production workspace implementation using the real filesystem.
///
/// All file operations are performed relative to the repository root using `std::fs`.
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
}

impl Workspace for WorkspaceFs {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        fs::read_to_string(self.root.join(relative))
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        fs::read(self.root.join(relative))
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(content)?;
        file.flush()
    }

    fn exists(&self, relative: &Path) -> bool {
        self.root.join(relative).exists()
    }

    fn is_file(&self, relative: &Path) -> bool {
        self.root.join(relative).is_file()
    }

    fn is_dir(&self, relative: &Path) -> bool {
        self.root.join(relative).is_dir()
    }

    fn remove(&self, relative: &Path) -> io::Result<()> {
        fs::remove_file(self.root.join(relative))
    }

    fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
        let path = self.root.join(relative);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
        fs::remove_dir_all(self.root.join(relative))
    }

    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
        let path = self.root.join(relative);
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        fs::create_dir_all(self.root.join(relative))
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>> {
        let abs_path = self.root.join(relative);
        let mut entries = Vec::new();
        for entry in fs::read_dir(abs_path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            // Store relative path from workspace root
            let rel_path = relative.join(entry.file_name());
            let modified = metadata.modified().ok();
            if let Some(mod_time) = modified {
                entries.push(DirEntry::with_modified(
                    rel_path,
                    metadata.is_file(),
                    metadata.is_dir(),
                    mod_time,
                ));
            } else {
                entries.push(DirEntry::new(
                    rel_path,
                    metadata.is_file(),
                    metadata.is_dir(),
                ));
            }
        }
        Ok(entries)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(self.root.join(from), self.root.join(to))
    }

    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let path = self.root.join(relative);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a NamedTempFile in the same directory as the target file.
        // This ensures atomic rename works (same filesystem).
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temp_file = NamedTempFile::new_in(parent_dir)?;

        // Set restrictive permissions on temp file (0600 = owner read/write only)
        // This prevents other users from reading the temp file before rename
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(temp_file.path())?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(temp_file.path(), perms)?;
        }

        // Write content to the temp file
        temp_file.write_all(content.as_bytes())?;
        temp_file.flush()?;
        temp_file.as_file().sync_all()?;

        // Persist the temp file to the target location (atomic rename)
        temp_file.persist(&path).map_err(|e| e.error)?;

        Ok(())
    }

    fn set_readonly(&self, relative: &Path) -> io::Result<()> {
        let path = self.root.join(relative);
        if !path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&path)?;
        let mut perms = metadata.permissions();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o444);
        }

        #[cfg(windows)]
        {
            perms.set_readonly(true);
        }

        fs::set_permissions(path, perms)
    }

    fn set_writable(&self, relative: &Path) -> io::Result<()> {
        let path = self.root.join(relative);
        if !path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&path)?;
        let mut perms = metadata.permissions();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o644);
        }

        #[cfg(windows)]
        {
            perms.set_readonly(false);
        }

        fs::set_permissions(path, perms)
    }
}

// ============================================================================
// Test Implementation: MemoryWorkspace
// ============================================================================

/// In-memory file entry with content and metadata.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
struct MemoryFile {
    content: Vec<u8>,
    modified: std::time::SystemTime,
}

#[cfg(any(test, feature = "test-utils"))]
impl MemoryFile {
    fn new(content: Vec<u8>) -> Self {
        Self {
            content,
            modified: std::time::SystemTime::now(),
        }
    }

    fn with_modified(content: Vec<u8>, modified: std::time::SystemTime) -> Self {
        Self { content, modified }
    }
}

/// In-memory workspace implementation for testing.
///
/// All file operations are performed against an in-memory HashMap, allowing tests to:
/// - Verify what was written without touching real files
/// - Control what reads return
/// - Run in parallel without filesystem conflicts
/// - Be deterministic and fast
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct MemoryWorkspace {
    root: PathBuf,
    files: std::sync::RwLock<std::collections::HashMap<PathBuf, MemoryFile>>,
    directories: std::sync::RwLock<std::collections::HashSet<PathBuf>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl MemoryWorkspace {
    /// Create a new in-memory workspace with the given virtual root path.
    ///
    /// The root path is used for path resolution but no real filesystem access occurs.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: std::sync::RwLock::new(std::collections::HashMap::new()),
            directories: std::sync::RwLock::new(std::collections::HashSet::new()),
        }
    }

    /// Create a new in-memory workspace with a default test root path.
    pub fn new_test() -> Self {
        Self::new(PathBuf::from("/test/repo"))
    }

    /// Ensure all parent directories exist for the given path.
    ///
    /// This is a helper to reduce duplication in file/directory creation methods.
    fn ensure_parent_dirs(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if parent.as_os_str().is_empty() {
                return;
            }
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
    }

    /// Ensure all components of the path exist as directories.
    ///
    /// Used for creating directories themselves (not just parents).
    fn ensure_dir_path(&self, path: &Path) {
        let mut dirs = self.directories.write().unwrap();
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            dirs.insert(current.clone());
        }
    }

    /// Pre-populate a file with content for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file(self, path: &str, content: &str) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files
            .write()
            .unwrap()
            .insert(path_buf, MemoryFile::new(content.as_bytes().to_vec()));
        self
    }

    /// Pre-populate a file with content and explicit modification time for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file_at_time(
        self,
        path: &str,
        content: &str,
        modified: std::time::SystemTime,
    ) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files.write().unwrap().insert(
            path_buf,
            MemoryFile::with_modified(content.as_bytes().to_vec(), modified),
        );
        self
    }

    /// Pre-populate a file with bytes for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file_bytes(self, path: &str, content: &[u8]) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files
            .write()
            .unwrap()
            .insert(path_buf, MemoryFile::new(content.to_vec()));
        self
    }

    /// Pre-populate a directory for testing.
    pub fn with_dir(self, path: &str) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_dir_path(&path_buf);
        self
    }

    /// List all files in a directory (for test assertions).
    ///
    /// Returns file paths relative to the workspace root.
    pub fn list_files_in_dir(&self, dir: &str) -> Vec<PathBuf> {
        let dir_path = PathBuf::from(dir);
        self.files
            .read()
            .unwrap()
            .keys()
            .filter(|path| {
                path.parent()
                    .map(|p| p == dir_path || p.starts_with(&dir_path))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Get the modification time of a file (for test assertions).
    pub fn get_modified(&self, path: &str) -> Option<std::time::SystemTime> {
        self.files
            .read()
            .unwrap()
            .get(&PathBuf::from(path))
            .map(|f| f.modified)
    }

    /// List all directories (for test assertions).
    pub fn list_directories(&self) -> Vec<PathBuf> {
        self.directories.read().unwrap().iter().cloned().collect()
    }

    /// Get all files that were written (for test assertions).
    pub fn written_files(&self) -> std::collections::HashMap<PathBuf, Vec<u8>> {
        self.files
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.content.clone()))
            .collect()
    }

    /// Get a specific file's content (for test assertions).
    pub fn get_file(&self, path: &str) -> Option<String> {
        self.files
            .read()
            .unwrap()
            .get(&PathBuf::from(path))
            .map(|f| String::from_utf8_lossy(&f.content).to_string())
    }

    /// Get a specific file's bytes (for test assertions).
    pub fn get_file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .read()
            .unwrap()
            .get(&PathBuf::from(path))
            .map(|f| f.content.clone())
    }

    /// Check if a file was written (for test assertions).
    pub fn was_written(&self, path: &str) -> bool {
        self.files
            .read()
            .unwrap()
            .contains_key(&PathBuf::from(path))
    }

    /// Clear all files (for test setup).
    pub fn clear(&self) {
        self.files.write().unwrap().clear();
        self.directories.write().unwrap().clear();
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Workspace for MemoryWorkspace {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        self.files
            .read()
            .unwrap()
            .get(relative)
            .map(|f| String::from_utf8_lossy(&f.content).to_string())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        self.files
            .read()
            .unwrap()
            .get(relative)
            .map(|f| f.content.clone())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        self.ensure_parent_dirs(relative);
        self.files.write().unwrap().insert(
            relative.to_path_buf(),
            MemoryFile::new(content.as_bytes().to_vec()),
        );
        Ok(())
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.ensure_parent_dirs(relative);
        self.files
            .write()
            .unwrap()
            .insert(relative.to_path_buf(), MemoryFile::new(content.to_vec()));
        Ok(())
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.ensure_parent_dirs(relative);
        let mut files = self.files.write().unwrap();
        let entry = files
            .entry(relative.to_path_buf())
            .or_insert_with(|| MemoryFile::new(Vec::new()));
        entry.content.extend_from_slice(content);
        entry.modified = std::time::SystemTime::now();
        Ok(())
    }

    fn exists(&self, relative: &Path) -> bool {
        self.files.read().unwrap().contains_key(relative)
            || self.directories.read().unwrap().contains(relative)
    }

    fn is_file(&self, relative: &Path) -> bool {
        self.files.read().unwrap().contains_key(relative)
    }

    fn is_dir(&self, relative: &Path) -> bool {
        self.directories.read().unwrap().contains(relative)
    }

    fn remove(&self, relative: &Path) -> io::Result<()> {
        self.files
            .write()
            .unwrap()
            .remove(relative)
            .map(|_| ())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.files.write().unwrap().remove(relative);
        Ok(())
    }

    fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
        // Check if directory exists first
        if !self.directories.read().unwrap().contains(relative) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Directory not found: {}", relative.display()),
            ));
        }
        self.remove_dir_all_if_exists(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
        // Remove all files under this directory
        {
            let mut files = self.files.write().unwrap();
            let to_remove: Vec<PathBuf> = files
                .keys()
                .filter(|path| path.starts_with(relative))
                .cloned()
                .collect();
            for path in to_remove {
                files.remove(&path);
            }
        }
        // Remove all directories under this directory (including itself)
        {
            let mut dirs = self.directories.write().unwrap();
            let to_remove: Vec<PathBuf> = dirs
                .iter()
                .filter(|path| path.starts_with(relative) || *path == relative)
                .cloned()
                .collect();
            for path in to_remove {
                dirs.remove(&path);
            }
        }
        Ok(())
    }

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.ensure_dir_path(relative);
        Ok(())
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>> {
        let files = self.files.read().unwrap();
        let dirs = self.directories.read().unwrap();

        // Check if the directory exists
        if !relative.as_os_str().is_empty() && !dirs.contains(relative) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Directory not found: {}", relative.display()),
            ));
        }

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Find all files that are direct children of this directory
        for (path, mem_file) in files.iter() {
            if let Some(parent) = path.parent() {
                if parent == relative {
                    if let Some(name) = path.file_name() {
                        if seen.insert(name.to_os_string()) {
                            entries.push(DirEntry::with_modified(
                                path.clone(),
                                true,
                                false,
                                mem_file.modified,
                            ));
                        }
                    }
                }
            }
        }

        // Find all directories that are direct children of this directory
        for dir_path in dirs.iter() {
            if let Some(parent) = dir_path.parent() {
                if parent == relative {
                    if let Some(name) = dir_path.file_name() {
                        if seen.insert(name.to_os_string()) {
                            entries.push(DirEntry::new(dir_path.clone(), false, true));
                        }
                    }
                }
            }
        }

        Ok(entries)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        // Create parent directories for destination first (before taking files lock)
        self.ensure_parent_dirs(to);
        let mut files = self.files.write().unwrap();
        if let Some(file) = files.remove(from) {
            files.insert(to.to_path_buf(), file);
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("File not found: {}", from.display()),
            ))
        }
    }

    fn set_readonly(&self, _relative: &Path) -> io::Result<()> {
        // No-op for in-memory workspace - permissions aren't relevant for testing
        Ok(())
    }

    fn set_writable(&self, _relative: &Path) -> io::Result<()> {
        // No-op for in-memory workspace - permissions aren't relevant for testing
        Ok(())
    }

    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
        // In-memory operations are inherently atomic - no partial state possible.
        // Just delegate to regular write().
        self.write(relative, content)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Clone for MemoryWorkspace {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            files: std::sync::RwLock::new(self.files.read().unwrap().clone()),
            directories: std::sync::RwLock::new(self.directories.read().unwrap().clone()),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // WorkspaceFs path resolution tests (no filesystem access needed)
    // =========================================================================

    #[test]
    fn test_workspace_fs_root() {
        let ws = WorkspaceFs::new(PathBuf::from("/test/repo"));
        assert_eq!(ws.root(), Path::new("/test/repo"));
    }

    #[test]
    fn test_workspace_fs_agent_paths() {
        let ws = WorkspaceFs::new(PathBuf::from("/test/repo"));

        assert_eq!(ws.agent_dir(), PathBuf::from("/test/repo/.agent"));
        assert_eq!(ws.agent_logs(), PathBuf::from("/test/repo/.agent/logs"));
        assert_eq!(ws.agent_tmp(), PathBuf::from("/test/repo/.agent/tmp"));
        assert_eq!(ws.plan_md(), PathBuf::from("/test/repo/.agent/PLAN.md"));
        assert_eq!(ws.issues_md(), PathBuf::from("/test/repo/.agent/ISSUES.md"));
        assert_eq!(
            ws.commit_message(),
            PathBuf::from("/test/repo/.agent/commit-message.txt")
        );
        assert_eq!(
            ws.checkpoint(),
            PathBuf::from("/test/repo/.agent/checkpoint.json")
        );
        assert_eq!(
            ws.start_commit(),
            PathBuf::from("/test/repo/.agent/start_commit")
        );
        assert_eq!(ws.prompt_md(), PathBuf::from("/test/repo/PROMPT.md"));
    }

    #[test]
    fn test_workspace_fs_dynamic_paths() {
        let ws = WorkspaceFs::new(PathBuf::from("/test/repo"));

        assert_eq!(
            ws.xsd_path("plan"),
            PathBuf::from("/test/repo/.agent/tmp/plan.xsd")
        );
        assert_eq!(
            ws.xml_path("issues"),
            PathBuf::from("/test/repo/.agent/tmp/issues.xml")
        );
        assert_eq!(
            ws.log_path("agent.log"),
            PathBuf::from("/test/repo/.agent/logs/agent.log")
        );
    }

    #[test]
    fn test_workspace_fs_absolute() {
        let ws = WorkspaceFs::new(PathBuf::from("/test/repo"));

        let abs = ws.absolute(Path::new(".agent/tmp/plan.xml"));
        assert_eq!(abs, PathBuf::from("/test/repo/.agent/tmp/plan.xml"));

        let abs_str = ws.absolute_str(".agent/tmp/plan.xml");
        assert_eq!(abs_str, "/test/repo/.agent/tmp/plan.xml");
    }

    // =========================================================================
    // MemoryWorkspace tests
    // =========================================================================

    #[test]
    fn test_memory_workspace_read_write() {
        let ws = MemoryWorkspace::new_test();

        ws.write(Path::new(".agent/test.txt"), "hello").unwrap();
        assert_eq!(ws.read(Path::new(".agent/test.txt")).unwrap(), "hello");
        assert!(ws.was_written(".agent/test.txt"));
    }

    #[test]
    fn test_memory_workspace_with_file() {
        let ws = MemoryWorkspace::new_test().with_file("existing.txt", "pre-existing content");

        assert_eq!(
            ws.read(Path::new("existing.txt")).unwrap(),
            "pre-existing content"
        );
    }

    #[test]
    fn test_memory_workspace_exists() {
        let ws = MemoryWorkspace::new_test().with_file("exists.txt", "content");

        assert!(ws.exists(Path::new("exists.txt")));
        assert!(!ws.exists(Path::new("not_exists.txt")));
    }

    #[test]
    fn test_memory_workspace_remove() {
        let ws = MemoryWorkspace::new_test().with_file("to_delete.txt", "content");

        assert!(ws.exists(Path::new("to_delete.txt")));
        ws.remove(Path::new("to_delete.txt")).unwrap();
        assert!(!ws.exists(Path::new("to_delete.txt")));
    }

    #[test]
    fn test_memory_workspace_read_nonexistent_fails() {
        let ws = MemoryWorkspace::new_test();

        let result = ws.read(Path::new("nonexistent.txt"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_memory_workspace_written_files() {
        let ws = MemoryWorkspace::new_test();

        ws.write(Path::new("file1.txt"), "content1").unwrap();
        ws.write(Path::new("file2.txt"), "content2").unwrap();

        let files = ws.written_files();
        assert_eq!(files.len(), 2);
        assert_eq!(
            String::from_utf8_lossy(files.get(&PathBuf::from("file1.txt")).unwrap()),
            "content1"
        );
    }

    #[test]
    fn test_memory_workspace_get_file() {
        let ws = MemoryWorkspace::new_test();

        ws.write(Path::new("test.txt"), "test content").unwrap();
        assert_eq!(ws.get_file("test.txt"), Some("test content".to_string()));
        assert_eq!(ws.get_file("nonexistent.txt"), None);
    }

    #[test]
    fn test_memory_workspace_clear() {
        let ws = MemoryWorkspace::new_test().with_file("file.txt", "content");

        assert!(ws.exists(Path::new("file.txt")));
        ws.clear();
        assert!(!ws.exists(Path::new("file.txt")));
    }

    #[test]
    fn test_memory_workspace_absolute_str() {
        let ws = MemoryWorkspace::new_test();

        assert_eq!(
            ws.absolute_str(".agent/tmp/commit_message.xml"),
            "/test/repo/.agent/tmp/commit_message.xml"
        );
    }

    #[test]
    fn test_memory_workspace_creates_parent_dirs() {
        let ws = MemoryWorkspace::new_test();

        ws.write(Path::new("a/b/c/file.txt"), "content").unwrap();

        // Parent directories should be tracked
        assert!(ws.is_dir(Path::new("a")));
        assert!(ws.is_dir(Path::new("a/b")));
        assert!(ws.is_dir(Path::new("a/b/c")));
        assert!(ws.is_file(Path::new("a/b/c/file.txt")));
    }

    #[test]
    fn test_memory_workspace_rename() {
        let ws = MemoryWorkspace::new_test().with_file("old.txt", "content");

        ws.rename(Path::new("old.txt"), Path::new("new.txt"))
            .unwrap();

        assert!(!ws.exists(Path::new("old.txt")));
        assert!(ws.exists(Path::new("new.txt")));
        assert_eq!(ws.read(Path::new("new.txt")).unwrap(), "content");
    }

    #[test]
    fn test_memory_workspace_rename_creates_parent_dirs() {
        let ws = MemoryWorkspace::new_test().with_file("old.txt", "content");

        ws.rename(Path::new("old.txt"), Path::new("a/b/new.txt"))
            .unwrap();

        assert!(!ws.exists(Path::new("old.txt")));
        assert!(ws.is_dir(Path::new("a")));
        assert!(ws.is_dir(Path::new("a/b")));
        assert!(ws.exists(Path::new("a/b/new.txt")));
    }

    #[test]
    fn test_memory_workspace_rename_nonexistent_fails() {
        let ws = MemoryWorkspace::new_test();

        let result = ws.rename(Path::new("nonexistent.txt"), Path::new("new.txt"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_memory_workspace_set_readonly_noop() {
        // In-memory workspace doesn't track permissions, but should succeed
        let ws = MemoryWorkspace::new_test().with_file("test.txt", "content");

        // Should succeed (no-op)
        ws.set_readonly(Path::new("test.txt")).unwrap();
        ws.set_writable(Path::new("test.txt")).unwrap();

        // File should still be readable
        assert_eq!(ws.read(Path::new("test.txt")).unwrap(), "content");
    }

    #[test]
    fn test_memory_workspace_write_atomic() {
        let ws = MemoryWorkspace::new_test();

        ws.write_atomic(Path::new("atomic.txt"), "atomic content")
            .unwrap();

        assert_eq!(ws.read(Path::new("atomic.txt")).unwrap(), "atomic content");
    }

    #[test]
    fn test_memory_workspace_write_atomic_creates_parent_dirs() {
        let ws = MemoryWorkspace::new_test();

        ws.write_atomic(Path::new("a/b/c/atomic.txt"), "nested atomic")
            .unwrap();

        assert!(ws.is_dir(Path::new("a")));
        assert!(ws.is_dir(Path::new("a/b")));
        assert!(ws.is_dir(Path::new("a/b/c")));
        assert_eq!(
            ws.read(Path::new("a/b/c/atomic.txt")).unwrap(),
            "nested atomic"
        );
    }

    #[test]
    fn test_memory_workspace_write_atomic_overwrites() {
        let ws = MemoryWorkspace::new_test().with_file("existing.txt", "old content");

        ws.write_atomic(Path::new("existing.txt"), "new content")
            .unwrap();

        assert_eq!(ws.read(Path::new("existing.txt")).unwrap(), "new content");
    }
}
