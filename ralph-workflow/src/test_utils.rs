//! Shared test utilities.
//!
//! This module provides utilities for testing that need to be shared across
//! multiple modules, particularly for tests that modify global state like
//! the current working directory.

#[cfg(test)]
pub mod testing {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    /// Global mutex for tests that modify the current working directory.
    ///
    /// Since changing CWD affects all threads, tests that do so must be
    /// serialized. This mutex ensures that only one test can change CWD at
    /// a time, preventing race conditions and flaky tests.
    pub static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// RAII guard to restore the working directory on drop.
    struct DirGuard(PathBuf);

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    /// Run a test function in a temporary directory.
    ///
    /// This function:
    /// 1. Acquires a global lock to prevent CWD race conditions
    /// 2. Creates a temporary directory
    /// 3. Changes to that directory
    /// 4. Runs the provided test function
    /// 5. Restores the original directory (even on panic)
    ///
    /// # Panics
    ///
    /// If the mutex is poisoned (a previous test panicked while holding it),
    /// this function will clear the poison and continue. This prevents a single
    /// test failure from causing cascading failures.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use crate::test_utils::testing::with_temp_cwd;
    ///
    /// #[test]
    /// fn test_something() {
    ///     with_temp_cwd(|dir| {
    ///         // dir is the TempDir, and we're already in it
    ///         std::fs::write("test.txt", "hello").unwrap();
    ///         assert!(std::path::Path::new("test.txt").exists());
    ///     });
    /// }
    /// ```
    pub fn with_temp_cwd<F: FnOnce(&TempDir)>(f: F) {
        let lock = CWD_LOCK.get_or_init(|| Mutex::new(()));

        // Clear poison if a previous test panicked
        let _cwd_guard = match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Clear the poison and continue - the directory will be restored
                // by the DirGuard even if the test panics
                poisoned.into_inner()
            }
        };

        let dir = TempDir::new().expect("Failed to create temp directory");
        let old_dir = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
        std::env::set_current_dir(dir.path()).expect("Failed to change to temp directory");
        let _guard = DirGuard(old_dir);

        f(&dir);
    }
}
