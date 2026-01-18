//! File operations trait for testable file I/O.
//!
//! This module provides a trait-based abstraction for file operations that allows
//! mocking file system side effects in tests. Only external side effects (file reads,
//! writes, existence checks) are abstracted - internal code logic is never mocked.
//!
//! **NOTE:** This module is only available when the `test-utils` feature is enabled.
//! The trait infrastructure is designed for integration testing purposes.
//!
//! # Design Philosophy
//!
//! The `FileOps` trait follows the same pattern as `GitOps` and `AgentExecutor`:
//! - Production code uses `RealFileOps` which delegates to `std::fs`
//! - Test code uses `MockFileOps` which captures calls and returns configured responses
//! - The trait only abstracts external I/O operations, not business logic
//!
//! # Example
//!
//! ```ignore
//! use ralph_workflow::files::io::file_ops::{FileOps, RealFileOps};
//! use std::path::Path;
//!
//! fn save_plan<F: FileOps>(file_ops: &F, content: &str) -> std::io::Result<()> {
//!     file_ops.write_file(Path::new(".agent/PLAN.md"), content)
//! }
//!
//! // Production: uses real file system
//! let real_ops = RealFileOps;
//! save_plan(&real_ops, "# Plan\n\nStep 1...").unwrap();
//!
//! // Testing: uses mock that captures calls
//! #[cfg(test)]
//! {
//!     use ralph_workflow::files::io::file_ops::MockFileOps;
//!     let mock_ops = MockFileOps::new();
//!     save_plan(&mock_ops, "# Plan\n\nStep 1...").unwrap();
//!     assert!(mock_ops.was_written(Path::new(".agent/PLAN.md")));
//! }
//! ```
//!
//! # Feature Gating
//!
//! The entire module is gated behind `test-utils` or `test` configuration:
//! - In tests (`#[cfg(test)]`), the module is always available
//! - For external consumers, enable the `test-utils` feature
//!
//! This ensures no unused code warnings in production builds while still
//! providing comprehensive testing infrastructure.

use std::io;
use std::path::Path;

/// Trait for file system operations.
///
/// This trait abstracts file operations for testing purposes. Implementations
/// can either call real `std::fs` functions (production) or capture calls for
/// assertion (testing).
///
/// Only external side effects are abstracted: file reads, writes, existence checks,
/// and directory operations. Internal code logic is never mocked.
pub trait FileOps {
    /// Read the contents of a file as a string.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to read
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Write content to a file, creating parent directories if needed.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to write
    /// * `content` - Content to write
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    fn write_file(&self, path: &Path, content: &str) -> io::Result<()>;

    /// Check if a path exists.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check
    ///
    /// # Returns
    ///
    /// `true` if the path exists, `false` otherwise.
    fn exists(&self, path: &Path) -> bool;

    /// Check if a path is a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check
    ///
    /// # Returns
    ///
    /// `true` if the path is a file, `false` otherwise.
    fn is_file(&self, path: &Path) -> bool;

    /// Remove a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be removed.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Create a directory and all parent directories.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory to create
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
}

/// Real file operations implementation.
///
/// This implementation delegates to `std::fs` for all operations.
/// Used in production code.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFileOps;

impl RealFileOps {
    /// Create a new real file operations instance.
    pub fn new() -> Self {
        Self
    }
}

impl FileOps for RealFileOps {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_file(&self, path: &Path, content: &str) -> io::Result<()> {
        // Ensure parent directories exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
}

// Test utilities are only available when test-utils feature is enabled
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Clonable representation of an io::Result.
    ///
    /// Since `io::Error` doesn't implement Clone, we store error info as strings
    /// and reconstruct the error on demand.
    #[derive(Debug, Clone)]
    enum MockResult<T: Clone> {
        Ok(T),
        Err {
            kind: io::ErrorKind,
            message: String,
        },
    }

    impl<T: Clone> MockResult<T> {
        fn to_io_result(&self) -> io::Result<T> {
            match self {
                MockResult::Ok(v) => Ok(v.clone()),
                MockResult::Err { kind, message } => Err(io::Error::new(*kind, message.clone())),
            }
        }

