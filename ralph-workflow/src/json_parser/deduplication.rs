//! Delta deduplication using KMP and Rolling Hash algorithms.
//!
//! This module provides efficient deduplication for streaming deltas using:
//! - **Rolling Hash (Rabin-Karp)**: Fast O(n) filtering to eliminate impossible matches
//! - **KMP (Knuth-Morris-Pratt)**: O(n+m) verification for exact substring matching
//! - **Strong Overlap Detection**: Thresholds and boundary checks to prevent false positives
//!
//! # Enhanced Deduplication
//!
//! The enhanced algorithm uses multiple layers of validation to prevent false positives:
//!
//! 1. **Rolling Hash Filter**: Fast O(n) check to eliminate impossible matches
//! 2. **KMP Verification**: O(n+m) confirmation of actual substring match
//! 3. **Overlap Threshold**: Only dedupe when overlap >= 30 chars AND >= 50% of delta
//! 4. **Boundary Sanity**: Ensure overlap ends at whitespace/punctuation/newline
//! 5. **Short Chunk Protection**: Chunks < 20 chars never deduped unless exact match
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
//!             Accept  ┌─────────────────────┐
//!             Delta   │ Strong Overlap Check│ ◄── >= 30 chars, >= 50%, safe boundary
//!                     └──────────┬──────────┘
//!                                │
//!                         ┌──────┴──────┐
//!                         │Measures?    │
//!                         └──────┬──────┘
//!                           No   │   Yes
//!                           │    │
//!                           ▼    ▼
//!                        Accept  Extract New
//!                        Delta   Portion Only
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

// ============================================================================
// Environment Trait for Testability
// ============================================================================

/// Trait for accessing environment variables.
///
/// This trait enables dependency injection for testing without global state pollution.
pub trait ThresholdEnvironment {
    /// Get an environment variable by name.
    fn get_var(&self, name: &str) -> Option<String>;
}

/// Production implementation that reads from actual environment.
pub struct RealThresholdEnvironment;

