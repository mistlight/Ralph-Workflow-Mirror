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
}

impl DirEntry {
    /// Create a new directory entry.
    pub fn new(path: PathBuf, is_file: bool, is_dir: bool) -> Self {
        Self {
            path,
            is_file,
            is_dir,
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

    /// Create a directory and all parent directories relative to the repository root.
    fn create_dir_all(&self, relative: &Path) -> io::Result<()>;

    /// List entries in a directory relative to the repository root.
    ///
    /// Returns a vector of `DirEntry`-like information for each entry.
    /// For production, this wraps `std::fs::read_dir`.
    /// For testing, this returns entries from the in-memory filesystem.
    fn read_dir(&self, relative: &Path) -> io::Result<Vec<DirEntry>>;

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
        self.root().join(".agent")
    }

    /// Path to the `.agent/logs` directory.
    fn agent_logs(&self) -> PathBuf {
        self.root().join(".agent/logs")
    }

    /// Path to the `.agent/tmp` directory.
    fn agent_tmp(&self) -> PathBuf {
        self.root().join(".agent/tmp")
    }

    /// Path to `.agent/PLAN.md`.
    fn plan_md(&self) -> PathBuf {
        self.root().join(".agent/PLAN.md")
    }

    /// Path to `.agent/ISSUES.md`.
    fn issues_md(&self) -> PathBuf {
        self.root().join(".agent/ISSUES.md")
    }

    /// Path to `.agent/STATUS.md`.
    fn status_md(&self) -> PathBuf {
        self.root().join(".agent/STATUS.md")
    }

    /// Path to `.agent/NOTES.md`.
    fn notes_md(&self) -> PathBuf {
        self.root().join(".agent/NOTES.md")
    }

    /// Path to `.agent/commit-message.txt`.
    fn commit_message(&self) -> PathBuf {
        self.root().join(".agent/commit-message.txt")
    }

    /// Path to `.agent/checkpoint.json`.
    fn checkpoint(&self) -> PathBuf {
        self.root().join(".agent/checkpoint.json")
    }

    /// Path to `.agent/start_commit`.
    fn start_commit(&self) -> PathBuf {
        self.root().join(".agent/start_commit")
    }

    /// Path to `.agent/review_baseline.txt`.
    fn review_baseline(&self) -> PathBuf {
        self.root().join(".agent/review_baseline.txt")
    }

    /// Path to `PROMPT.md` in the repository root.
    fn prompt_md(&self) -> PathBuf {
        self.root().join("PROMPT.md")
    }

    /// Path to `.agent/PROMPT.md.backup`.
    fn prompt_backup(&self) -> PathBuf {
        self.root().join(".agent/PROMPT.md.backup")
    }

    /// Path to `.agent/config.toml`.
    fn agent_config(&self) -> PathBuf {
        self.root().join(".agent/config.toml")
    }

    /// Path to `.agent/agents.toml`.
    fn agents_toml(&self) -> PathBuf {
        self.root().join(".agent/agents.toml")
    }

    /// Path to `.agent/logs/pipeline.log`.
    fn pipeline_log(&self) -> PathBuf {
        self.root().join(".agent/logs/pipeline.log")
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
            entries.push(DirEntry::new(
                rel_path,
                metadata.is_file(),
                metadata.is_dir(),
            ));
        }
        Ok(entries)
    }
}

