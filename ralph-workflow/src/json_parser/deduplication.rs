//! Delta deduplication using KMP and Rolling Hash algorithms.
//!
//! This module provides efficient deduplication for streaming deltas using:
//! - **Rolling Hash (Rabin-Karp)**: Fast O(n) filtering to eliminate impossible matches
//! - **KMP (Knuth-Morris-Pratt)**: O(n+m) verification for exact substring matching
//!
//! The two-phase approach ensures optimal performance:
//! 1. Rolling hash quickly filters out non-matches
//! 2. KMP verifies actual matches when hash collides
//!
//! # Architecture
//!
//! ```text
//! Incoming Delta
//!       │
//!       ▼
//! ┌─────────────────────┐
//! │  Rolling Hash Check │  ◄── Compute hash of delta, compare against
//! │  (Rabin-Karp)       │      sliding window hashes of accumulated content
//! └──────────┬──────────┘
//!            │
//!     ┌──────┴──────┐
//!     │ Hash Match? │
//!     └──────┬──────┘
//!       No   │   Yes
//!       │    │
//!       ▼    ▼
//!    Accept  ┌─────────────────┐
//!    Delta   │  KMP Verification│  ◄── Confirm actual substring match
//!            └────────┬────────┘
//!                     │
//!              ┌──────┴──────┐
//!              │True Match?  │
//!              └──────┬──────┘
//!                No   │   Yes
//!                │    │
//!                ▼    ▼
//!             Accept  Extract New
//!             Delta   Portion Only
//! ```

#![allow(dead_code)]

use std::collections::HashMap;

/// Rolling hash window for fast substring detection.
///
/// Maintains rolling hash values over accumulated content using the Rabin-Karp
/// algorithm. Provides O(1) hash computation for sliding windows.
///
/// # Algorithm
///
/// Uses polynomial rolling hash with base 256 (byte values) and modulus
/// from a large prime to minimize collisions.
///
/// Hash computation:
/// ```text
/// hash(s[0..n]) = (s[0] * b^(n-1) + s[1] * b^(n-2) + ... + s[n-1]) mod m
/// ```
///
/// Rolling update:
/// ```text
/// hash(s[i+1..i+n+1]) = ((hash(s[i..i+n]) - s[i] * b^(n-1)) * b + s[i+n]) mod m
/// ```
///
/// # Example
///
/// ```ignore
/// let mut window = RollingHashWindow::new();
/// window.add_content("Hello World");
///
/// // Check if "World" exists in the accumulated content
/// let hashes = window.get_window_hashes(5); // 5 = length of "World"
/// let world_hash = RollingHashWindow::compute_hash("World");
///
/// if hashes.contains(&world_hash) {
///     // Potential match - verify with KMP
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct RollingHashWindow {
    /// Accumulated content for hash computation
    content: String,
    /// Cached hash values for different window sizes
    /// Maps `window_size` -> Vec<(position, hash)>
    cached_hashes: HashMap<usize, Vec<(usize, u64)>>,
}

impl RollingHashWindow {
    /// Base for polynomial rolling hash (256 for byte values)
    const BASE: u64 = 256;
    /// Modulus for hash computation (large prime to minimize collisions)
    const MODULUS: u64 = 2_147_483_647; // 2^31 - 1 (Mersenne prime)

    /// Create a new rolling hash window.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute rolling hash of a string slice.
    ///
    /// Uses polynomial rolling hash with base 256 and a large prime modulus.
    ///
    /// # Arguments
    /// * `text` - The text to hash
    ///
    /// # Returns
    /// The hash value as a u64
    ///
    /// # Example
    /// ```ignore
    /// let hash = RollingHashWindow::compute_hash("Hello");
    /// ```
    pub fn compute_hash(text: &str) -> u64 {
        let mut hash: u64 = 0;
        for byte in text.bytes() {
            hash = (hash * Self::BASE + u64::from(byte)) % Self::MODULUS;
        }
        hash
    }

