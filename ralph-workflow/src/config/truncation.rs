//! Truncation limits for verbosity levels.
//!
//! This module defines the character limits for different content types
//! at each verbosity level. These limits control how much output is displayed
//! in the terminal to balance usability with information density.
//!
//! # Content Types
//!
//! - `text`: Assistant text output
//! - `tool_result`: Tool execution results
//! - `tool_input`: Tool input parameters
//! - `user`: User messages
//! - `result`: Final result summaries
//! - `command`: Command execution strings
//! - `agent_msg`: Agent messages/thinking

/// Truncation limits for Quiet verbosity mode.
///
/// Quiet mode uses aggressive truncation to minimize output noise.
pub mod quiet {
    pub const TEXT: usize = 80;
    pub const TOOL_RESULT: usize = 60;
    pub const TOOL_INPUT: usize = 40;
    pub const USER: usize = 40;
    pub const RESULT: usize = 300;
    pub const COMMAND: usize = 60;
    pub const AGENT_MSG: usize = 80;
    pub const DEFAULT: usize = 60;
}

/// Truncation limits for Normal verbosity mode.
///
/// Normal mode provides balanced output with moderate truncation
/// for better usability while still keeping output manageable.
///
/// NOTE: `TOOL_INPUT` is unlimited in Normal mode because tool input
/// provides crucial context for understanding agent behavior. Truncating
/// it makes it hard to understand what the agent is doing.
pub mod normal {
    pub const TEXT: usize = 400;
    pub const TOOL_RESULT: usize = 300;
    pub const TOOL_INPUT: usize = 999_999; // Unlimited - don't truncate tool input
    pub const USER: usize = 200;
    pub const RESULT: usize = 1500;
    pub const COMMAND: usize = 200;
    pub const AGENT_MSG: usize = 400;
    pub const DEFAULT: usize = 300;
}

/// Truncation limits for Verbose verbosity mode.
///
/// Verbose is the default mode, providing generous limits to help
/// users understand agent behavior without being overwhelming.
///
/// NOTE: `TOOL_INPUT` is unlimited in Verbose mode because this mode
/// is specifically for understanding agent behavior in detail.
pub mod verbose {
    pub const TEXT: usize = 800;
    pub const TOOL_RESULT: usize = 600;
    pub const TOOL_INPUT: usize = 999_999; // Unlimited - don't truncate tool input
    pub const USER: usize = 400;
    pub const RESULT: usize = 3000;
    pub const COMMAND: usize = 400;
    pub const AGENT_MSG: usize = 800;
    pub const DEFAULT: usize = 600;
}

/// Effectively unlimited output for Full/Debug modes.
pub const UNLIMITED: usize = 999_999;

/// Returns the truncation limit for a content type at a given verbosity level.
///
/// # Arguments
///
/// * `level` - The verbosity level (0=Quiet, 1=Normal, 2=Verbose, 3+=Full/Debug)
/// * `content_type` - The type of content being truncated
///
/// # Returns
///
/// The maximum number of characters to display for the given content type.
pub fn get_limit(level: u8, content_type: &str) -> usize {
    match level {
        0 => match content_type {
            "text" => quiet::TEXT,
            "tool_result" => quiet::TOOL_RESULT,
            "tool_input" => quiet::TOOL_INPUT,
            "user" => quiet::USER,
            "result" => quiet::RESULT,
            "command" => quiet::COMMAND,
            "agent_msg" => quiet::AGENT_MSG,
            _ => quiet::DEFAULT,
        },
        1 => match content_type {
            "text" => normal::TEXT,
            "tool_result" => normal::TOOL_RESULT,
            "tool_input" => normal::TOOL_INPUT,
            "user" => normal::USER,
            "result" => normal::RESULT,
            "command" => normal::COMMAND,
            "agent_msg" => normal::AGENT_MSG,
            _ => normal::DEFAULT,
        },
        2 => match content_type {
            "text" => verbose::TEXT,
            "tool_result" => verbose::TOOL_RESULT,
            "tool_input" => verbose::TOOL_INPUT,
            "user" => verbose::USER,
            "result" => verbose::RESULT,
            "command" => verbose::COMMAND,
            "agent_msg" => verbose::AGENT_MSG,
            _ => verbose::DEFAULT,
        },
        _ => UNLIMITED,
    }
}
