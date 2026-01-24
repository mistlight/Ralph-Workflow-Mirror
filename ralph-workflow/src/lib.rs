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

// Re-export XML extraction and validation functions for use in integration tests
pub use files::llm_output_extraction::extract_development_result_xml;
pub use files::llm_output_extraction::extract_fix_result_xml;
pub use files::llm_output_extraction::extract_issues_xml;
pub use files::llm_output_extraction::format_xml_for_display;
pub use files::llm_output_extraction::validate_development_result_xml;
pub use files::llm_output_extraction::validate_fix_result_xml;
pub use files::llm_output_extraction::validate_issues_xml;

// Re-export process executor
pub use executor::{
    AgentChild, AgentChildHandle, AgentCommandResult, AgentSpawnConfig, ProcessExecutor,
    ProcessOutput, RealAgentChild, RealProcessExecutor,
};

// Re-export mock executor for test-utils feature
#[cfg(any(test, feature = "test-utils"))]
pub use executor::{MockAgentChild, MockProcessExecutor};