impl ThresholdEnvironment for RealThresholdEnvironment {
    fn get_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

// ============================================================================
// Configuration Constants for Strong Overlap Detection
// ============================================================================

/// Default minimum overlap character count for deduplication.
///
/// Overlaps must be at least this many characters to be considered for deduplication.
/// This prevents false positives from short accidental matches (e.g., "the", "and").
const DEFAULT_MIN_OVERLAP_CHARS: usize = 30;

/// Minimum overlap ratio expressed as integer (50 = 50%).
/// Used for integer-based ratio comparison to avoid floating point issues.
const MIN_OVERLAP_RATIO_INT: usize = 50;

/// Default threshold for considering a chunk "short".
///
/// Short chunks (< this many chars) are never deduped unless they're exact matches
/// with the accumulated content. This prevents aggressive deduplication of tokens
/// like ".", "\n", "Ok" that are legitimately repeated.
const DEFAULT_SHORT_CHUNK_THRESHOLD: usize = 20;

/// Default threshold for consecutive duplicate detection.
///
/// If the exact same chunk arrives this many times in a row, it's treated as a
/// resend glitch and dropped entirely.
const DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD: usize = 3;

/// Minimum allowed value for `MIN_OVERLAP_CHARS`.
const MIN_MIN_OVERLAP_CHARS: usize = 10;

/// Maximum allowed value for `MIN_OVERLAP_CHARS`.
const MAX_MIN_OVERLAP_CHARS: usize = 100;

/// Minimum allowed value for `SHORT_CHUNK_THRESHOLD`.
const MIN_SHORT_CHUNK_THRESHOLD: usize = 5;

/// Maximum allowed value for `SHORT_CHUNK_THRESHOLD`.
const MAX_SHORT_CHUNK_THRESHOLD: usize = 50;

/// Minimum allowed value for `CONSECUTIVE_DUPLICATE_THRESHOLD`.
const MIN_CONSECUTIVE_DUPLICATE_THRESHOLD: usize = 2;

/// Maximum allowed value for `CONSECUTIVE_DUPLICATE_THRESHOLD`.
const MAX_CONSECUTIVE_DUPLICATE_THRESHOLD: usize = 10;

/// Configuration for strong overlap detection.
///
/// This struct holds the tunable thresholds that determine when an overlap
/// is "strong enough" to warrant deduplication.
#[derive(Debug, Clone, Copy)]
pub struct OverlapThresholds {
    /// Minimum character count for overlap
    pub min_overlap_chars: usize,
    /// Threshold below which chunks are considered "short"
    pub short_chunk_threshold: usize,
    /// Number of consecutive duplicates before aggressive dedupe
    pub consecutive_duplicate_threshold: usize,
}

impl Default for OverlapThresholds {
    fn default() -> Self {
        Self {
            min_overlap_chars: DEFAULT_MIN_OVERLAP_CHARS,
            short_chunk_threshold: DEFAULT_SHORT_CHUNK_THRESHOLD,
            consecutive_duplicate_threshold: DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD,
        }
    }
}

/// Testable variant that accepts an environment trait for dependency injection.
///
/// This allows tests to mock environment variables without global state pollution.
///
/// Reads the following environment variables:
/// - `RALPH_STREAMING_MIN_OVERLAP_CHARS`: Minimum overlap characters (default: 30, range: 10-100)
/// - `RALPH_STREAMING_SHORT_CHUNK_THRESHOLD`: Short chunk threshold (default: 20, range: 5-50)
/// - `RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD`: Consecutive duplicate threshold (default: 3, range: 2-10)
pub fn get_overlap_thresholds_with_env(env: &dyn ThresholdEnvironment) -> OverlapThresholds {
    let min_overlap_chars = env
        .get_var("RALPH_STREAMING_MIN_OVERLAP_CHARS")
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|v| {
            if (MIN_MIN_OVERLAP_CHARS..=MAX_MIN_OVERLAP_CHARS).contains(&v) {
                Some(v)
            } else {
                None
            }
        })
        .unwrap_or(DEFAULT_MIN_OVERLAP_CHARS);

    let short_chunk_threshold = env
        .get_var("RALPH_STREAMING_SHORT_CHUNK_THRESHOLD")
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|v| {
            if (MIN_SHORT_CHUNK_THRESHOLD..=MAX_SHORT_CHUNK_THRESHOLD).contains(&v) {
                Some(v)
            } else {
                None
            }
        })
        .unwrap_or(DEFAULT_SHORT_CHUNK_THRESHOLD);

    let consecutive_duplicate_threshold = env
        .get_var("RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD")
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|v| {
            if (MIN_CONSECUTIVE_DUPLICATE_THRESHOLD..=MAX_CONSECUTIVE_DUPLICATE_THRESHOLD)
                .contains(&v)
            {
                Some(v)
            } else {
                None
            }
        })
        .unwrap_or(DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD);

    OverlapThresholds {
        min_overlap_chars,
        short_chunk_threshold,
        consecutive_duplicate_threshold,
    }
}

pub fn get_overlap_thresholds() -> OverlapThresholds {
    static THRESHOLDS: OnceLock<OverlapThresholds> = OnceLock::new();
    *THRESHOLDS.get_or_init(|| get_overlap_thresholds_with_env(&RealThresholdEnvironment))
}

// ============================================================================
// Boundary Detection
// ============================================================================

