//! Review Quality Metrics Module
//!
//! Tracks and reports on review quality and pipeline effectiveness.
//! Parses `.agent/ISSUES.md` to extract issue counts by severity,
//! measures fix success rate, and provides summary statistics.
//!
//! # Module Structure
//!
//! - [`severity`] - Issue severity levels (Critical, High, Medium, Low)
//! - [`metrics`] - Core `ReviewMetrics` struct and parsing logic

#![deny(unsafe_code)]

mod metrics;
mod severity;

#[cfg(test)]
mod tests;

// Re-export public types at module level
pub use metrics::ReviewMetrics;
