//! XML pretty-printing for debugging/fallback display.
//!
//! For user-facing output, prefer using `UIEvent::XmlOutput` which routes
//! through the semantic renderers. This formatter is for:
//! - Debugging/logging where raw XML structure is needed
//! - Fallback rendering when semantic parsing fails
//! - Tests that verify XML structure
//!
//! # Architecture Note
//!
//! The implementation lives in `files::llm_output_extraction` to avoid
//! dependency cycles. This module provides the canonical import path
//! under `rendering`.

pub use crate::files::llm_output_extraction::format_xml_for_display;
