//! File management and inspection methods for `MockAppEffectHandler`.
//!
//! This module provides methods to inspect captured effects, query the in-memory
//! filesystem state, and manage files after handler construction.
//!
//! # Inspection Methods
//!
//! These methods allow tests to verify which effects were executed and what
//! state the handler is in:
//!
//! - `captured()` - Get all executed effects in order
//! - `was_executed()` - Check if a specific effect was executed
//! - `effect_count()` - Count total effects executed
//! - `clear_captured()` - Clear effect history for multi-phase testing
//!
//! # Filesystem Queries
//!
//! These methods provide read access to the in-memory filesystem:
//!
//! - `get_file()` - Get content of a specific file
//! - `file_exists()` - Check if a file exists
//! - `get_all_files()` - Get all files as (path, content) tuples
//! - `get_cwd()` - Get current working directory
//!
//! # File Management
//!
//! These methods allow modifying the filesystem after construction:
//!
//! - `add_file()` - Add/update a file (non-builder version)
//! - `remove_file()` - Delete a file
//!
//! # Example Usage
//!
//! ```ignore
//! let mut handler = MockAppEffectHandler::new()
//!     .with_file("initial.txt", "content");
//!
//! handler.execute(AppEffect::ReadFile {
//!     path: "initial.txt".into(),
//! });
//!
//! // Inspect execution
//! assert_eq!(handler.effect_count(), 1);
//! assert!(handler.was_executed(&AppEffect::ReadFile {
//!     path: "initial.txt".into(),
//! }));
//!
//! // Query filesystem
//! assert!(handler.file_exists(&PathBuf::from("initial.txt")));
//! assert_eq!(handler.get_file(&PathBuf::from("initial.txt")),
//!            Some("content".to_string()));
//!
//! // Add more files after construction
//! handler.add_file("dynamic.txt", "new content");
//! ```

use super::super::effect::AppEffect;
use super::core::MockAppEffectHandler;
use std::path::PathBuf;

impl MockAppEffectHandler {
    // =========================================================================
    // Inspection Methods
    // =========================================================================

    /// Get all captured effects in execution order.
    ///
    /// Returns a vector of all effects that have been executed via `execute()`,
    /// in the order they were executed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let effects = handler.captured();
    /// assert_eq!(effects.len(), 3);
    /// assert!(matches!(effects[0], AppEffect::GitRequireRepo));
    /// ```
    pub fn captured(&self) -> Vec<AppEffect> {
        self.captured_effects.borrow().clone()
    }

    /// Check if a specific effect was executed.
    ///
    /// Uses [`PartialEq`] comparison to match effects. Note that effects
    /// with data fields must match exactly (including all field values).
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert!(handler.was_executed(&AppEffect::GitRequireRepo));
    ///
    /// // For effects with data, all fields must match
    /// assert!(handler.was_executed(&AppEffect::ReadFile {
    ///     path: PathBuf::from("config.toml"),
    /// }));
    /// ```
    pub fn was_executed(&self, effect: &AppEffect) -> bool {
        self.captured_effects.borrow().contains(effect)
    }

    /// Get the number of captured effects.
    ///
    /// Returns the total count of effects executed via `execute()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert_eq!(handler.effect_count(), 0);
    /// handler.execute(AppEffect::GitRequireRepo);
    /// assert_eq!(handler.effect_count(), 1);
    /// ```
    pub fn effect_count(&self) -> usize {
        self.captured_effects.borrow().len()
    }

    /// Clear all captured effects.
    ///
    /// Removes all effects from the captured effects list. Useful for testing
    /// multiple phases where you want to verify effects from a specific phase only.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Phase 1
    /// handler.execute(AppEffect::GitRequireRepo);
    /// assert_eq!(handler.effect_count(), 1);
    ///
    /// // Clear and start Phase 2
    /// handler.clear_captured();
    /// assert_eq!(handler.effect_count(), 0);
    ///
    /// handler.execute(AppEffect::GitGetHeadOid);
    /// assert_eq!(handler.effect_count(), 1);  // Only Phase 2 effects counted
    /// ```
    pub fn clear_captured(&self) {
        self.captured_effects.borrow_mut().clear();
    }

