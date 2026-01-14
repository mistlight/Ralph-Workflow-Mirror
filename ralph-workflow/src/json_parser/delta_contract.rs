//! Delta contract validation module.
//!
//! This module defines the contract that all streaming parsers must follow:
//! **delta-only content** - each streaming event must contain only newly
//! generated text, never the full accumulated content (snapshot).
//!
//! # The Delta Contract
//!
//! Treating snapshots as deltas causes exponential duplication bugs. When
//! an agent sends full accumulated content as if it were a "delta", the same
//! content gets displayed repeatedly, growing exponentially with each event.
//!
//! This module provides:
//! - `DeltaContract` trait for contract validation
//! - `Violation` type for clear violation reporting
//! - Validation functions that detect common violation patterns
//!
//! # Example
//!
//! ```ignore
//! use crate::json_parser::delta_contract::{DeltaContract, Violation};
//!
//! let previous = "Hello World";
//! let incoming = "Hello World! This is more text";
//!
//! if let Some(violation) = DeltaContract::validate_delta(previous, incoming) {
//!     eprintln!("Contract violation: {}", violation);
//! }
//! ```

use std::fmt;

/// A delta contract violation.
///
/// Represents different ways the delta contract can be violated.
#[derive(Debug, Clone, PartialEq)]
enum Violation {
    /// The incoming content is an exact snapshot (starts with previous and is longer).
    Snapshot {
        /// The previous accumulated content length
        previous_len: usize,
        /// The incoming content length
        incoming_len: usize,
    },

    /// The incoming content is a fuzzy snapshot (contains previous embedded).
    FuzzySnapshot {
        /// The previous accumulated content length
        previous_len: usize,
        /// The incoming content length
        incoming_len: usize,
        /// The overlap ratio (0.0 to 1.0)
        overlap_ratio: f64,
    },

    /// The incoming content is identical to previous (duplicate).
    Duplicate {
        /// The content length
        len: usize,
    },

    /// The incoming delta exceeds the reasonable size threshold.
    OversizedDelta {
        /// The delta size
        size: usize,
        /// The threshold that was exceeded
        threshold: usize,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::Snapshot {
                previous_len,
                incoming_len,
            } => {
                write!(
                    f,
                    "Snapshot-as-delta: incoming content ({incoming_len} chars) \
                    starts with previous content ({previous_len} chars). \
                    This indicates the agent sent full accumulated content instead of a delta."
                )
            }
            Violation::FuzzySnapshot {
                previous_len,
                incoming_len,
                overlap_ratio,
            } => {
                write!(
                    f,
                    "Fuzzy snapshot-as-delta: incoming content ({incoming_len} chars) \
                    contains previous content ({previous_len} chars) embedded with \
                    {ratio:.0}% overlap. This suggests the agent added a prefix or suffix \
                    to the accumulated content.",
                    ratio = overlap_ratio * 100.0
                )
            }
            Violation::Duplicate { len } => {
                write!(
                    f,
                    "Duplicate delta: incoming content ({len} chars) is identical \
                    to previous accumulated content. This is a no-op that should be ignored."
                )
            }
            Violation::OversizedDelta { size, threshold } => {
                write!(
                    f,
                    "Oversized delta: incoming content ({size} chars) exceeds \
                    threshold ({threshold} chars). This may indicate unusual streaming \
                    behavior or a partial snapshot."
                )
            }
        }
    }
}

impl std::error::Error for Violation {}

/// The threshold for considering a delta "oversized".
///
/// Deltas are expected to be small chunks (typically < 100 chars). If a single
/// "delta" exceeds this threshold, it may indicate a snapshot being treated
/// as a delta.
const SNAPSHOT_THRESHOLD: usize = 200;

/// The minimum length for fuzzy snapshot detection.
///
/// For very short previous content, we skip fuzzy matching to avoid false positives.
const FUZZY_MIN_LENGTH: usize = 20;

