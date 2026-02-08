//! Test helper methods for MemoryWorkspace.
//!
//! This module provides builder-pattern methods for pre-populating a workspace
//! with files and directories, and assertion helpers for tests.

use super::{MemoryFile, MemoryWorkspace};
use std::path::PathBuf;

impl MemoryWorkspace {
    /// Pre-populate a file with content for testing.
    ///
    /// Also creates parent directories automatically.
    ///
    /// # Examples
    ///
    /// ```
    /// use ralph_workflow::workspace::MemoryWorkspace;
    ///
    /// let workspace = MemoryWorkspace::new_test()
    ///     .with_file(".agent/PLAN.md", "# Implementation Plan");
    /// ```
    pub fn with_file(self, path: &str, content: &str) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(path_buf, MemoryFile::new(content.as_bytes().to_vec()));
        self
    }

    /// Pre-populate a file with content and explicit modification time for testing.
    ///
    /// Also creates parent directories automatically.
    ///
    /// # Examples
    ///
    /// ```
    /// use ralph_workflow::workspace::MemoryWorkspace;
    /// use std::time::{SystemTime, Duration};
    ///
    /// let old_time = SystemTime::now() - Duration::from_secs(3600);
    /// let workspace = MemoryWorkspace::new_test()
    ///     .with_file_at_time("old_file.txt", "content", old_time);
    /// ```
    pub fn with_file_at_time(
        self,
        path: &str,
        content: &str,
        modified: std::time::SystemTime,
    ) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(
                path_buf,
                MemoryFile::with_modified(content.as_bytes().to_vec(), modified),
            );
        self
    }

    /// Pre-populate a file with bytes for testing.
    ///
    /// Also creates parent directories automatically.
    ///
    /// # Examples
    ///
    /// ```
    /// use ralph_workflow::workspace::MemoryWorkspace;
    ///
    /// let workspace = MemoryWorkspace::new_test()
    ///     .with_file_bytes("binary.dat", &[0xFF, 0xFE, 0xFD]);
    /// ```
    pub fn with_file_bytes(self, path: &str, content: &[u8]) -> Self {
        let path_buf = PathBuf::from(path);
        self.ensure_parent_dirs(&path_buf);
        self.files
            .write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .insert(path_buf, MemoryFile::new(content.to_vec()));
        self
    }

    /// Pre-populate a directory for testing.
    ///
    /// # Examples
    ///
    /// ```
    /// use ralph_workflow::workspace::MemoryWorkspace;
    ///
    /// let workspace = MemoryWorkspace::new_test()
    ///     .with_dir(".agent/logs");
    /// ```
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
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
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
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(&PathBuf::from(path))
            .map(|f| f.modified)
    }

    /// List all directories (for test assertions).
    pub fn list_directories(&self) -> Vec<PathBuf> {
        self.directories.read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
            .iter().cloned().collect()
    }

    /// Get all files that were written (for test assertions).
    pub fn written_files(&self) -> std::collections::HashMap<PathBuf, Vec<u8>> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .iter()
            .map(|(k, v)| (k.clone(), v.content.clone()))
            .collect()
    }

    /// Get a specific file's content (for test assertions).
    pub fn get_file(&self, path: &str) -> Option<String> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(&PathBuf::from(path))
            .map(|f| String::from_utf8_lossy(&f.content).to_string())
    }

    /// Get a specific file's bytes (for test assertions).
    pub fn get_file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .get(&PathBuf::from(path))
            .map(|f| f.content.clone())
    }

    /// Check if a file was written (for test assertions).
    pub fn was_written(&self, path: &str) -> bool {
        self.files
            .read()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .contains_key(&PathBuf::from(path))
    }

    /// Clear all files (for test setup).
    pub fn clear(&self) {
        self.files.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace files lock")
            .clear();
        self.directories.write()
            .expect("RwLock poisoned - indicates panic in another thread holding MemoryWorkspace directories lock")
            .clear();
    }
}