    // =========================================================================
    // Filesystem Query Methods
    // =========================================================================

    /// Get the content of a file from the in-memory filesystem.
    ///
    /// Returns `Some(content)` if the file exists, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_file("config.toml", "key = value");
    ///
    /// assert_eq!(handler.get_file(&PathBuf::from("config.toml")),
    ///            Some("key = value".to_string()));
    /// assert_eq!(handler.get_file(&PathBuf::from("missing.txt")), None);
    /// ```
    pub fn get_file(&self, path: &PathBuf) -> Option<String> {
        self.files.borrow().get(path).cloned()
    }

    /// Check if a file exists in the in-memory filesystem.
    ///
    /// Returns `true` if the file exists, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_file("config.toml", "key = value");
    ///
    /// assert!(handler.file_exists(&PathBuf::from("config.toml")));
    /// assert!(!handler.file_exists(&PathBuf::from("missing.txt")));
    /// ```
    pub fn file_exists(&self, path: &PathBuf) -> bool {
        self.files.borrow().contains_key(path)
    }

    /// Get all files in the in-memory filesystem.
    ///
    /// Returns a vector of (path, content) tuples for all files currently
    /// stored in the handler's filesystem.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_file("a.txt", "content a")
    ///     .with_file("b.txt", "content b");
    ///
    /// let files = handler.get_all_files();
    /// assert_eq!(files.len(), 2);
    /// ```
    pub fn get_all_files(&self) -> Vec<(PathBuf, String)> {
        self.files
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Get the current simulated working directory.
    ///
    /// Returns the CWD as set by `with_cwd()` or `SetCurrentDir` effects.
    /// Default is "/".
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handler = MockAppEffectHandler::new()
    ///     .with_cwd("/home/user/project");
    ///
    /// assert_eq!(handler.get_cwd(), PathBuf::from("/home/user/project"));
    /// ```
    pub fn get_cwd(&self) -> PathBuf {
        self.cwd.borrow().clone()
    }

    /// Get all captured log messages.
    ///
    /// Returns tuples of (level, message) where level is one of:
    /// "info", "success", "warn", "error".
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut handler = MockAppEffectHandler::new();
    ///
    /// handler.execute(AppEffect::LogInfo {
    ///     message: "Starting".to_string(),
    /// });
    /// handler.execute(AppEffect::LogSuccess {
    ///     message: "Done".to_string(),
    /// });
    ///
    /// let logs = handler.get_log_messages();
    /// assert_eq!(logs.len(), 2);
    /// assert_eq!(logs[0], ("info".to_string(), "Starting".to_string()));
    /// assert_eq!(logs[1], ("success".to_string(), "Done".to_string()));
    /// ```
    pub fn get_log_messages(&self) -> Vec<(String, String)> {
        self.log_messages.borrow().clone()
    }

    // =========================================================================
    // File Management Methods
    // =========================================================================

    /// Add a file to the in-memory filesystem (non-builder version).
    ///
    /// Unlike `with_file`, this method takes `&mut self` instead of consuming
    /// and returning `self`, making it suitable for use after handler construction.
    /// This is useful for syncing workspace files back to handler after pipeline
    /// execution.
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file
    /// * `content` - The content of the file
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut handler = MockAppEffectHandler::new();
    ///
    /// // Add file after construction
    /// handler.add_file("dynamic.txt", "runtime content");
    ///
    /// assert!(handler.file_exists(&PathBuf::from("dynamic.txt")));
    /// ```
    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.borrow_mut().insert(path.into(), content.into());
    }

    /// Remove a file from the in-memory filesystem.
    ///
    /// This method removes a file from the handler's in-memory filesystem.
    /// Used for syncing deletions from workspace back to handler after pipeline
    /// execution.
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to remove
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut handler = MockAppEffectHandler::new()
    ///     .with_file("temp.txt", "content");
    ///
    /// assert!(handler.file_exists(&PathBuf::from("temp.txt")));
    ///
    /// handler.remove_file(&PathBuf::from("temp.txt"));
    ///
    /// assert!(!handler.file_exists(&PathBuf::from("temp.txt")));
    /// ```
    pub fn remove_file(&mut self, path: &PathBuf) {
        self.files.borrow_mut().remove(path);
    }
}
