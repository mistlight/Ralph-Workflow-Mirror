//! # DEPRECATED - Test trait for agent command execution.
//!
//! **This module is deprecated and will be removed in a future release.**
//!
//! # Migration Path
//!
//! All code using this module should migrate to `ralph_workflow::executor::ProcessExecutor`.
//!
//! ## Before (deprecated):
//!
//! ```ignore
//! use ralph_workflow::pipeline::test_trait::{AgentExecutor, MockAgentExecutor};
//!
//! let mock = MockAgentExecutor::new();
//! let result = mock.execute(&config)?;
//! ```
//!
//! ## After (new approach):
//!
//! ```ignore
//! use ralph_workflow::executor::{ProcessExecutor, MockProcessExecutor, AgentSpawnConfig};
//!
//! let mock = MockProcessExecutor::new()
//!     .with_agent_result("claude", Ok(AgentCommandResult {
//!         exit_code: 0,
//!         stderr: String::new(),
//!     }));
//! let executor: Arc<dyn ProcessExecutor> = Arc::new(mock);
//! let config = AgentSpawnConfig { ... };
//! let agent_handle = executor.spawn_agent(&config)?;
//! ```
//!
//! # Why This Change?
//!
//! The `AgentExecutor` trait only handled agent spawning, while the new `ProcessExecutor`
//! trait provides a unified interface for ALL external process execution (git commands,
//! clipboard operations, and agent spawning). This follows the same dependency injection
//! pattern as the `AppEffectHandler` trait.
//!
//! **Status:** This module is retained for backward compatibility during migration.
//! Do not add new code using this module.

#![cfg(any(test, feature = "test-utils"))]
#![deprecated(note = "Use ralph_workflow::executor::ProcessExecutor instead")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

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

/// File-based mock agent executor that creates files instead of running commands.
///
/// This executor simulates agent behavior by creating expected files and returning
/// success. It is designed to replace shell command mocking in tests, avoiding
/// the need to spawn subprocesses.
///
/// # File Creation Pattern
///
/// Instead of using `RALPH_DEVELOPER_CMD="sh -c 'echo plan > .agent/PLAN.md'"`,
/// tests should pre-create the expected files and use this executor which simply
/// returns success.
///
/// # Example
///
/// ```rust
/// use ralph_workflow::pipeline::test_trait::{AgentExecutor, FileMockAgentExecutor};
///
/// // In test setup
/// fs::write(".agent/PLAN.md", "plan content")?;
///
/// // Use the file mock executor
/// let executor = FileMockAgentExecutor::new();
/// let result = executor.execute(&config)?;
/// assert_eq!(result.exit_code, 0);
/// ```
pub struct FileMockAgentExecutor {
    /// Optional custom exit code to return (default: 0).
    exit_code: RefCell<i32>,
    /// Optional custom stderr to return.
    stderr: RefCell<String>,
}

impl FileMockAgentExecutor {
    /// Create a new file mock executor that returns success.
    pub fn new() -> Self {
        Self {
            exit_code: RefCell::new(0),
            stderr: RefCell::new(String::new()),
        }
    }

    /// Create a new file mock executor that returns failure.
    pub fn new_failure(exit_code: i32, stderr: impl Into<String>) -> Self {
        Self {
            exit_code: RefCell::new(exit_code),
            stderr: RefCell::new(stderr.into()),
        }
    }

    /// Set the exit code to return for subsequent executions.
    pub fn with_exit_code(self, exit_code: i32) -> Self {
        self.exit_code.replace(exit_code);
        self
    }

    /// Set the stderr to return for subsequent executions.
    pub fn with_stderr(self, stderr: impl Into<String>) -> Self {
        self.stderr.replace(stderr.into());
        self
    }
}

impl Default for FileMockAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentExecutor for FileMockAgentExecutor {
    fn execute(&self, config: &AgentCommandConfig) -> io::Result<AgentCommandResult> {
        // Ensure the log file directory exists
        if let Some(parent) = Path::new(&config.logfile).parent() {
            fs::create_dir_all(parent)?;
        }
        // Create an empty log file
        fs::write(&config.logfile, "")?;

        Ok(AgentCommandResult {
            exit_code: *self.exit_code.borrow(),
            stdout: String::new(),
            stderr: self.stderr.borrow().clone(),
        })
    }
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
