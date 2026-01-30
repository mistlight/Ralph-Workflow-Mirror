//! Generic JSON pretty-printing for display.
//!
//! This module provides the canonical entrypoint for JSON formatting.
//! The implementation lives in `logger/output.rs` to avoid dependency cycles.
//!
//! # Architecture Note
//!
//! The format function is kept in `logger` module where it has access to
//! verbosity settings. This module provides the canonical import path
//! under `rendering`.

pub use crate::logger::format_generic_json_for_display;