/// The overlap ratio threshold for fuzzy snapshot detection.
///
/// If the incoming text contains more than this ratio of the previous content,
/// it's considered a fuzzy snapshot.
const FUZZY_OVERLAP_THRESHOLD: f64 = 0.6;

/// Trait for delta contract validation.
///
/// This trait defines the contract that streaming parsers must follow.
/// Implementations provide validation logic to detect when agents are sending
/// snapshots instead of deltas.
trait DeltaContract {
    /// Validate whether incoming content violates the delta contract.
    ///
    /// # Arguments
    /// * `previous` - The previously accumulated content
    /// * `incoming` - The incoming content to validate
    ///
    /// # Returns
    /// * `None` - The content is a valid delta
    /// * `Some(Violation)` - The content violates the contract
    fn validate_delta(previous: &str, incoming: &str) -> Option<Violation>;

    /// Extract the delta portion from a snapshot.
    ///
    /// When a snapshot is detected, this method extracts only the new portion
    /// that hasn't been accumulated yet.
    ///
    /// # Arguments
    /// * `previous` - The previously accumulated content
    /// * `incoming` - The incoming snapshot (full accumulated + new content)
    ///
    /// # Returns
    /// * `Ok(delta)` - The delta portion (new content only)
    /// * `Err(msg)` - If the incoming content is not a valid snapshot
    fn extract_delta<'a>(&'a self, previous: &str, incoming: &'a str) -> Result<&'a str, String>;

    /// Check if incoming content is likely a snapshot using fuzzy matching.
    ///
    /// This handles cases where agents add prefixes or have minor whitespace
    /// differences before sending the accumulated content.
    ///
    /// # Arguments
    /// * `previous` - The previously accumulated content
    /// * `incoming` - The incoming content to check
    ///
    /// # Returns
    /// * `true` - The content appears to be a fuzzy snapshot
    /// * `false` - The content appears to be a genuine delta
    fn is_fuzzy_snapshot(previous: &str, incoming: &str) -> bool;

    /// Check if the content size indicates a fuzzy snapshot.
    ///
    /// Returns true if:
    /// - Previous content is long enough (>= FUZZY_MIN_LENGTH)
    /// - Previous is contained within incoming
    /// - Overlap ratio exceeds threshold
    fn meets_fuzzy_snapshot_criteria(previous: &str, incoming: &str) -> bool;
}

/// Default implementation of delta contract validation.
struct DefaultDeltaContract;

impl DeltaContract for DefaultDeltaContract {
    fn validate_delta(previous: &str, incoming: &str) -> Option<Violation> {
        // Check for exact snapshot (incoming starts with previous and is longer)
        // Only apply when we have actual previous content to avoid false positives on first delta
        if !previous.is_empty() && incoming.len() > previous.len() && incoming.starts_with(previous) {
            return Some(Violation::Snapshot {
                previous_len: previous.len(),
                incoming_len: incoming.len(),
            });
        }

        // Check for duplicate (identical to previous)
        if incoming == previous {
            return Some(Violation::Duplicate { len: incoming.len() });
        }

        // Check for fuzzy snapshot (previous embedded within incoming)
        if Self::is_fuzzy_snapshot(previous, incoming) {
            let overlap_ratio = previous.len() as f64 / incoming.len() as f64;
            return Some(Violation::FuzzySnapshot {
                previous_len: previous.len(),
                incoming_len: incoming.len(),
                overlap_ratio,
            });
        }

        // Check for oversized delta
        if incoming.len() > SNAPSHOT_THRESHOLD {
            return Some(Violation::OversizedDelta {
                size: incoming.len(),
                threshold: SNAPSHOT_THRESHOLD,
            });
        }

        // No violation detected
        None
    }

    fn extract_delta<'a>(&'a self, previous: &str, incoming: &'a str) -> Result<&'a str, String> {
        // Try exact match first (previous is at the start)
        if incoming.starts_with(previous) {
            return Ok(&incoming[previous.len()..]);
        }

        // Try fuzzy match (previous is contained somewhere in incoming)
        if let Some(pos) = incoming.find(previous) {
            let delta_start = pos + previous.len();
            return Ok(&incoming[delta_start..]);
        }

        // Not a valid snapshot
        Err(format!(
            "extract_delta called on non-snapshot content. \
            previous_len={}, incoming_len={}. \
            Snapshot detection may have had a false positive.",
            previous.len(),
            incoming.len()
        ))
    }

