//! Ralph: PROMPT-driven multi-agent orchestrator for git repos
//!
//! This crate provides the core functionality for orchestrating AI agents
//! (Claude, Codex, etc.) in a development workflow.

pub mod agents;
pub mod colors;
pub mod config;
pub mod git_helpers;
pub mod json_parser;
pub mod prompts;
pub mod timer;
pub mod utils;

pub use config::Config;