// ============================================================================
// Test Implementation: MemoryWorkspace
// ============================================================================

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
    files: std::sync::RwLock<std::collections::HashMap<PathBuf, Vec<u8>>>,
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

    /// Pre-populate a file with content for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file(self, path: &str, content: &str) -> Self {
        let path_buf = PathBuf::from(path);
        // Create parent directories
        if let Some(parent) = path_buf.parent() {
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
        self.files
            .write()
            .unwrap()
            .insert(path_buf, content.as_bytes().to_vec());
        self
    }

    /// Pre-populate a file with bytes for testing.
    ///
    /// Also creates parent directories automatically.
    pub fn with_file_bytes(self, path: &str, content: &[u8]) -> Self {
        let path_buf = PathBuf::from(path);
        // Create parent directories
        if let Some(parent) = path_buf.parent() {
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
        self.files
            .write()
            .unwrap()
            .insert(path_buf, content.to_vec());
        self
    }

    /// Pre-populate a directory for testing.
    pub fn with_dir(self, path: &str) -> Self {
        let path_buf = PathBuf::from(path);
        {
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in path_buf.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
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

    /// List all directories (for test assertions).
    pub fn list_directories(&self) -> Vec<PathBuf> {
        self.directories.read().unwrap().iter().cloned().collect()
    }

    /// Get all files that were written (for test assertions).
    pub fn written_files(&self) -> std::collections::HashMap<PathBuf, Vec<u8>> {
        self.files.read().unwrap().clone()
    }

    /// Get a specific file's content (for test assertions).
    pub fn get_file(&self, path: &str) -> Option<String> {
        self.files
            .read()
            .unwrap()
            .get(&PathBuf::from(path))
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
    }

    /// Get a specific file's bytes (for test assertions).
    pub fn get_file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .read()
            .unwrap()
            .get(&PathBuf::from(path))
            .cloned()
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
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
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
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("File not found: {}", relative.display()),
                )
            })
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        // Create parent directories
        if let Some(parent) = relative.parent() {
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
        self.files
            .write()
            .unwrap()
            .insert(relative.to_path_buf(), content.as_bytes().to_vec());
        Ok(())
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        // Create parent directories
        if let Some(parent) = relative.parent() {
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
        self.files
            .write()
            .unwrap()
            .insert(relative.to_path_buf(), content.to_vec());
        Ok(())
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        // Create parent directories
        if let Some(parent) = relative.parent() {
            let mut dirs = self.directories.write().unwrap();
            let mut current = PathBuf::new();
            for component in parent.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
        }
        let mut files = self.files.write().unwrap();
        let existing = files.entry(relative.to_path_buf()).or_default();
        existing.extend_from_slice(content);
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

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        let mut dirs = self.directories.write().unwrap();
        let mut current = PathBuf::new();
        for component in relative.components() {
            current.push(component);
            dirs.insert(current.clone());
        }
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
        for path in files.keys() {
            if let Some(parent) = path.parent() {
                if parent == relative {
                    if let Some(name) = path.file_name() {
                        if seen.insert(name.to_os_string()) {
                            entries.push(DirEntry::new(path.clone(), true, false));
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
    use tempfile::TempDir;

    // =========================================================================
    // WorkspaceFs (production) tests
    // =========================================================================

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
        ws.write(Path::new(".agent/test/nested/file.txt"), "hello world")
            .unwrap();
        assert!(ws.exists(Path::new(".agent/test/nested/file.txt")));

        // Read returns content
        let content = ws.read(Path::new(".agent/test/nested/file.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_workspace_fs_exists() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        assert!(!ws.exists(Path::new("nonexistent.txt")));

        ws.write(Path::new("test.txt"), "content").unwrap();
        assert!(ws.exists(Path::new("test.txt")));
        assert!(ws.is_file(Path::new("test.txt")));
        assert!(!ws.is_dir(Path::new("test.txt")));

        ws.create_dir_all(Path::new("subdir")).unwrap();
        assert!(ws.exists(Path::new("subdir")));
        assert!(ws.is_dir(Path::new("subdir")));
        assert!(!ws.is_file(Path::new("subdir")));
    }

    #[test]
    fn test_workspace_fs_remove() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        ws.write(Path::new("to_delete.txt"), "content").unwrap();
        assert!(ws.exists(Path::new("to_delete.txt")));

        ws.remove(Path::new("to_delete.txt")).unwrap();
        assert!(!ws.exists(Path::new("to_delete.txt")));
    }

    #[test]
    fn test_workspace_fs_remove_if_exists() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        // Should succeed even if file doesn't exist
        ws.remove_if_exists(Path::new("nonexistent.txt")).unwrap();

        // Should remove existing file
        ws.write(Path::new("to_delete.txt"), "content").unwrap();
        ws.remove_if_exists(Path::new("to_delete.txt")).unwrap();
        assert!(!ws.exists(Path::new("to_delete.txt")));
    }

    #[test]
    fn test_workspace_fs_absolute() {
        let dir = TempDir::new().unwrap();
        let ws = WorkspaceFs::new(dir.path().to_path_buf());

        let abs = ws.absolute(Path::new(".agent/tmp/plan.xml"));
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
        ws.write_bytes(Path::new("binary.bin"), &data).unwrap();

        let read_data = ws.read_bytes(Path::new("binary.bin")).unwrap();
        assert_eq!(read_data, data);
    }

    // =========================================================================
    // MemoryWorkspace (test) tests
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
}
