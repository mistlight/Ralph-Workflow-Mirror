//! Review Quality Metrics Module
//!
//! Tracks and reports on review quality and pipeline effectiveness.
//! Parses `.agent/ISSUES.md` to extract issue counts by severity,
//! measures fix success rate, and provides summary statistics.
//!
//! # Module Structure
//!
//! - [`severity`] - Issue severity levels (Critical, High, Medium, Low)
//! - [`issue`] - Individual issue representation
//! - [`metrics`] - Core `ReviewMetrics` struct and parsing logic
//! - [`parser`] - Helper functions for extracting issue data

#![deny(unsafe_code)]

mod issue;
mod metrics;
mod parser;
mod severity;

#[cfg(test)]
mod tests;

// Re-export public types at module level
pub(crate) use metrics::ReviewMetrics;
