//! Ralph workflow library for AI agent orchestration.
//!
//! This crate provides the core functionality for the `ralph` CLI binary,
//! implementing a reducer-based architecture for orchestrating AI coding agents
//! through development and review cycles.
//!
//! # Architecture
//!
//! Ralph uses an event-sourced reducer architecture with two effect layers:
//!
//! - [`app`] - CLI layer effects (before repo root is known)
//! - [`reducer`] - Pipeline effects (after repo root is established)
//!
//! All I/O is abstracted through traits for testability:
//!
//! - [`workspace::Workspace`] - Filesystem operations
//! - [`executor::ProcessExecutor`] - Process spawning
//!
//! # Feature Flags
//!
//! - `monitoring` (default) - Enable streaming metrics and debugging APIs
//! - `test-utils` - Enable test utilities (MockProcessExecutor, etc.)
//! - `hardened-resume` (default) - Enable checkpoint file state capture for recovery
//!
//! # For Library Users
//!
//! This crate is primarily designed as a binary. Library usage is supported
//! for integration testing with the `test-utils` feature:
//!
//! ```toml
//! [dev-dependencies]
//! ralph-workflow = { version = "0.6", features = ["test-utils"] }
//! ```
//!
//! # Key Modules
//!
//! - [`agents`] - Agent configuration, registry, and CCS support
//! - [`reducer`] - Core state machine and effect handling
//! - [`phases`] - Pipeline phase implementations (development, review, commit)
//! - [`workspace`] - Filesystem abstraction (production and test implementations)
//! - [`executor`] - Process execution abstraction
//! - [`json_parser`] - NDJSON streaming parsers for Claude, Codex, Gemini, OpenCode

pub mod agents;
pub mod app;
pub mod banner;
pub mod checkpoint;
pub mod cli;
pub mod common;
pub mod config;
pub mod diagnostics;
pub mod executor;
pub mod files;
pub mod git_helpers;
pub mod guidelines;
pub mod interrupt;
pub mod json_parser;
pub mod language_detector;
pub mod logger;
pub mod phases;
pub mod pipeline;
pub mod platform;
pub mod prompts;
pub mod reducer;
pub mod review_metrics;
pub mod templates;
pub mod workspace;

// Re-export XML extraction and validation functions for use in integration tests
pub use files::llm_output_extraction::extract_development_result_xml;
pub use files::llm_output_extraction::extract_fix_result_xml;
pub use files::llm_output_extraction::extract_issues_xml;
pub use files::llm_output_extraction::validate_development_result_xml;
pub use files::llm_output_extraction::validate_fix_result_xml;
pub use files::llm_output_extraction::validate_issues_xml;

// Deprecated: Use UIEvent::XmlOutput for user-facing XML display.
// This re-export is kept for backward compatibility with tests and debugging.
#[deprecated(
    since = "0.8.0",
    note = "Use UIEvent::XmlOutput for user-facing XML display. This function is kept for debugging/logging only."
)]
pub use files::llm_output_extraction::format_xml_for_display;

// Re-export process executor
pub use executor::{
    AgentChild, AgentChildHandle, AgentCommandResult, AgentSpawnConfig, ProcessExecutor,
    ProcessOutput, RealAgentChild, RealProcessExecutor,
};

// Re-export mock executor for test-utils feature
#[cfg(any(test, feature = "test-utils"))]
pub use executor::{MockAgentChild, MockProcessExecutor};