    /// Compute base^(n-1) mod MODULUS for rolling hash updates.
    ///
    /// This is used to efficiently remove the leftmost character when
    /// sliding the window.
    fn compute_power(power: usize) -> u64 {
        let mut result = 1u64;
        for _ in 0..power {
            result = (result * Self::BASE) % Self::MODULUS;
        }
        result
    }

    /// Add content to the window and update cached hashes.
    ///
    /// This appends new content and recomputes hash values for all
    /// cached window sizes.
    ///
    /// # Arguments
    /// * `text` - The new content to add
    pub fn add_content(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let _start_pos = self.content.len();
        self.content.push_str(text);

        // Invalidate and recompute all cached hashes
        self.cached_hashes.clear();
    }

    /// Get all window hashes of a specific size.
    ///
    /// Computes rolling hash values for all windows of the given size
    /// in the accumulated content.
    ///
    /// # Arguments
    /// * `window_size` - The size of each window
    ///
    /// # Returns
    /// A vector of (position, hash) tuples for each window
    ///
    /// # Example
    /// ```ignore
    /// // Get hashes for all 5-character windows
    /// let hashes = window.get_window_hashes(5);
    /// ```
    pub fn get_window_hashes(&mut self, window_size: usize) -> Vec<(usize, u64)> {
        // Return cached copy if available
        if let Some(hashes) = self.cached_hashes.get(&window_size) {
            return hashes.clone();
        }

        // Compute hashes for this window size
        let content_bytes = self.content.as_bytes();
        let content_len = content_bytes.len();

        if content_len < window_size {
            // Not enough content for even one window
            let empty: Vec<(usize, u64)> = Vec::new();
            self.cached_hashes.insert(window_size, empty.clone());
            return empty;
        }

        let mut hashes = Vec::new();
        let mut hash: u64 = 0;

        // Compute initial window hash
        for byte in content_bytes.iter().take(window_size) {
            hash = (hash * Self::BASE + u64::from(*byte)) % Self::MODULUS;
        }
        hashes.push((0, hash));

        // Precompute base^(window_size-1) mod MODULUS for rolling updates
        let power = Self::compute_power(window_size - 1);

        // Slide window and compute rolling hashes
        for i in 1..=(content_len - window_size) {
            // Remove leftmost character contribution
            let leftmost = u64::from(content_bytes[i - 1]);
            let removed = (leftmost * power) % Self::MODULUS;
            hash = (hash + Self::MODULUS - removed) % Self::MODULUS;

            // Shift and add new character
            hash = (hash * Self::BASE) % Self::MODULUS;
            let new_char = u64::from(content_bytes[i + window_size - 1]);
            hash = (hash + new_char) % Self::MODULUS;

            hashes.push((i, hash));
        }

        // Cache for future use
        self.cached_hashes.insert(window_size, hashes.clone());
        hashes
    }

    /// Check if a hash exists in any window of the given size.
    ///
    /// This is a fast O(1) check after hashes have been computed.
    ///
    /// # Arguments
    /// * `hash` - The hash value to search for
    /// * `window_size` - The window size to check
    ///
    /// # Returns
    /// * `Some(position)` - The position where the hash was found
    /// * `None` - Hash not found
    pub fn contains_hash(&mut self, hash: u64, window_size: usize) -> Option<usize> {
        let hashes = self.get_window_hashes(window_size);
        hashes
            .into_iter()
            .find(|(_, h)| *h == hash)
            .map(|(pos, _)| pos)
    }

    /// Clear all content and cached hashes.
    pub fn clear(&mut self) {
        self.content.clear();
        self.cached_hashes.clear();
    }

