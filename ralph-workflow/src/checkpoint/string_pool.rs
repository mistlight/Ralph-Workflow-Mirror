//! String interning for deduplicating repeated strings in execution history.
//!
//! This module provides a string pool that deduplicates commonly repeated strings
//! (like phase names and agent names) by storing them as Arc<str>. This reduces
//! memory usage when the same strings appear many times across execution history.

use std::collections::HashMap;
use std::sync::Arc;

/// String pool for deduplicating commonly repeated strings in execution history.
///
/// Phase names and agent names are repeated frequently across execution history
/// entries. Using Arc<str> with a string pool reduces memory usage by sharing
/// the same allocation for identical strings.
///
/// # Example
///
/// ```
/// use ralph_workflow::checkpoint::string_pool::StringPool;
///
/// let mut pool = StringPool::new();
/// let phase1 = pool.intern("Development");
/// let phase2 = pool.intern("Development");
///
/// // Both Arc<str> values point to the same allocation
/// assert!(Arc::ptr_eq(&phase1, &phase2));
/// ```
#[derive(Debug, Clone, Default)]
pub struct StringPool {
    pool: HashMap<String, Arc<str>>,
}

impl StringPool {
    /// Create a new empty string pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or insert a string into the pool, returning an Arc<str>.
    ///
    /// If the string already exists in the pool, returns a clone of the existing
    /// Arc<str> (which is a cheap reference count increment). If the string is
    /// new, it is inserted into the pool.
    ///
    /// # Example
    ///
    /// ```
    /// use ralph_workflow::checkpoint::string_pool::StringPool;
    ///
    /// let mut pool = StringPool::new();
    /// let s1 = pool.intern("test");
    /// let s2 = pool.intern("test");
    /// assert!(std::sync::Arc::ptr_eq(&s1, &s2));
    /// ```
    pub fn intern(&mut self, s: impl Into<String>) -> Arc<str> {
        let s = s.into();
        self.pool
            .entry(s.clone())
            .or_insert_with(|| Arc::from(s.as_str()))
            .clone()
    }

    /// Get the number of unique strings in the pool.
    pub fn len(&self) -> usize {
        self.pool.len()
    }

    /// Check if the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.pool.is_empty()
    }

    /// Clear all entries from the pool.
    pub fn clear(&mut self) {
        self.pool.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_pool_new() {
        let pool = StringPool::new();
        assert_eq!(pool.len(), 0);
        assert!(pool.is_empty());
    }

    #[test]
    fn test_identical_strings_return_same_arc() {
        let mut pool = StringPool::new();
        let s1 = pool.intern("Development");
        let s2 = pool.intern("Development");

        // Both should point to the same allocation
        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(*s1, *s2);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_different_strings_return_different_arc() {
        let mut pool = StringPool::new();
        let s1 = pool.intern("Development");
        let s2 = pool.intern("Review");

        // Should point to different allocations
        assert!(!Arc::ptr_eq(&s1, &s2));
        assert_ne!(*s1, *s2);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_pool_size_does_not_grow_for_repeated_strings() {
        let mut pool = StringPool::new();

        // Intern the same string multiple times
        for _ in 0..100 {
            pool.intern("Development");
        }

        // Pool should still only contain one entry
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_intern_different_string_types() {
        let mut pool = StringPool::new();

        // Test with &str
        let s1 = pool.intern("test");
        // Test with String
        let s2 = pool.intern("test".to_string());
        // Test with owned String
        let s3 = pool.intern(String::from("test"));

        // All should point to the same allocation
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(Arc::ptr_eq(&s2, &s3));
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut pool = StringPool::new();
        pool.intern("Development");
        pool.intern("Review");
        assert_eq!(pool.len(), 2);

        pool.clear();
        assert_eq!(pool.len(), 0);
        assert!(pool.is_empty());
    }

    #[test]
    fn test_arc_content_matches_input() {
        let mut pool = StringPool::new();
        let arc = pool.intern("Development");
        assert_eq!(&*arc, "Development");
    }

    #[test]
    fn test_memory_efficiency_multiple_calls() {
        let mut pool = StringPool::new();
        let mut arcs = Vec::new();

        // Create 1000 references to the same string
        for _ in 0..1000 {
            arcs.push(pool.intern("Development"));
        }

        // Pool should still only contain one entry
        assert_eq!(pool.len(), 1);

        // All arcs should point to the same allocation
        for i in 1..arcs.len() {
            assert!(Arc::ptr_eq(&arcs[0], &arcs[i]));
        }
    }

    #[test]
    fn test_empty_string() {
        let mut pool = StringPool::new();
        let s1 = pool.intern("");
        let s2 = pool.intern("");

        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(&*s1, "");
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_clone_pool() {
        let mut pool = StringPool::new();
        pool.intern("Development");
        pool.intern("Review");

        let cloned = pool.clone();
        assert_eq!(pool.len(), cloned.len());
    }
}
