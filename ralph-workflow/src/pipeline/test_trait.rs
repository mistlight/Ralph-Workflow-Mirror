//! Test trait for agent command execution.
//!
//! This module provides a trait-based abstraction for agent command execution
//! that allows mocking external side effects (subprocess spawning) in tests.
//! Only external side effects are mocked - internal code logic is never mocked.

#![cfg(any(test, feature = "test-utils"))]

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;

use crate::agents::JsonParserType;

/// Result of an agent command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCommandResult {
    /// Exit code from the command (0 = success).
    pub exit_code: i32,
    /// Standard output from the command (for JSON streaming).
    pub stdout: String,
    /// Standard error from the command.
    pub stderr: String,
}

impl AgentCommandResult {
    /// Create a successful result with the given stdout.
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    /// Create a failed result with the given exit code and stderr.
    pub fn failure(exit_code: i32, stderr: impl Into<String>) -> Self {
        Self {
            exit_code,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

/// Configuration for an agent command execution.
#[derive(Debug, Clone)]
pub struct AgentCommandConfig {
    /// The command string to execute (e.g., "claude -p").
    pub cmd: String,
    /// The prompt to pass to the command.
    pub prompt: String,
    /// Environment variables to pass to the command.
    pub env_vars: HashMap<String, String>,
    /// The JSON parser type to use for output.
    pub parser_type: JsonParserType,
    /// Path to the log file.
    pub logfile: String,
    /// Display name for logging.
    pub display_name: String,
}

/// Record of an agent command execution (for test assertions).
#[derive(Debug, Clone)]
pub struct AgentCommandCall {
    /// The command string that was executed.
    pub cmd: String,
    /// The prompt that was passed.
    pub prompt: String,
    /// Environment variables that were passed.
    pub env_vars: HashMap<String, String>,
    /// The JSON parser type that was used.
    pub parser_type: JsonParserType,
}

impl From<&AgentCommandConfig> for AgentCommandCall {
    fn from(config: &AgentCommandConfig) -> Self {
        Self {
            cmd: config.cmd.clone(),
            prompt: config.prompt.clone(),
            env_vars: config.env_vars.clone(),
            parser_type: config.parser_type,
        }
    }
}

/// Trait for executing agent commands.
///
/// This trait abstracts the subprocess spawning for agent execution,
/// allowing tests to control agent behavior without spawning real processes.
///
/// Only external side effects are mocked: subprocess spawning and I/O.
/// Internal code logic is never mocked.
pub trait AgentExecutor {
    /// Execute an agent command and return the result.
    ///
    /// This method is responsible for:
    /// - Spawning the command as a subprocess (or simulating it)
    /// - Passing the prompt to the command
    /// - Setting environment variables
    /// - Capturing stdout/stderr
    /// - Returning the exit code and output
    fn execute(&self, config: &AgentCommandConfig) -> io::Result<AgentCommandResult>;
}

/// Clonable representation of an io::Result.
///
/// Since `io::Error` doesn't implement Clone, we store error info as strings
/// and reconstruct the error on demand.
#[derive(Debug, Clone)]
enum MockResult<T: Clone> {
    Ok(T),
    Err {
        kind: io::ErrorKind,
        message: String,
    },
}

impl<T: Clone> MockResult<T> {
    fn to_io_result(&self) -> io::Result<T> {
        match self {
            MockResult::Ok(v) => Ok(v.clone()),
            MockResult::Err { kind, message } => Err(io::Error::new(*kind, message.clone())),
        }
    }

    fn from_io_result(result: io::Result<T>) -> Self {
        match result {
            Ok(v) => MockResult::Ok(v),
            Err(e) => MockResult::Err {
                kind: e.kind(),
                message: e.to_string(),
            },
        }
    }
}

/// Response configuration for a specific agent call.
#[derive(Debug, Clone)]
struct AgentResponse {
    /// The result to return for this call.
    result: MockResult<AgentCommandResult>,
    /// Optional command pattern to match (if None, matches any command).
    cmd_pattern: Option<String>,
}

/// Mock agent executor that captures calls for assertion.
///
/// This implementation allows tests to verify that specific agent commands
/// were executed and to control their outcomes.
#[derive(Debug)]
pub struct MockAgentExecutor {
    /// Captured calls to execute.
    calls: RefCell<Vec<AgentCommandCall>>,
    /// Responses to return (consumed in order, or uses default_response).
    responses: RefCell<Vec<AgentResponse>>,
    /// Default response if no queued responses remain.
    default_response: RefCell<MockResult<AgentCommandResult>>,
}

impl Default for MockAgentExecutor {
    fn default() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            responses: RefCell::new(Vec::new()),
            default_response: RefCell::new(MockResult::Ok(AgentCommandResult::success(""))),
        }
    }
}

impl MockAgentExecutor {
    /// Create a new mock executor with default successful responses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new mock executor that returns errors for all executions.
    pub fn new_error() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            responses: RefCell::new(Vec::new()),
            default_response: RefCell::new(MockResult::Err {
                kind: io::ErrorKind::Other,
                message: "mock agent error".to_string(),
            }),
        }
    }

    /// Set the default response for all executions (used when no queued responses).
    pub fn with_default_response(self, result: io::Result<AgentCommandResult>) -> Self {
        self.default_response
            .replace(MockResult::from_io_result(result));
        self
    }

    /// Queue a response to return for the next execution.
    ///
    /// Responses are consumed in order. After all queued responses are used,
    /// the default response is returned.
    pub fn with_response(self, result: io::Result<AgentCommandResult>) -> Self {
        self.responses.borrow_mut().push(AgentResponse {
            result: MockResult::from_io_result(result),
            cmd_pattern: None,
        });
        self
    }

    /// Queue a response for a specific command pattern.
    ///
    /// The response will only be used if the command contains the given pattern.
    pub fn with_response_for_cmd(
        self,
        cmd_pattern: impl Into<String>,
        result: io::Result<AgentCommandResult>,
    ) -> Self {
        self.responses.borrow_mut().push(AgentResponse {
            result: MockResult::from_io_result(result),
            cmd_pattern: Some(cmd_pattern.into()),
        });
        self
    }

    /// Queue multiple responses to return in sequence.
    pub fn with_responses(self, results: Vec<io::Result<AgentCommandResult>>) -> Self {
        for result in results {
            self.responses.borrow_mut().push(AgentResponse {
                result: MockResult::from_io_result(result),
                cmd_pattern: None,
            });
        }
        self
    }

    /// Get all captured execution calls.
    pub fn calls(&self) -> Vec<AgentCommandCall> {
        self.calls.borrow().clone()
    }

    /// Get the number of executions.
    pub fn call_count(&self) -> usize {
        self.calls.borrow().len()
    }

    /// Check if any execution was made.
    pub fn was_called(&self) -> bool {
        !self.calls.borrow().is_empty()
    }

    /// Get calls filtered by command pattern.
    pub fn calls_matching(&self, cmd_pattern: &str) -> Vec<AgentCommandCall> {
        self.calls
            .borrow()
            .iter()
            .filter(|call| call.cmd.contains(cmd_pattern))
            .cloned()
            .collect()
    }

    /// Get the prompts that were passed to executions.
    pub fn prompts(&self) -> Vec<String> {
        self.calls
            .borrow()
            .iter()
            .map(|c| c.prompt.clone())
            .collect()
    }

    /// Get the commands that were executed.
    pub fn commands(&self) -> Vec<String> {
        self.calls.borrow().iter().map(|c| c.cmd.clone()).collect()
    }

    /// Clear all captured calls.
    pub fn clear(&self) {
        self.calls.borrow_mut().clear();
    }

    /// Find the next response to return based on the command.
    fn find_response(&self, cmd: &str) -> MockResult<AgentCommandResult> {
        let mut responses = self.responses.borrow_mut();

        // First, look for a response with a matching command pattern
        if let Some(idx) = responses
            .iter()
            .position(|r| r.cmd_pattern.as_ref().is_some_and(|p| cmd.contains(p)))
        {
            return responses.remove(idx).result;
        }

        // Then, use the first generic response (no cmd_pattern)
        if let Some(idx) = responses.iter().position(|r| r.cmd_pattern.is_none()) {
            return responses.remove(idx).result;
        }

        // Finally, use the default response
        self.default_response.borrow().clone()
    }
}