        fn from_io_result(result: io::Result<T>) -> Self {
            match result {
                Ok(v) => MockResult::Ok(v),
                Err(e) => MockResult::Err {
                    kind: e.kind(),
                    message: e.to_string(),
                },
            }
        }
    }

    impl<T: Clone + Default> Default for MockResult<T> {
        fn default() -> Self {
            MockResult::Ok(T::default())
        }
    }

    /// A recorded file operation for test assertions.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum FileOperation {
        /// A read operation on the given path.
        Read(PathBuf),
        /// A write operation with path and content.
        Write(PathBuf, String),
        /// An exists check on the given path.
        Exists(PathBuf),
        /// An is_file check on the given path.
        IsFile(PathBuf),
        /// A remove operation on the given path.
        Remove(PathBuf),
        /// A create_dir_all operation on the given path.
        CreateDirAll(PathBuf),
    }

    /// Mock file operations implementation for testing.
    ///
    /// This implementation captures all file operations for assertion and
    /// returns configured responses. It does NOT interact with the real file system.
    #[derive(Debug)]
    pub struct MockFileOps {
        /// All operations performed, in order.
        operations: RefCell<Vec<FileOperation>>,

        /// Virtual file system: path -> content.
        files: RefCell<HashMap<PathBuf, String>>,

        /// Paths that should return errors on read.
        read_errors: RefCell<HashMap<PathBuf, MockResult<String>>>,

        /// Paths that should return errors on write.
        write_errors: RefCell<HashMap<PathBuf, MockResult<()>>>,

        /// Paths that should return errors on remove.
        remove_errors: RefCell<HashMap<PathBuf, MockResult<()>>>,

        /// Default error to return for all operations (if set).
        default_error: RefCell<Option<(io::ErrorKind, String)>>,
    }

    impl Default for MockFileOps {
        fn default() -> Self {
            Self {
                operations: RefCell::new(Vec::new()),
                files: RefCell::new(HashMap::new()),
                read_errors: RefCell::new(HashMap::new()),
                write_errors: RefCell::new(HashMap::new()),
                remove_errors: RefCell::new(HashMap::new()),
                default_error: RefCell::new(None),
            }
        }
    }

    impl MockFileOps {
        /// Create a new mock file operations instance.
        pub fn new() -> Self {
            Self::default()
        }

        /// Create a mock that returns errors for all operations.
        pub fn new_error() -> Self {
            let mock = Self::new();
            mock.default_error
                .replace(Some((io::ErrorKind::Other, "mock file error".to_string())));
            mock
        }

        // Builder methods

        /// Add a file with the given content to the virtual file system.
        pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
            self.files.borrow_mut().insert(path.into(), content.into());
            self
        }

        /// Configure a read to fail with the given error.
        pub fn with_read_error(self, path: impl Into<PathBuf>, error: io::Error) -> Self {
            self.read_errors
                .borrow_mut()
                .insert(path.into(), MockResult::from_io_result(Err(error)));
            self
        }

        /// Configure a write to fail with the given error.
        pub fn with_write_error(self, path: impl Into<PathBuf>, error: io::Error) -> Self {
            self.write_errors
                .borrow_mut()
                .insert(path.into(), MockResult::from_io_result(Err(error)));
            self
        }

        /// Configure a remove to fail with the given error.
        pub fn with_remove_error(self, path: impl Into<PathBuf>, error: io::Error) -> Self {
            self.remove_errors
                .borrow_mut()
                .insert(path.into(), MockResult::from_io_result(Err(error)));
            self
        }

        // Assertion methods

        /// Get all recorded operations.
        pub fn operations(&self) -> Vec<FileOperation> {
            self.operations.borrow().clone()
        }

        /// Get the number of operations performed.
        pub fn operation_count(&self) -> usize {
            self.operations.borrow().len()
        }

        /// Check if a specific path was written to.
        pub fn was_written(&self, path: &Path) -> bool {
            self.operations
                .borrow()
                .iter()
                .any(|op| matches!(op, FileOperation::Write(p, _) if p == path))
        }

        /// Check if a specific path was read from.
        pub fn was_read(&self, path: &Path) -> bool {
            self.operations
                .borrow()
                .iter()
                .any(|op| matches!(op, FileOperation::Read(p) if p == path))
        }

        /// Check if a specific path was removed.
        pub fn was_removed(&self, path: &Path) -> bool {
            self.operations
                .borrow()
                .iter()
                .any(|op| matches!(op, FileOperation::Remove(p) if p == path))
        }

        /// Get the content written to a specific path (most recent write).
        pub fn get_written_content(&self, path: &Path) -> Option<String> {
            self.operations
                .borrow()
                .iter()
                .rev()
                .find_map(|op| match op {
                    FileOperation::Write(p, content) if p == path => Some(content.clone()),
                    _ => None,
                })
        }

        /// Get all paths that were written to.
        pub fn written_paths(&self) -> Vec<PathBuf> {
            self.operations
                .borrow()
                .iter()
                .filter_map(|op| match op {
                    FileOperation::Write(p, _) => Some(p.clone()),
                    _ => None,
                })
                .collect()
        }

        /// Get all write operations.
        pub fn writes(&self) -> Vec<(PathBuf, String)> {
            self.operations
                .borrow()
                .iter()
                .filter_map(|op| match op {
                    FileOperation::Write(p, content) => Some((p.clone(), content.clone())),
                    _ => None,
                })
                .collect()
        }

        /// Get the virtual file system contents.
        pub fn files(&self) -> HashMap<PathBuf, String> {
            self.files.borrow().clone()
        }

        /// Clear all recorded operations.
        pub fn clear(&self) {
            self.operations.borrow_mut().clear();
        }

        /// Clear all virtual files.
        pub fn clear_files(&self) {
            self.files.borrow_mut().clear();
        }

        /// Helper to check for default error.
        fn check_default_error<T: Default>(&self) -> Option<io::Result<T>> {
            self.default_error
                .borrow()
                .as_ref()
                .map(|(kind, msg)| Err(io::Error::new(*kind, msg.clone())))
        }
    }

    impl FileOps for MockFileOps {
        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            self.operations
                .borrow_mut()
                .push(FileOperation::Read(path.to_path_buf()));

            // Check for default error
            if let Some(err) = self.check_default_error() {
                return err;
            }

            // Check for path-specific error
            if let Some(result) = self.read_errors.borrow().get(path) {
                return result.to_io_result();
            }

            // Return content from virtual file system
            self.files.borrow().get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("mock file not found: {}", path.display()),
                )
            })
        }

        fn write_file(&self, path: &Path, content: &str) -> io::Result<()> {
            self.operations.borrow_mut().push(FileOperation::Write(
                path.to_path_buf(),
                content.to_string(),
            ));

            // Check for default error
            if let Some(err) = self.check_default_error() {
                return err;
            }

            // Check for path-specific error
            if let Some(result) = self.write_errors.borrow().get(path) {
                return result.to_io_result();
            }

            // Store in virtual file system
            self.files
                .borrow_mut()
                .insert(path.to_path_buf(), content.to_string());
            Ok(())
        }

        fn exists(&self, path: &Path) -> bool {
            self.operations
                .borrow_mut()
                .push(FileOperation::Exists(path.to_path_buf()));

            // Check for default error - returns false in error mode
            if self.default_error.borrow().is_some() {
                return false;
            }

            self.files.borrow().contains_key(path)
        }

        fn is_file(&self, path: &Path) -> bool {
            self.operations
                .borrow_mut()
                .push(FileOperation::IsFile(path.to_path_buf()));

            // In mock, all entries are files (no directories)
            self.files.borrow().contains_key(path)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.operations
                .borrow_mut()
                .push(FileOperation::Remove(path.to_path_buf()));

            // Check for default error
            if let Some(err) = self.check_default_error() {
                return err;
            }

            // Check for path-specific error
            if let Some(result) = self.remove_errors.borrow().get(path) {
                return result.to_io_result();
            }

            // Remove from virtual file system
            if self.files.borrow_mut().remove(path).is_some() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("mock file not found: {}", path.display()),
                ))
            }
        }

        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            self.operations
                .borrow_mut()
                .push(FileOperation::CreateDirAll(path.to_path_buf()));

            // Check for default error
            if let Some(err) = self.check_default_error() {
                return err;
            }

            // Mock always succeeds for directory creation
            Ok(())
        }
    }
}

