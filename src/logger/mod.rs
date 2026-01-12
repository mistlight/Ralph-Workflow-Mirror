//! Logging and progress display utilities.
//!
//! This module provides structured logging for Ralph's pipeline:
//! - `Logger` struct for consistent, colorized output
//! - Progress bar display
//! - Section headers and formatting
//!
//! # Example
//!
//! ```ignore
//! use ralph::logger::Logger;
//! use ralph::colors::Colors;
//!
//! let colors = Colors::new();
//! let logger = Logger::new(colors)
//!     .with_log_file(".agent/logs/pipeline.log");
//!
//! logger.info("Starting pipeline...");
//! logger.success("Task completed");
//! logger.warn("Potential issue detected");
//! logger.error("Critical failure");
//! ```

mod output;
mod progress;

pub use output::{strip_ansi_codes, Logger};
pub use progress::print_progress;

// Re-export timestamp for backward compatibility
pub use crate::checkpoint::timestamp;