    fn is_fuzzy_snapshot(previous: &str, incoming: &str) -> bool {
        // Must meet size criteria first
        if !Self::meets_fuzzy_snapshot_criteria(previous, incoming) {
            return false;
        }

        // Check if previous is contained within incoming
        if incoming.contains(previous) {
            // Calculate overlap ratio
            let overlap_ratio = previous.len() as f64 / incoming.len() as f64;
            // If threshold is exceeded, it's a fuzzy snapshot
            overlap_ratio > FUZZY_OVERLAP_THRESHOLD
        } else {
            false
        }
    }

    fn meets_fuzzy_snapshot_criteria(previous: &str, incoming: &str) -> bool {
        // For very short previous content, skip fuzzy matching
        if previous.len() < FUZZY_MIN_LENGTH {
            return false;
        }

        // Previous must be contained within incoming for fuzzy matching
        incoming.contains(previous)
    }
}

/// Convenience function for validating a delta using the default contract.
///
/// This is a shorthand for `DefaultDeltaContract::validate_delta(previous, incoming)`.
fn validate_delta(previous: &str, incoming: &str) -> Option<Violation> {
    DefaultDeltaContract::validate_delta(previous, incoming)
}

/// Convenience function for extracting a delta from a snapshot.
///
/// This is a shorthand for `DefaultDeltaContract.extract_delta(previous, incoming)`.
fn extract_delta<'a>(previous: &str, incoming: &'a str) -> Result<&'a str, String> {
    DefaultDeltaContract.extract_delta(previous, incoming)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_delta_with_genuine_delta() {
        let previous = "Hello";
        let incoming = " World";

        let result = validate_delta(previous, incoming);
        assert!(
            result.is_none(),
            "Genuine delta should not be flagged as violation"
        );
    }

    #[test]
    fn test_validate_delta_detects_snapshot() {
        let previous = "Hello";
        let incoming = "Hello World";

        let result = validate_delta(previous, incoming);
        assert!(result.is_some(), "Should detect snapshot-as-delta");

        match result {
            Some(Violation::Snapshot { .. }) => {}
            _ => panic!("Expected Snapshot violation, got {:?}", result),
        }
    }

    #[test]
    fn test_validate_delta_detects_duplicate() {
        let previous = "Hello";
        let incoming = "Hello";

        let result = validate_delta(previous, incoming);
        assert!(result.is_some(), "Should detect duplicate");

        match result {
            Some(Violation::Duplicate { .. }) => {}
            _ => panic!("Expected Duplicate violation, got {:?}", result),
        }
    }

    #[test]
    fn test_validate_delta_detects_fuzzy_snapshot() {
        let previous = "Hello World! This is a test message that is long enough to trigger fuzzy matching";
        let incoming = "Response: Hello World! This is a test message that is long enough to trigger fuzzy matching and more";

        let result = validate_delta(previous, incoming);
        assert!(result.is_some(), "Should detect fuzzy snapshot");

        match result {
            Some(Violation::FuzzySnapshot { .. }) => {}
            _ => panic!("Expected FuzzySnapshot violation, got {:?}", result),
        }
    }

    #[test]
    fn test_validate_delta_oversized_delta() {
        let previous = "";
        let incoming = "x".repeat(SNAPSHOT_THRESHOLD + 1);

        let result = validate_delta(previous, &incoming);
        assert!(result.is_some(), "Should detect oversized delta");

        match result {
            Some(Violation::OversizedDelta { .. }) => {}
            _ => panic!("Expected OversizedDelta violation, got {:?}", result),
        }
    }

    #[test]
    fn test_extract_delta_exact_match() {
        let previous = "Hello";
        let incoming = "Hello World";

        let result = extract_delta(previous, incoming);
        assert_eq!(result.unwrap(), " World");
    }

    #[test]
    fn test_extract_delta_fuzzy_match() {
        let previous = "Hello World";
        let incoming = "Response: Hello World and more";

        let result = extract_delta(previous, incoming);
        assert!(result.unwrap().contains(" and more"));
    }

    #[test]
    fn test_extract_delta_empty_delta() {
        let previous = "Hello";
        let incoming = "Hello";

        let result = extract_delta(previous, incoming);
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_extract_delta_non_snapshot_returns_error() {
        let previous = "Hello";
        let incoming = "World";

        let result = extract_delta(previous, incoming);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_fuzzy_snapshot_with_prefix() {
        let previous = "Hello World! This is a test message that is long enough to trigger fuzzy matching";
        let incoming = "Response: Hello World! This is a test message that is long enough to trigger fuzzy matching and more";

        assert!(DefaultDeltaContract::is_fuzzy_snapshot(previous, incoming));
    }

    #[test]
    fn test_is_fuzzy_snapshot_no_false_positive() {
        let previous = "Hello";
        let incoming = "World!";

        assert!(!DefaultDeltaContract::is_fuzzy_snapshot(previous, incoming));
    }

    #[test]
    fn test_is_fuzzy_snapshot_requires_minimum_length() {
        let previous = "Hi there";
        let incoming = "Response: Hi there, how are you?";

        assert!(!DefaultDeltaContract::is_fuzzy_snapshot(previous, incoming));
    }

    #[test]
    fn test_meets_fuzzy_snapshot_criteria() {
        let long = "This is a moderately long message for testing fuzzy snapshot detection thresholds";
        let incoming = "Response: This is a moderately long message for testing fuzzy snapshot detection thresholds and more";

        assert!(DefaultDeltaContract::meets_fuzzy_snapshot_criteria(long, incoming));
    }

    #[test]
    fn test_meets_fuzzy_snapshot_criteria_short_content() {
        let short = "Hi there";
        let incoming = "Response: Hi there, how are you?";

        assert!(!DefaultDeltaContract::meets_fuzzy_snapshot_criteria(short, incoming));
    }

    #[test]
    fn test_violation_display() {
        let violation = Violation::Snapshot {
            previous_len: 5,
            incoming_len: 11,
        };

        let display = format!("{}", violation);
        assert!(display.contains("Snapshot-as-delta"));
        assert!(display.contains("11 chars"));
        assert!(display.contains("5 chars"));
    }

    #[test]
    fn test_violation_display_fuzzy() {
        let violation = Violation::FuzzySnapshot {
            previous_len: 80,
            incoming_len: 100,
            overlap_ratio: 0.8,
        };

        let display = format!("{}", violation);
        assert!(display.contains("Fuzzy snapshot-as-delta"));
        assert!(display.contains("80%"));
    }

    #[test]
    fn test_violation_display_duplicate() {
        let violation = Violation::Duplicate { len: 10 };

        let display = format!("{}", violation);
        assert!(display.contains("Duplicate delta"));
        assert!(display.contains("10 chars"));
    }

    #[test]
    fn test_violation_display_oversized() {
        let violation = Violation::OversizedDelta {
            size: 250,
            threshold: 200,
        };

        let display = format!("{}", violation);
        assert!(display.contains("Oversized delta"));
        assert!(display.contains("250 chars"));
        assert!(display.contains("200 chars"));
    }

    #[test]
    fn test_validate_delta_no_previous_content() {
        let previous = "";
        let incoming = "Hello World";

        let result = validate_delta(previous, incoming);
        // Should only flag as oversized, not as snapshot (no previous to compare)
        match result {
            Some(Violation::OversizedDelta { .. }) => {
                // Expected - content is oversized
            }
            None => {
                // Also acceptable - first delta may be large
            }
            _ => {
                panic!("Unexpected violation for first delta: {:?}", result);
            }
        }
    }
}
