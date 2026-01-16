//! Ralph workflow library for commit message parsing and validation.
//!
//! This library exposes the core functionality used by the ralph binary,
//! including commit message extraction from LLM output.

pub mod agents;
pub mod app;
pub mod banner;
pub mod checkpoint;
pub mod cli;
pub mod common;
pub mod config;
pub mod diagnostics;
pub mod files;
pub mod git_helpers;
pub mod guidelines;
pub mod json_parser;
pub mod language_detector;
pub mod logger;
pub mod phases;
pub mod pipeline;
pub mod platform;
pub mod prompts;
pub mod review_metrics;
pub mod templates;
