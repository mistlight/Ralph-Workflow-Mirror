//! Shared tests for JSON parsers.
//!
//! This module contains tests for cross-parser behavior, shared utilities,
//! and streaming functionality that applies to multiple parsers.

use super::*;
use crate::config::Verbosity;
use crate::logger::Colors;
use crate::workspace::MemoryWorkspace;

#[cfg(test)]
use super::terminal::TerminalMode;

#[cfg(test)]
use crate::json_parser::printer::{SharedPrinter, TestPrinter};
#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::rc::Rc;

// Cross-parser behavior tests (verbosity, display names, tool use)
include!("tests/cross_parser.rs");

// DeltaAccumulator shared type tests
include!("tests/delta_accumulator.rs");

// Streaming behavior tests (format, classifier, health, session, deduplication)
include!("tests/streaming.rs");