    /// Get the current content length.
    pub const fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if the window is empty.
    pub const fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

/// KMP (Knuth-Morris-Pratt) matcher for exact substring matching.
///
/// Provides linear-time substring search by precomputing a failure function
/// that allows skipping already-matched characters.
///
/// # Algorithm
///
/// The failure function (also called "prefix function" or "lps array") stores
/// the length of the longest proper prefix of the pattern that is also a suffix
/// for each position in the pattern.
///
/// # Example
///
/// ```ignore
/// let mut kmp = KMPMatcher::new("abc");
/// let text = "xyzabcuvw";
///
/// if let Some(pos) = kmp.find(text) {
///     // Found "abc" at position 3
/// }
/// ```
#[derive(Debug, Clone)]
pub struct KMPMatcher {
    /// The pattern to search for
    pattern: String,
    /// Failure function (longest proper prefix which is also suffix)
    failure: Vec<usize>,
}

impl KMPMatcher {
    /// Create a new KMP matcher for the given pattern.
    ///
    /// Precomputes the failure function for efficient searching.
    ///
    /// # Arguments
    /// * `pattern` - The pattern to search for
    ///
    /// # Example
    /// ```ignore
    /// let matcher = KMPMatcher::new("hello");
    /// ```
    pub fn new(pattern: &str) -> Self {
        let pattern = pattern.to_string();
        let failure = Self::compute_failure(&pattern);
        Self { pattern, failure }
    }

    /// Compute the failure function for KMP.
    ///
    /// The failure function `lps[i]` stores the length of the longest proper
    /// prefix of `pattern[0..=i]` that is also a suffix.
    ///
    /// # Example
    ///
    /// For pattern "abab":
    /// - lps[0] = 0 (no proper prefix for single char)
    /// - lps[1] = 0 ("ab" has no matching prefix/suffix)
    /// - lps[2] = 1 ("aba" has "a" as prefix and suffix)
    /// - lps[3] = 2 ("abab" has "ab" as prefix and suffix)
    fn compute_failure(pattern: &str) -> Vec<usize> {
        let m = pattern.len();
        if m == 0 {
            return Vec::new();
        }

        let mut lps = vec![0; m];
        let mut len = 0; // Length of the previous longest prefix suffix
        let mut i = 1;

        let pattern_bytes = pattern.as_bytes();

        while i < m {
            if pattern_bytes[i] == pattern_bytes[len] {
                len += 1;
                lps[i] = len;
                i += 1;
            } else if len != 0 {
                // Fall back to previous longest prefix suffix
                len = lps[len - 1];
                // Note: we don't increment i here
            } else {
                // No proper prefix suffix found
                lps[i] = 0;
                i += 1;
            }
        }

        lps
    }

    /// Find the pattern in text, returning the first position.
    ///
    /// Uses the precomputed failure function for O(n) search time.
    ///
    /// # Arguments
    /// * `text` - The text to search in
    ///
    /// # Returns
    /// * `Some(position)` - The starting position of the pattern
    /// * `None` - Pattern not found
    ///
    /// # Example
    /// ```ignore
    /// let matcher = KMPMatcher::new("world");
    /// assert_eq!(matcher.find("hello world"), Some(6));
    /// assert_eq!(matcher.find("hello"), None);
    /// ```
    pub fn find(&self, text: &str) -> Option<usize> {
        let n = text.len();
        let m = self.pattern.len();

        if m == 0 || n < m {
            return None;
        }

        let text_bytes = text.as_bytes();
        let pattern_bytes = self.pattern.as_bytes();

        let mut i = 0; // Index for text
        let mut j = 0; // Index for pattern

        while i < n {
            if pattern_bytes[j] == text_bytes[i] {
                i += 1;
                j += 1;

                if j == m {
                    // Found complete pattern match
                    return Some(i - j);
                }
            } else if j != 0 {
                // Use failure function to skip ahead
                j = self.failure[j - 1];
            } else {
                // No match at all, move to next character
                i += 1;
            }
        }

        None
    }