// Re-export test utilities when the feature is enabled
#[cfg(any(test, feature = "test-utils"))]
pub use test_utils::{FileOperation, MockFileOps};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_file_ops_write_and_read() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        let ops = RealFileOps::new();

        ops.write_file(&file_path, "Hello, World!").unwrap();
        assert!(ops.exists(&file_path));
        assert!(ops.is_file(&file_path));

        let content = ops.read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, World!");

        ops.remove_file(&file_path).unwrap();
        assert!(!ops.exists(&file_path));
    }

    #[test]
    fn test_real_file_ops_creates_parent_dirs() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("nested/deep/file.txt");
        let ops = RealFileOps::new();

        ops.write_file(&file_path, "content").unwrap();
        assert!(ops.exists(&file_path));
    }

    #[test]
    fn test_mock_file_ops_captures_operations() {
        let mock = MockFileOps::new().with_file(".agent/PLAN.md", "# Plan");

        let _ = mock.read_to_string(Path::new(".agent/PLAN.md"));
        let _ = mock.write_file(Path::new(".agent/ISSUES.md"), "# Issues");
        let _ = mock.exists(Path::new("PROMPT.md"));

        assert_eq!(mock.operation_count(), 3);
        assert!(mock.was_read(Path::new(".agent/PLAN.md")));
        assert!(mock.was_written(Path::new(".agent/ISSUES.md")));
    }

    #[test]
    fn test_mock_file_ops_virtual_filesystem() {
        let mock = MockFileOps::new()
            .with_file("file1.txt", "content1")
            .with_file("file2.txt", "content2");

        assert!(mock.exists(Path::new("file1.txt")));
        assert!(mock.exists(Path::new("file2.txt")));
        assert!(!mock.exists(Path::new("file3.txt")));

        let content = mock.read_to_string(Path::new("file1.txt")).unwrap();
        assert_eq!(content, "content1");
    }

    #[test]
    fn test_mock_file_ops_write_updates_filesystem() {
        let mock = MockFileOps::new();

        assert!(!mock.exists(Path::new("new.txt")));

        mock.write_file(Path::new("new.txt"), "new content")
            .unwrap();

        assert!(mock.exists(Path::new("new.txt")));
        let content = mock.read_to_string(Path::new("new.txt")).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_mock_file_ops_remove() {
        let mock = MockFileOps::new().with_file("to_delete.txt", "content");

        assert!(mock.exists(Path::new("to_delete.txt")));
        mock.remove_file(Path::new("to_delete.txt")).unwrap();
        assert!(!mock.exists(Path::new("to_delete.txt")));
        assert!(mock.was_removed(Path::new("to_delete.txt")));
    }

    #[test]
    fn test_mock_file_ops_read_nonexistent_returns_error() {
        let mock = MockFileOps::new();
        let result = mock.read_to_string(Path::new("nonexistent.txt"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_mock_file_ops_configured_errors() {
        let mock = MockFileOps::new()
            .with_file("exists.txt", "content")
            .with_read_error(
                "exists.txt",
                io::Error::new(io::ErrorKind::PermissionDenied, "cannot read"),
            );

        let result = mock.read_to_string(Path::new("exists.txt"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn test_mock_file_ops_error_mode() {
        let mock = MockFileOps::new_error();

        assert!(mock.read_to_string(Path::new("any.txt")).is_err());
        assert!(mock.write_file(Path::new("any.txt"), "content").is_err());
        assert!(mock.remove_file(Path::new("any.txt")).is_err());
    }

    #[test]
    fn test_mock_file_ops_get_written_content() {
        let mock = MockFileOps::new();

        mock.write_file(Path::new("test.txt"), "first").unwrap();
        mock.write_file(Path::new("test.txt"), "second").unwrap();

        let content = mock.get_written_content(Path::new("test.txt"));
        assert_eq!(content, Some("second".to_string()));
    }

    #[test]
    fn test_mock_file_ops_writes() {
        let mock = MockFileOps::new();

        mock.write_file(Path::new("a.txt"), "content a").unwrap();
        mock.write_file(Path::new("b.txt"), "content b").unwrap();

        let writes = mock.writes();
        assert_eq!(writes.len(), 2);
    }
}