impl AgentExecutor for MockAgentExecutor {
    fn execute(&self, config: &AgentCommandConfig) -> io::Result<AgentCommandResult> {
        // Record the call
        self.calls.borrow_mut().push(AgentCommandCall::from(config));

        // Find and return the appropriate response
        self.find_response(&config.cmd).to_io_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(cmd: &str) -> AgentCommandConfig {
        AgentCommandConfig {
            cmd: cmd.to_string(),
            prompt: "test prompt".to_string(),
            env_vars: HashMap::new(),
            parser_type: JsonParserType::Claude,
            logfile: "/tmp/test.log".to_string(),
            display_name: "test".to_string(),
        }
    }

    #[test]
    fn test_mock_executor_captures_calls() {
        let mock = MockAgentExecutor::new();
        let config = make_config("claude -p");

        let _ = mock.execute(&config);

        assert_eq!(mock.call_count(), 1);
        let calls = mock.calls();
        assert_eq!(calls[0].cmd, "claude -p");
        assert_eq!(calls[0].prompt, "test prompt");
    }

    #[test]
    fn test_mock_executor_returns_default_success() {
        let mock = MockAgentExecutor::new();
        let config = make_config("claude -p");

        let result = mock.execute(&config).unwrap();

        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_mock_executor_with_queued_responses() {
        let mock = MockAgentExecutor::new()
            .with_response(Ok(AgentCommandResult::success("first")))
            .with_response(Ok(AgentCommandResult::success("second")));

        let config = make_config("test");

        let r1 = mock.execute(&config).unwrap();
        let r2 = mock.execute(&config).unwrap();
        let r3 = mock.execute(&config).unwrap(); // falls back to default

        assert_eq!(r1.stdout, "first");
        assert_eq!(r2.stdout, "second");
        assert_eq!(r3.stdout, ""); // default empty success
    }

    #[test]
    fn test_mock_executor_with_failure() {
        let mock = MockAgentExecutor::new()
            .with_response(Ok(AgentCommandResult::failure(1, "error message")));

        let config = make_config("test");
        let result = mock.execute(&config).unwrap();

        assert_eq!(result.exit_code, 1);
        assert_eq!(result.stderr, "error message");
    }

    #[test]
    fn test_mock_executor_error() {
        let mock = MockAgentExecutor::new_error();
        let config = make_config("test");

        let result = mock.execute(&config);

        assert!(result.is_err());
    }

    #[test]
    fn test_mock_executor_response_for_cmd() {
        let mock = MockAgentExecutor::new()
            .with_response_for_cmd("claude", Ok(AgentCommandResult::success("claude response")))
            .with_response_for_cmd("codex", Ok(AgentCommandResult::success("codex response")));

        let claude_config = make_config("claude -p");
        let codex_config = make_config("codex run");

        let r1 = mock.execute(&claude_config).unwrap();
        let r2 = mock.execute(&codex_config).unwrap();

        assert_eq!(r1.stdout, "claude response");
        assert_eq!(r2.stdout, "codex response");
    }

    #[test]
    fn test_mock_executor_calls_matching() {
        let mock = MockAgentExecutor::new();

        let _ = mock.execute(&make_config("claude -p"));
        let _ = mock.execute(&make_config("codex run"));
        let _ = mock.execute(&make_config("claude --verbose"));

        let claude_calls = mock.calls_matching("claude");
        assert_eq!(claude_calls.len(), 2);

        let codex_calls = mock.calls_matching("codex");
        assert_eq!(codex_calls.len(), 1);
    }

    #[test]
    fn test_mock_executor_clear() {
        let mock = MockAgentExecutor::new();

        let _ = mock.execute(&make_config("test"));
        assert_eq!(mock.call_count(), 1);

        mock.clear();
        assert_eq!(mock.call_count(), 0);
    }

    #[test]
    fn test_agent_command_result_helpers() {
        let success = AgentCommandResult::success("output");
        assert_eq!(success.exit_code, 0);
        assert_eq!(success.stdout, "output");
        assert!(success.stderr.is_empty());

        let failure = AgentCommandResult::failure(127, "not found");
        assert_eq!(failure.exit_code, 127);
        assert!(failure.stdout.is_empty());
        assert_eq!(failure.stderr, "not found");
    }
}