    /// Find all occurrences of the pattern in text.
    ///
    /// Returns all positions where the pattern appears in the text.
    ///
    /// # Arguments
    /// * `text` - The text to search in
    ///
    /// # Returns
    /// A vector of positions where the pattern was found
    ///
    /// # Example
    /// ```ignore
    /// let matcher = KMPMatcher::new("ab");
    /// let positions = matcher.find_all("ababab");
    /// assert_eq!(positions, vec![0, 2, 4]);
    /// ```
    pub fn find_all(&self, text: &str) -> Vec<usize> {
        let mut positions = Vec::new();
        let n = text.len();
        let m = self.pattern.len();

        if m == 0 || n < m {
            return positions;
        }

        let text_bytes = text.as_bytes();
        let pattern_bytes = self.pattern.as_bytes();

        let mut i = 0; // Index for text
        let mut j = 0; // Index for pattern

        while i < n {
            if pattern_bytes[j] == text_bytes[i] {
                i += 1;
                j += 1;

                if j == m {
                    // Found complete pattern match
                    positions.push(i - j);
                    j = self.failure[j - 1];
                }
            } else if j != 0 {
                j = self.failure[j - 1];
            } else {
                i += 1;
            }
        }

        positions
    }

    /// Get the pattern length.
    pub const fn pattern_len(&self) -> usize {
        self.pattern.len()
    }

    /// Check if the pattern is empty.
    pub const fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }
}

/// Delta deduplicator using rolling hash and KMP.
///
/// Orchestrates the two-phase deduplication approach:
/// 1. Rolling hash for fast filtering
/// 2. KMP for exact verification
///
/// # Example
///
/// ```ignore
/// let mut dedup = DeltaDeduplicator::new();
///
/// // Add accumulated content
/// dedup.add_accumulated("Hello World");
///
/// // Check if a delta is a duplicate
/// if let Some(new_portion) = dedup.extract_new_content("Hello World!") {
///     // "!" is the new portion
///     assert_eq!(new_portion, "!");
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct DeltaDeduplicator {
    /// Rolling hash window for accumulated content
    hash_window: RollingHashWindow,
}

impl DeltaDeduplicator {
    /// Create a new delta deduplicator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add accumulated content for deduplication tracking.
    ///
    /// # Arguments
    /// * `content` - The accumulated content to track
    pub fn add_accumulated(&mut self, content: &str) {
        self.hash_window.add_content(content);
    }