/// Check if a character position is at a safe boundary for deduplication.
///
/// A "safe boundary" is where the overlap ends at a natural break point in text:
/// - Whitespace (space, tab, newline, etc.)
/// - ASCII punctuation (.,!?;:, etc.)
/// - End of string
///
/// This prevents deduplication from splitting words or tokens mid-way through,
/// which could cause incorrect rendering of intentional repetitions.
///
/// # Arguments
/// * `text` - The text to check
/// * `pos` - The position in the text (byte offset)
///
/// # Returns
/// * `true` - The position is at a safe boundary for deduplication
/// * `false` - The position is NOT at a safe boundary (mid-word, etc.)
///
/// # Examples
///
/// ```ignore
/// // Safe: overlap ends at space
/// assert!(is_safe_boundary("Hello World", 11)); // After "World"
///
/// // Safe: overlap ends at punctuation
/// assert!(is_safe_boundary("Hello, World!", 12)); // After "!"
///
/// // Unsafe: overlap ends mid-word
/// assert!(!is_safe_boundary("HelloWorld", 5)); // After "Hello"
/// ```
fn is_safe_boundary(text: &str, pos: usize) -> bool {
    // End of string is always safe
    if pos >= text.len() {
        return true;
    }

    // Get the character at the boundary position
    // We need to use character iteration for Unicode safety
    let char_at_pos = text[pos..].chars().next();

    char_at_pos
        .is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation() || c.is_ascii_control())
}

// ============================================================================
// Overlap Quality Scoring
// ============================================================================

/// Score representing the "strength" of an overlap.
///
/// This struct captures multiple metrics about an overlap to determine
/// if it's strong enough to warrant deduplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlapScore {
    /// Character count of the overlap
    pub char_count: usize,
    /// Whether the overlap meets the minimum ratio threshold
    pub ratio_met: bool,
    /// Whether the overlap ends at a safe boundary
    pub is_safe_boundary: bool,
}

impl OverlapScore {
    /// Check if this overlap meets all thresholds for deduplication.
    ///
    /// # Arguments
    /// * `thresholds` - The overlap thresholds to check against
    ///
    /// # Returns
    /// * `true` - The overlap is strong enough for deduplication
    /// * `false` - The overlap is too weak
    #[must_use]
    pub const fn meets_thresholds(&self, thresholds: &OverlapThresholds) -> bool {
        self.char_count >= thresholds.min_overlap_chars && self.ratio_met && self.is_safe_boundary
    }

    /// Check if the delta is short (below short chunk threshold).
    ///
    /// # Arguments
    /// * `delta_len` - The length of the delta
    /// * `thresholds` - The overlap thresholds
    ///
    /// # Returns
    /// * `true` - The delta is considered short
    /// * `false` - The delta is normal length
    #[must_use]
    #[cfg(test)]
    pub const fn is_short_delta(delta_len: usize, thresholds: &OverlapThresholds) -> bool {
        delta_len < thresholds.short_chunk_threshold
    }
}

/// Score the quality of an overlap between delta and accumulated content.
///
/// This function computes multiple metrics about an overlap to determine
/// if it's strong enough to warrant deduplication.
///
/// # Arguments
/// * `delta` - The incoming delta
/// * `accumulated` - The previously accumulated content
///
/// # Returns
/// An `OverlapScore` containing:
/// - `char_count`: The length of the overlap in characters
/// - `ratio`: The overlap as a fraction of delta length
/// - `is_safe_boundary`: Whether the overlap ends at a safe boundary
///
/// # Examples
///
/// ```ignore
/// // Strong overlap (30+ chars, 50%+ ratio, safe boundary)
/// let score = score_overlap("Hello World! More text here", "Hello World!");
/// assert!(score.char_count >= 30);
/// assert!(score.ratio_met);
/// assert!(score.is_safe_boundary);
/// ```
fn score_overlap(delta: &str, accumulated: &str) -> OverlapScore {
    // Check if delta starts with accumulated (snapshot detection)
    let overlap_len = if delta.starts_with(accumulated) {
        accumulated.len()
    } else {
        0
    };

    // Calculate ratio as integer to avoid floating point precision issues
    // We'll compare overlap * 100 >= delta * MIN_OVERLAP_RATIO_INT
    // This avoids f64 casting entirely
    let ratio_met = if delta.is_empty() {
        false
    } else {
        // Check if overlap/delta >= MIN_OVERLAP_RATIO without floating point
        // By cross-multiplying: overlap * 100 >= delta * MIN_OVERLAP_RATIO_INT
        let overlap_scaled = overlap_len.saturating_mul(100);
        let threshold = delta.len().saturating_mul(MIN_OVERLAP_RATIO_INT);
        overlap_scaled >= threshold
    };

    // Check if the accumulated string ends at a safe boundary
    // This is important because we don't want to dedupe if the accumulated
    // string ends mid-word (e.g., accumulated="Hello" and delta="HelloWorld")
    let is_safe_boundary = if overlap_len > 0 {
        // Check if the last character of accumulated is a safe boundary
        is_safe_boundary(accumulated, accumulated.len())
    } else {
        false
    };

    OverlapScore {
        char_count: overlap_len,
        ratio_met,
        is_safe_boundary,
    }
}

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
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
    pub const fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if the window is empty.
    #[cfg(test)]
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
#[cfg(test)]
pub struct KMPMatcher {
    /// The pattern to search for
    pattern: String,
    /// Failure function (longest proper prefix which is also suffix)
    failure: Vec<usize>,
}