    /// Extract new content from a potential snapshot.
    ///
    /// Uses rolling hash and KMP to detect if the incoming delta contains
    /// previously accumulated content (a snapshot) and extracts only the
    /// new portion.
    ///
    /// # Two-Phase Algorithm
    ///
    /// 1. **Rolling Hash Filter**: Compute hash of delta and check if it exists
    ///    in any window of the accumulated content. O(n) time.
    ///
    /// 2. **KMP Verification**: If hash matches, use KMP to verify actual
    ///    substring match and find the exact position. O(n+m) time.
    ///
    /// # Arguments
    /// * `delta` - The incoming delta to check
    /// * `accumulated` - The previously accumulated content
    ///
    /// # Returns
    /// * `Some(new_portion)` - The delta starts with accumulated, returns new portion only
    /// * `None` - The delta is genuinely new, doesn't start with accumulated
    ///
    /// # Example
    /// ```ignore
    /// let mut dedup = DeltaDeduplicator::new();
    /// dedup.add_accumulated("Hello");
    ///
    /// // Snapshot: "Hello World"
    /// assert_eq!(
    ///     DeltaDeduplicator::extract_new_content("Hello World", "Hello"),
    ///     Some(" World")
    /// );
    ///
    /// // Genuine delta: " World"
    /// assert_eq!(
    ///     DeltaDeduplicator::extract_new_content(" World", "Hello"),
    ///     None
    /// );
    /// ```
    pub fn extract_new_content<'a>(delta: &'a str, accumulated: &str) -> Option<&'a str> {
        // Fast rejection: delta must be longer than accumulated
        if delta.len() <= accumulated.len() {
            return None;
        }

        // Phase 1: Rolling hash check
        let accumulated_hash = RollingHashWindow::compute_hash(accumulated);
        let delta_prefix_hash = RollingHashWindow::compute_hash(&delta[..accumulated.len()]);

        // If hashes don't match, definitely not a snapshot
        if accumulated_hash != delta_prefix_hash {
            return None;
        }

        // Phase 2: KMP verification for exact match
        let kmp = KMPMatcher::new(accumulated);
        if let Some(pos) = kmp.find(delta) {
            // Verify it's at position 0 (snapshot starts with accumulated)
            if pos == 0 {
                return Some(&delta[accumulated.len()..]);
            }
        }

        // Hash collision or match not at start - not a snapshot
        None
    }

    /// Check if delta is likely a snapshot using rolling hash only.
    ///
    /// This is a faster O(n) check that may have false positives due to
    /// hash collisions. Use `extract_new_content` for verified results.
    ///
    /// # Arguments
    /// * `delta` - The incoming delta to check
    /// * `accumulated` - The previously accumulated content
    ///
    /// # Returns
    /// * `true` - Delta may be a snapshot (hash matches)
    /// * `false` - Delta is definitely not a snapshot (hash doesn't match)
    pub fn is_likely_snapshot(delta: &str, accumulated: &str) -> bool {
        if delta.len() <= accumulated.len() {
            return false;
        }

        let accumulated_hash = RollingHashWindow::compute_hash(accumulated);
        let delta_prefix_hash = RollingHashWindow::compute_hash(&delta[..accumulated.len()]);

        accumulated_hash == delta_prefix_hash
    }

    /// Clear all tracked content.
    pub fn clear(&mut self) {
        self.hash_window.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for RollingHashWindow

    #[test]
    fn test_rolling_hash_compute_hash() {
        let hash1 = RollingHashWindow::compute_hash("Hello");
        let hash2 = RollingHashWindow::compute_hash("Hello");
        let hash3 = RollingHashWindow::compute_hash("World");

        // Same input produces same hash
        assert_eq!(hash1, hash2);
        // Different input likely produces different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_rolling_hash_add_content() {
        let mut window = RollingHashWindow::new();
        assert!(window.is_empty());

        window.add_content("Hello");
        assert_eq!(window.len(), 5);
        assert!(!window.is_empty());
    }

    #[test]
    fn test_rolling_hash_get_window_hashes() {
        let mut window = RollingHashWindow::new();
        window.add_content("HelloWorld");

        // Get hashes for 5-character windows
        let hashes = window.get_window_hashes(5);
        assert_eq!(hashes.len(), 6); // "Hello", "elloW", "lloWo", "loWor", "oWorl", "World"

        // Verify positions
        assert_eq!(hashes[0].0, 0); // First window starts at 0
        assert_eq!(hashes[5].0, 5); // Last window starts at 5
    }

    #[test]
    fn test_rolling_hash_contains_hash() {
        let mut window = RollingHashWindow::new();
        window.add_content("HelloWorld");

        let world_hash = RollingHashWindow::compute_hash("World");
        let xyz_hash = RollingHashWindow::compute_hash("XYZ");

        // "World" exists in the content
        assert!(window.contains_hash(world_hash, 5).is_some());
        // "XYZ" doesn't exist
        assert!(window.contains_hash(xyz_hash, 3).is_none());
    }

    #[test]
    fn test_rolling_hash_clear() {
        let mut window = RollingHashWindow::new();
        window.add_content("Hello");
        assert_eq!(window.len(), 5);

        window.clear();
        assert!(window.is_empty());
    }

    // Tests for KMPMatcher

    #[test]
    fn test_kmp_find_pattern_exists() {
        let kmp = KMPMatcher::new("World");
        assert_eq!(kmp.find("Hello World"), Some(6));
        assert_eq!(kmp.find("WorldHello"), Some(0));
    }

    #[test]
    fn test_kmp_find_pattern_not_exists() {
        let kmp = KMPMatcher::new("XYZ");
        assert_eq!(kmp.find("Hello World"), None);
    }

    #[test]
    fn test_kmp_find_pattern_empty() {
        let kmp = KMPMatcher::new("");
        assert_eq!(kmp.find("Hello"), None);
    }

    #[test]
    fn test_kmp_find_text_shorter_than_pattern() {
        let kmp = KMPMatcher::new("Hello World");
        assert_eq!(kmp.find("Hello"), None);
    }

    #[test]
    fn test_kmp_find_all() {
        let kmp = KMPMatcher::new("ab");
        let positions = kmp.find_all("ababab");
        assert_eq!(positions, vec![0, 2, 4]);
    }

    #[test]
    fn test_kmp_find_all_no_matches() {
        let kmp = KMPMatcher::new("xyz");
        let positions = kmp.find_all("abcabc");
        assert!(positions.is_empty());
    }

    #[test]
    fn test_kmp_find_overlapping_patterns() {
        let kmp = KMPMatcher::new("aa");
        let positions = kmp.find_all("aaa");
        assert_eq!(positions, vec![0, 1]);
    }

    #[test]
    fn test_kmp_failure_function() {
        let kmp = KMPMatcher::new("abab");
        // lps = [0, 0, 1, 2]
        assert_eq!(kmp.failure, vec![0, 0, 1, 2]);
    }

    #[test]
    fn test_kmp_pattern_len() {
        let kmp = KMPMatcher::new("Hello");
        assert_eq!(kmp.pattern_len(), 5);
    }

    #[test]
    fn test_kmp_is_empty() {
        let kmp = KMPMatcher::new("");
        assert!(kmp.is_empty());

        let kmp = KMPMatcher::new("Hello");
        assert!(!kmp.is_empty());
    }

    // Tests for DeltaDeduplicator

    #[test]
    fn test_dedup_extract_new_content_snapshot() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello");

        // Snapshot: "Hello World"
        assert_eq!(
            DeltaDeduplicator::extract_new_content("Hello World", "Hello"),
            Some(" World")
        );
    }

    #[test]
    fn test_dedup_extract_new_content_genuine_delta() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello");

        // Genuine delta: " World"
        assert_eq!(
            DeltaDeduplicator::extract_new_content(" World", "Hello"),
            None
        );
    }

    #[test]
    fn test_dedup_extract_new_content_shorter_delta() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello World");

        // Delta shorter than accumulated - can't be snapshot
        assert_eq!(
            DeltaDeduplicator::extract_new_content("Hello", "Hello World"),
            None
        );
    }

    #[test]
    fn test_dedup_extract_new_content_equal_length() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello");

        // Equal length - even if identical, there's no "new" portion
        assert_eq!(
            DeltaDeduplicator::extract_new_content("Hello", "Hello"),
            None
        );
    }

    #[test]
    fn test_dedup_extract_new_content_no_match() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello");

        // Different content entirely
        assert_eq!(
            DeltaDeduplicator::extract_new_content("World", "Hello"),
            None
        );
    }

    #[test]
    fn test_dedup_is_likely_snapshot() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello");

        // Actual snapshot
        assert!(DeltaDeduplicator::is_likely_snapshot(
            "Hello World",
            "Hello"
        ));

        // Not a snapshot
        assert!(!DeltaDeduplicator::is_likely_snapshot(" World", "Hello"));
    }

    #[test]
    fn test_dedup_clear() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello");
        assert!(!dedup.hash_window.is_empty());

        dedup.clear();
        assert!(dedup.hash_window.is_empty());
    }

    // Integration tests

    #[test]
    fn test_dedup_two_phase_algorithm() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("The quick brown fox");

        // Phase 1: Rolling hash will match
        assert!(DeltaDeduplicator::is_likely_snapshot(
            "The quick brown fox jumps",
            "The quick brown fox"
        ));

        // Phase 2: KMP verification confirms and extracts new portion
        assert_eq!(
            DeltaDeduplicator::extract_new_content(
                "The quick brown fox jumps",
                "The quick brown fox"
            ),
            Some(" jumps")
        );
    }

    #[test]
    fn test_dedup_handles_unicode() {
        let mut dedup = DeltaDeduplicator::new();
        dedup.add_accumulated("Hello 世界");

        // Should handle UTF-8 correctly
        assert_eq!(
            DeltaDeduplicator::extract_new_content("Hello 世界!", "Hello 世界"),
            Some("!")
        );
    }

    #[test]
    fn test_dedup_empty_accumulated() {
        // No accumulated content

        // Any delta is genuine
        assert_eq!(DeltaDeduplicator::extract_new_content("Hello", ""), None);
    }
}