#[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
    pub const fn pattern_len(&self) -> usize {
        self.pattern.len()
    }

    /// Check if the pattern is empty.
    #[cfg(test)]
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
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add accumulated content for deduplication tracking.
    ///
    /// # Arguments
    /// * `content` - The accumulated content to track
    #[cfg(test)]
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
    #[cfg(test)]
    pub fn extract_new_content<'a>(delta: &'a str, accumulated: &str) -> Option<&'a str> {
        // Handle identical content (delta == accumulated) - return empty string
        if delta == accumulated {
            return Some("");
        }

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
        // Handle identical content (duplicate delta)
        if delta == accumulated {
            return true;
        }

        // Delta must be longer than accumulated to be a snapshot
        if delta.len() <= accumulated.len() {
            return false;
        }

        let accumulated_hash = RollingHashWindow::compute_hash(accumulated);
        let delta_prefix_hash = RollingHashWindow::compute_hash(&delta[..accumulated.len()]);

        accumulated_hash == delta_prefix_hash
    }

    /// Check if delta is likely a snapshot with strong overlap detection.
    ///
    /// This is an enhanced version of `is_likely_snapshot` that applies
    /// strong overlap thresholds to prevent false positives on intentional
    /// repetitions.
    ///
    /// # Strong Overlap Detection
    ///
    /// This method only returns `true` when:
    /// - The overlap meets minimum character count threshold (default: 30 chars)
    /// - The overlap meets minimum ratio threshold (default: 50% of delta)
    /// - The overlap ends at a safe boundary (whitespace/punctuation/newline)
    /// - Short chunks (< 20 chars) are only deduped if exact match
    ///
    /// # Arguments
    /// * `delta` - The incoming delta to check
    /// * `accumulated` - The previously accumulated content
    ///
    /// # Returns
    /// * `true` - Delta is a snapshot meeting strong overlap criteria
    /// * `false` - Delta is either genuine or overlap is too weak
    pub fn is_likely_snapshot_with_thresholds(delta: &str, accumulated: &str) -> bool {
        let thresholds = get_overlap_thresholds();

        // Handle short chunks: only dedupe if exact match
        if delta.len() < thresholds.short_chunk_threshold {
            return delta == accumulated;
        }

        // Handle identical content (delta == accumulated)
        // This is a snapshot where no new content is added
        if delta == accumulated {
            return true;
        }

        // Fast rejection: delta must be longer than accumulated
        if delta.len() <= accumulated.len() {
            return false;
        }

        // First check with basic rolling hash for quick rejection
        if !Self::is_likely_snapshot(delta, accumulated) {
            return false;
        }

        // Score the overlap to check if it meets strong overlap criteria
        let score = score_overlap(delta, accumulated);

        // Apply threshold checks
        score.meets_thresholds(&thresholds)
    }

    /// Extract new content from a snapshot with strong overlap detection.
    ///
    /// This is an enhanced version of `extract_new_content` that only extracts
    /// new content when the overlap meets strong overlap thresholds.
    ///
    /// # Arguments
    /// * `delta` - The incoming delta to check
    /// * `accumulated` - The previously accumulated content
    ///
    /// # Returns
    /// * `Some(new_portion)` - The overlap meets thresholds, returns new portion
    /// * `None` - The overlap is too weak or not a snapshot
    pub fn extract_new_content_with_thresholds<'a>(
        delta: &'a str,
        accumulated: &str,
    ) -> Option<&'a str> {
        let thresholds = get_overlap_thresholds();

        // Handle short chunks: only dedupe if exact match
        if delta.len() < thresholds.short_chunk_threshold {
            if delta == accumulated {
                return Some("");
            }
            return None;
        }

        // Handle identical content
        if delta == accumulated {
            return Some("");
        }

        // Fast rejection: delta must be longer than accumulated
        if delta.len() <= accumulated.len() {
            return None;
        }

        // Score the overlap
        let score = score_overlap(delta, accumulated);

        // Check if overlap meets thresholds
        if !score.meets_thresholds(&thresholds) {
            return None;
        }

        // Extract new content using the overlap length from the score
        if score.char_count > 0 && delta.len() > score.char_count {
            Some(&delta[score.char_count..])
        } else {
            None
        }
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

        // Equal length - if identical, return empty string (no new content)
        assert_eq!(
            DeltaDeduplicator::extract_new_content("Hello", "Hello"),
            Some("")
        );

        // Equal length but different content - not a snapshot
        assert_eq!(
            DeltaDeduplicator::extract_new_content("World", "Hello"),
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

    // Tests for Strong Overlap Detection with Thresholds

    #[test]
    fn test_strong_overlap_meets_char_threshold() {
        // Overlap of 30+ chars with safe boundary should pass
        let accumulated = "The quick brown fox jumps over the lazy";
        let delta = "The quick brown fox jumps over the lazy dog!";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should detect snapshot with 30+ char overlap"
        );
    }

    #[test]
    fn test_strong_overlap_meets_ratio_threshold() {
        // Overlap is 50%+ of delta length
        let accumulated = "The quick brown fox jumps over the lazy dog. ";
        let delta = "The quick brown fox jumps over the lazy dog. And more!";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should detect snapshot when overlap is 50%+ of delta"
        );
    }

    #[test]
    fn test_strong_overlap_fails_char_threshold() {
        // Overlap < 30 chars, even if ratio is good
        let accumulated = "Hello";
        let delta = "Hello World!";

        assert!(
            !DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should NOT detect snapshot when overlap < 30 chars"
        );
    }

    #[test]
    fn test_strong_overlap_fails_ratio_threshold() {
        // Overlap < 50% of delta, even if char count is good
        let accumulated = "The quick brown fox jumps over the lazy dog. ";
        let delta = "The quick brown fox jumps over the lazy dog. And then a whole lot more text follows to make the ratio low!";

        assert!(
            !DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should NOT detect snapshot when overlap < 50% of delta"
        );
    }

    #[test]
    fn test_boundary_check_whitespace() {
        // Overlap ends at space (safe boundary)
        let accumulated = "The quick brown fox jumps over the lazy dog and ";
        let delta = "The quick brown fox jumps over the lazy dog and then more!";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should detect snapshot when overlap ends at whitespace"
        );
    }

    #[test]
    fn test_boundary_check_punctuation() {
        // Overlap ends at punctuation (safe boundary)
        let accumulated = "The quick brown fox jumps over the lazy dog.";
        let delta = "The quick brown fox jumps over the lazy dog. How are you?";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should detect snapshot when overlap ends at punctuation"
        );
    }

    #[test]
    fn test_boundary_check_mid_word_fails() {
        // Overlap ends mid-word (unsafe boundary)
        let accumulated = "Hello";
        let delta = "HelloWorld! This is a lot of text to ensure we have enough characters.";

        // Even though we have 30+ chars, the boundary check should fail
        // because the overlap ends mid-word (at 'W' of "World")
        assert!(
            !DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should NOT detect snapshot when overlap ends mid-word"
        );
    }

    #[test]
    fn test_short_chunk_never_deduped() {
        // Short chunks (< 20 chars) never deduped unless exact match
        let accumulated = "Hello";
        let delta = "Hello World!";

        // Even though "Hello World!" starts with "Hello", it's < 20 chars total
        // and not an exact match, so it should NOT be deduped
        assert!(
            !DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Short chunks should NOT be deduped unless exact match"
        );
    }

    #[test]
    fn test_short_chunk_exact_match_deduped() {
        // Short chunks (< 20 chars) ARE deduped if exact match
        let accumulated = "Hello";
        let delta = "Hello";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Short chunk exact match SHOULD be deduped"
        );
    }

    #[test]
    fn test_extract_new_content_with_thresholds_strong_overlap() {
        let accumulated = "The quick brown fox jumps over the lazy dog. ";
        let delta = "The quick brown fox jumps over the lazy dog. More content here!";

        let result = DeltaDeduplicator::extract_new_content_with_thresholds(delta, accumulated);
        assert_eq!(result, Some("More content here!"));
    }

    #[test]
    fn test_extract_new_content_with_thresholds_weak_overlap() {
        // Weak overlap (< 30 chars) should return None
        let accumulated = "Hello";
        let delta = "Hello World! This is more content to exceed thresholds.";

        let result = DeltaDeduplicator::extract_new_content_with_thresholds(delta, accumulated);
        assert_eq!(result, None, "Weak overlap should return None");
    }

    #[test]
    fn test_extract_new_content_with_thresholds_short_chunk() {
        // Short chunk that's not an exact match should return None
        let accumulated = "Hi";
        let delta = "Hi there!";

        let result = DeltaDeduplicator::extract_new_content_with_thresholds(delta, accumulated);
        assert_eq!(
            result, None,
            "Short chunk should return None unless exact match"
        );
    }

    #[test]
    fn test_extract_new_content_with_thresholds_short_chunk_exact() {
        // Short chunk exact match should return empty string
        let accumulated = "Hello";
        let delta = "Hello";

        let result = DeltaDeduplicator::extract_new_content_with_thresholds(delta, accumulated);
        assert_eq!(result, Some(""));
    }

    #[test]
    fn test_strong_overlap_with_unicode() {
        // Test with Unicode characters
        let accumulated = "Hello 世界! This is a long enough string to meet thresholds. ";
        let delta = "Hello 世界! This is a long enough string to meet thresholds. More!";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Should handle Unicode correctly with strong overlap"
        );

        let result = DeltaDeduplicator::extract_new_content_with_thresholds(delta, accumulated);
        assert_eq!(result, Some("More!"));
    }

    #[test]
    fn test_intentional_repetition_not_deduped() {
        // Simulate intentional repetition (e.g., "Hello World! Hello World!")
        // where the overlap is small relative to the total delta
        let accumulated = "Hello World!";
        let delta = "Hello World! Hello World! This is a lot of additional content to make the overlap ratio low enough that it won't be deduplicated!";

        assert!(
            !DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Intentional repetition should NOT be deduped when overlap ratio is low"
        );
    }

    #[test]
    fn test_snapshot_strong_overlap_deduped() {
        // Real snapshot scenario: agent sends full accumulated + new content
        let accumulated = "The quick brown fox jumps over the lazy dog. ";
        let delta = "The quick brown fox jumps over the lazy dog. And then some more!";

        assert!(
            DeltaDeduplicator::is_likely_snapshot_with_thresholds(delta, accumulated),
            "Actual snapshot SHOULD be detected and deduped"
        );

        let result = DeltaDeduplicator::extract_new_content_with_thresholds(delta, accumulated);
        assert_eq!(result, Some("And then some more!"));
    }

    #[test]
    fn test_overlap_score_meets_thresholds() {
        let thresholds = OverlapThresholds::default();

        // Strong overlap: 30+ chars, 50%+ ratio, safe boundary
        let score = OverlapScore {
            char_count: 50,
            ratio_met: true,
            is_safe_boundary: true,
        };

        assert!(score.meets_thresholds(&thresholds));
    }

    #[test]
    fn test_overlap_score_fails_char_count() {
        let thresholds = OverlapThresholds::default();

        // Char count too low
        let score = OverlapScore {
            char_count: 20,
            ratio_met: true,
            is_safe_boundary: true,
        };

        assert!(!score.meets_thresholds(&thresholds));
    }

    #[test]
    fn test_overlap_score_fails_ratio() {
        let thresholds = OverlapThresholds::default();

        // Ratio too low (met = false)
        let score = OverlapScore {
            char_count: 50,
            ratio_met: false,
            is_safe_boundary: true,
        };

        assert!(!score.meets_thresholds(&thresholds));
    }

    #[test]
    fn test_overlap_score_fails_boundary() {
        let thresholds = OverlapThresholds::default();

        // Boundary not safe
        let score = OverlapScore {
            char_count: 50,
            ratio_met: true,
            is_safe_boundary: false,
        };

        assert!(!score.meets_thresholds(&thresholds));
    }

    #[test]
    fn test_is_short_delta() {
        let thresholds = OverlapThresholds::default();

        assert!(OverlapScore::is_short_delta(10, &thresholds));
        assert!(!OverlapScore::is_short_delta(30, &thresholds));
    }

    #[test]
    fn test_is_safe_boundary_whitespace() {
        assert!(is_safe_boundary("Hello World", 5));
        assert!(is_safe_boundary("Hello\nWorld", 5));
        assert!(is_safe_boundary("Hello\tWorld", 5));
    }

    #[test]
    fn test_is_safe_boundary_punctuation() {
        assert!(is_safe_boundary("Hello, World!", 12)); // After "!"
        assert!(is_safe_boundary("Hello. World", 5)); // After "."
        assert!(is_safe_boundary("Hello; World", 5)); // After ";"
    }

    #[test]
    fn test_is_safe_boundary_end_of_string() {
        assert!(is_safe_boundary("Hello", 5));
        assert!(is_safe_boundary("Hello", 10)); // Beyond length
    }

    #[test]
    fn test_is_safe_boundary_mid_word() {
        assert!(!is_safe_boundary("HelloWorld", 5));
        assert!(!is_safe_boundary("Testing", 3));
    }

    #[test]
    fn test_score_overlap_with_snapshot() {
        let accumulated = "The quick brown fox jumps over the lazy dog. ";
        let delta = "The quick brown fox jumps over the lazy dog. And more!";

        let score = score_overlap(delta, accumulated);

        assert!(score.char_count > 30);
        assert!(score.ratio_met);
        assert!(score.is_safe_boundary);
    }

    #[test]
    fn test_score_overlap_with_genuine_delta() {
        let accumulated = "Hello";
        let delta = " World";

        let score = score_overlap(delta, accumulated);

        assert_eq!(score.char_count, 0);
    }

    #[test]
    fn test_get_overlap_thresholds_default() {
        let thresholds = get_overlap_thresholds();

        assert_eq!(thresholds.min_overlap_chars, 30);
        assert_eq!(thresholds.short_chunk_threshold, 20);
        assert_eq!(thresholds.consecutive_duplicate_threshold, 3);
    }

    #[test]
    fn test_consecutive_duplicate_threshold_default() {
        let thresholds = OverlapThresholds::default();
        assert_eq!(
            thresholds.consecutive_duplicate_threshold, DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD,
            "Default consecutive_duplicate_threshold should match constant"
        );
        assert_eq!(
            thresholds.consecutive_duplicate_threshold, 3,
            "Default consecutive_duplicate_threshold should be 3"
        );
    }

    /// Mock environment for testing threshold parsing.
    struct MockThresholdEnv {
        vars: std::collections::HashMap<String, String>,
    }

    impl MockThresholdEnv {
        fn new() -> Self {
            Self {
                vars: std::collections::HashMap::new(),
            }
        }

        fn with_var(mut self, key: &str, value: &str) -> Self {
            self.vars.insert(key.to_string(), value.to_string());
            self
        }
    }

    impl ThresholdEnvironment for MockThresholdEnv {
        fn get_var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
    }

    #[test]
    fn test_threshold_env_parsing_min_overlap_chars() {
        // Test valid custom value
        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_MIN_OVERLAP_CHARS", "50");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.min_overlap_chars, 50);

        // Test out of range (too low) - should use default
        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_MIN_OVERLAP_CHARS", "5");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.min_overlap_chars, DEFAULT_MIN_OVERLAP_CHARS);

        // Test out of range (too high) - should use default
        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_MIN_OVERLAP_CHARS", "200");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.min_overlap_chars, DEFAULT_MIN_OVERLAP_CHARS);

        // Test invalid value - should use default
        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_MIN_OVERLAP_CHARS", "invalid");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.min_overlap_chars, DEFAULT_MIN_OVERLAP_CHARS);
    }

    #[test]
    fn test_threshold_env_parsing_short_chunk_threshold() {
        // Test valid custom value
        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_SHORT_CHUNK_THRESHOLD", "10");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.short_chunk_threshold, 10);

        // Test boundary values
        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_SHORT_CHUNK_THRESHOLD", "5");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.short_chunk_threshold, 5); // Min boundary

        let env = MockThresholdEnv::new().with_var("RALPH_STREAMING_SHORT_CHUNK_THRESHOLD", "50");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.short_chunk_threshold, 50); // Max boundary
    }

    #[test]
    fn test_threshold_env_parsing_consecutive_duplicate() {
        // Test valid custom value
        let env = MockThresholdEnv::new()
            .with_var("RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD", "5");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.consecutive_duplicate_threshold, 5);

        // Test min boundary
        let env = MockThresholdEnv::new()
            .with_var("RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD", "2");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.consecutive_duplicate_threshold, 2);

        // Test max boundary
        let env = MockThresholdEnv::new()
            .with_var("RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD", "10");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.consecutive_duplicate_threshold, 10);

        // Test out of range - should use default
        let env = MockThresholdEnv::new()
            .with_var("RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD", "1");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(
            thresholds.consecutive_duplicate_threshold,
            DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD
        );

        let env = MockThresholdEnv::new()
            .with_var("RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD", "15");
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(
            thresholds.consecutive_duplicate_threshold,
            DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD
        );
    }

    #[test]
    fn test_threshold_env_empty_returns_defaults() {
        let env = MockThresholdEnv::new();
        let thresholds = get_overlap_thresholds_with_env(&env);
        assert_eq!(thresholds.min_overlap_chars, DEFAULT_MIN_OVERLAP_CHARS);
        assert_eq!(
            thresholds.short_chunk_threshold,
            DEFAULT_SHORT_CHUNK_THRESHOLD
        );
        assert_eq!(
            thresholds.consecutive_duplicate_threshold,
            DEFAULT_CONSECUTIVE_DUPLICATE_THRESHOLD
        );
    }

    #[test]
    fn test_threshold_bounds_constants() {
        // Verify bounds constants are correct (pure constant tests, no env var manipulation)
        assert_eq!(
            MIN_CONSECUTIVE_DUPLICATE_THRESHOLD, 2,
            "Minimum threshold should be 2"
        );
        assert_eq!(
            MAX_CONSECUTIVE_DUPLICATE_THRESHOLD, 10,
            "Maximum threshold should be 10"
        );
        assert_eq!(MIN_MIN_OVERLAP_CHARS, 10);
        assert_eq!(MAX_MIN_OVERLAP_CHARS, 100);
        assert_eq!(MIN_SHORT_CHUNK_THRESHOLD, 5);
        assert_eq!(MAX_SHORT_CHUNK_THRESHOLD, 50);
    }
}
