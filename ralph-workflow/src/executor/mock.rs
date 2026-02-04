//! Mock process executor for testing.
//!
//! This module provides a mock implementation of ProcessExecutor that
//! captures all calls and allows tests to control process behavior.
//! Only available with the `test-utils` feature.

use super::{
    AgentChild, AgentChildHandle, AgentCommandResult, AgentSpawnConfig, ProcessExecutor,
    ProcessOutput,
};
use crate::agents::JsonParserType;
use std::collections::HashMap;
use std::io::{self, Cursor};
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Mutex;

/// Clonable representation of an io::Result.
///
/// Since io::Error doesn't implement Clone, we store error info as strings
/// and reconstruct the error on demand.
#[derive(Debug, Clone)]
pub(crate) enum MockResult<T: Clone> {
    Ok(T),
    Err {
        kind: io::ErrorKind,
        message: String,
    },
}

impl<T: Clone> MockResult<T> {
    pub(crate) fn to_io_result(&self) -> io::Result<T> {
        match self {
            MockResult::Ok(v) => Ok(v.clone()),
            MockResult::Err { kind, message } => Err(io::Error::new(*kind, message.clone())),
        }
    }

    pub(crate) fn from_io_result(result: io::Result<T>) -> Self {
        match result {
            Ok(v) => MockResult::Ok(v),
            Err(e) => MockResult::Err {
                kind: e.kind(),
                message: e.to_string(),
            },
        }
    }
}

impl<T: Clone + Default> Default for MockResult<T> {
    fn default() -> Self {
        MockResult::Ok(T::default())
    }
}

/// Type alias for captured execute calls.
///
/// Each call is a tuple of (command, args, env, workdir).
pub type ExecuteCall = (String, Vec<String>, Vec<(String, String)>, Option<String>);

/// Mock process executor for testing.
///
/// This implementation captures all calls and allows tests to control
/// what each execution returns.
#[derive(Debug)]
pub struct MockProcessExecutor {
    /// Captured execute calls: (command, args, env, workdir).
    execute_calls: Mutex<Vec<ExecuteCall>>,
    /// Mock results indexed by command.
    results: Mutex<HashMap<String, MockResult<ProcessOutput>>>,
    /// Default result for commands not explicitly set.
    default_result: Mutex<MockResult<ProcessOutput>>,
    /// Captured agent spawn calls.
    agent_calls: Mutex<Vec<AgentSpawnConfig>>,
    /// Mock agent results indexed by command pattern.
    agent_results: Mutex<HashMap<String, MockResult<AgentCommandResult>>>,
    /// Default agent result.
    default_agent_result: Mutex<MockResult<AgentCommandResult>>,
}

impl Default for MockProcessExecutor {
    fn default() -> Self {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        Self {
            execute_calls: Mutex::new(Vec::new()),
            results: Mutex::new(HashMap::new()),
            #[cfg(unix)]
            default_result: Mutex::new(MockResult::Ok(ProcessOutput {
                status: ExitStatus::from_raw(0),
                stdout: String::new(),
                stderr: String::new(),
            })),
            #[cfg(not(unix))]
            default_result: Mutex::new(MockResult::Ok(ProcessOutput {
                status: std::process::ExitStatus::default(),
                stdout: String::new(),
                stderr: String::new(),
            })),
            agent_calls: Mutex::new(Vec::new()),
            agent_results: Mutex::new(HashMap::new()),
            default_agent_result: Mutex::new(MockResult::Ok(AgentCommandResult::success())),
        }
    }
}

impl MockProcessExecutor {
    /// Create a new MockProcessExecutor with default successful responses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new MockProcessExecutor that returns errors for all commands.
    pub fn new_error() -> Self {
        fn err_result<T: Clone>(msg: &str) -> MockResult<T> {
            MockResult::Err {
                kind: io::ErrorKind::Other,
                message: msg.to_string(),
            }
        }

        Self {
            execute_calls: Mutex::new(Vec::new()),
            results: Mutex::new(HashMap::new()),
            default_result: Mutex::new(err_result("mock process error")),
            agent_calls: Mutex::new(Vec::new()),
            agent_results: Mutex::new(HashMap::new()),
            default_agent_result: Mutex::new(err_result("mock agent error")),
        }
    }

    /// Set the mock result for a specific command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command name
    /// * `result` - The mock result to return
    pub fn with_result(self, command: &str, result: io::Result<ProcessOutput>) -> Self {
        self.results
            .lock()
            .unwrap()
            .insert(command.to_string(), MockResult::from_io_result(result));
        self
    }

    /// Set a default successful output for a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command name
    /// * `stdout` - The stdout to return
    pub fn with_output(self, command: &str, stdout: &str) -> Self {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        #[cfg(unix)]
        let result = Ok(ProcessOutput {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
        });
        #[cfg(not(unix))]
        let result = Ok(ProcessOutput {
            status: std::process::ExitStatus::default(),
            stdout: stdout.to_string(),
            stderr: String::new(),
        });
        self.with_result(command, result)
    }

    /// Set a default failed output for a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command name
    /// * `stderr` - The stderr to return
    pub fn with_error(self, command: &str, stderr: &str) -> Self {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        #[cfg(unix)]
        let result = Ok(ProcessOutput {
            status: ExitStatus::from_raw(1),
            stdout: String::new(),
            stderr: stderr.to_string(),
        });
        #[cfg(not(unix))]
        let result = Ok(ProcessOutput {
            status: std::process::ExitStatus::default(),
            stdout: String::new(),
            stderr: stderr.to_string(),
        });
        self.with_result(command, result)
    }

    /// Set a mock error result for a specific command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command name
    /// * `kind` - The error kind
    /// * `message` - The error message
    pub fn with_io_error(self, command: &str, kind: io::ErrorKind, message: &str) -> Self {
        self.with_result(command, Err(io::Error::new(kind, message)))
    }

    /// Get the number of times execute was called.
    pub fn execute_count(&self) -> usize {
        self.execute_calls.lock().unwrap().len()
    }

    /// Get all execute calls.
    ///
    /// Each call is a tuple of (command, args, env, workdir).
    pub fn execute_calls(&self) -> Vec<ExecuteCall> {
        self.execute_calls.lock().unwrap().clone()
    }

    /// Get all execute calls for a specific command.
    pub fn execute_calls_for(&self, command: &str) -> Vec<ExecuteCall> {
        self.execute_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(cmd, _, _, _)| cmd == command)
            .cloned()
            .collect()
    }

    /// Reset all captured calls.
    pub fn reset_calls(&self) {
        self.execute_calls.lock().unwrap().clear();
        self.agent_calls.lock().unwrap().clear();
    }

    /// Set a mock result for agent spawning.
    ///
    /// # Arguments
    ///
    /// * `command_pattern` - Pattern to match against the agent command
    /// * `result` - The mock result to return when the pattern matches
    pub fn with_agent_result(
        self,
        command_pattern: &str,
        result: io::Result<AgentCommandResult>,
    ) -> Self {
        self.agent_results.lock().unwrap().insert(
            command_pattern.to_string(),
            MockResult::from_io_result(result),
        );
        self
    }

    /// Get all agent spawn calls.
    pub fn agent_calls(&self) -> Vec<AgentSpawnConfig> {
        self.agent_calls.lock().unwrap().clone()
    }

    /// Get agent spawn calls for a specific command pattern.
    pub fn agent_calls_for(&self, command_pattern: &str) -> Vec<AgentSpawnConfig> {
        self.agent_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|config| config.command.contains(command_pattern))
            .cloned()
            .collect()
    }

    /// Find the agent result for a given command pattern.
    fn find_agent_result(&self, command: &str) -> AgentCommandResult {
        // Look for an exact match first
        if let Some(result) = self.agent_results.lock().unwrap().get(command) {
            return result
                .clone()
                .to_io_result()
                .unwrap_or_else(|_| AgentCommandResult::success());
        }

        // Look for a partial pattern match
        for (pattern, result) in self.agent_results.lock().unwrap().iter() {
            if command.contains(pattern) {
                return result
                    .clone()
                    .to_io_result()
                    .unwrap_or_else(|_| AgentCommandResult::success());
            }
        }

        // Use default result
        self.default_agent_result
            .lock()
            .unwrap()
            .clone()
            .to_io_result()
            .unwrap_or_else(|_| AgentCommandResult::success())
    }
}

/// Generate minimal valid agent output for mock testing.
///
/// This function creates a minimal valid NDJSON output that the streaming
/// parser can successfully parse without hanging. The output format depends
/// on the parser type being used.
///
/// # Arguments
///
/// * `parser_type` - The JSON parser type (Claude, Codex, Gemini, OpenCode, Generic)
/// * `_command` - The agent command name (reserved for future logging/debugging)
///
/// # Returns
///
/// A string containing valid NDJSON output for the given parser type.
fn generate_mock_agent_output(parser_type: JsonParserType, _command: &str) -> String {
    // Valid commit message in XML format for commit generation tests
    let commit_message = r#"<ralph-commit>
<ralph-subject>test: commit message</ralph-subject>
<ralph-body>Test commit message for integration tests.</ralph-body>
</ralph-commit>"#;

    match parser_type {
        JsonParserType::Claude => {
            // Claude expects events with "type" field
            // Include session_id in init event (for session continuation tests)
            // and the commit message in the result
            format!(
                r#"{{"type":"system","subtype":"init","session_id":"ses_mock_session_12345"}}
{{"type":"result","result":"{}"}}
"#,
                commit_message.replace('\n', "\\n").replace('"', "\\\"")
            )
        }
        JsonParserType::Codex => {
            // Codex expects completion events with the actual content
            // We need to provide events that will be written to the log file
            // and then extracted as the commit message
            format!(
                r#"{{"type":"turn_started","turn_id":"test_turn"}}
{{"type":"item_started","item":{{"type":"agent_message","text":"{}"}}}}
{{"type":"item_completed","item":{{"type":"agent_message","text":"{}"}}}}
{{"type":"turn_completed"}}
{{"type":"completion","reason":"stop"}}
"#,
                commit_message, commit_message
            )
        }
        JsonParserType::Gemini => {
            // Gemini expects message events with content
            format!(
                r#"{{"type":"message","role":"assistant","content":"{}"}}
{{"type":"result","status":"success"}}
"#,
                commit_message.replace('\n', "\\n")
            )
        }
        JsonParserType::OpenCode => {
            // OpenCode expects text events
            format!(
                r#"{{"type":"text","content":"{}"}}
{{"type":"end","success":true}}
"#,
                commit_message.replace('\n', "\\n")
            )
        }
        JsonParserType::Generic => {
            // Generic parser treats all output as plain text
            // Return the commit message directly
            format!("{}\n", commit_message)
        }
    }
}

/// Mock agent child process for testing.
///
/// This simulates a real Child process with configurable termination behavior.
#[derive(Debug)]
pub struct MockAgentChild {
    exit_code: i32,
    /// Simulates a process that hasn't terminated yet
    still_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MockAgentChild {
    pub fn new(exit_code: i32) -> Self {
        Self {
            exit_code,
            still_running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a mock child that simulates a running process that needs to be killed.
    /// Set the returned AtomicBool to `false` to simulate process termination.
    pub fn new_running(exit_code: i32) -> (Self, std::sync::Arc<std::sync::atomic::AtomicBool>) {
        let still_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let controller = std::sync::Arc::clone(&still_running);
        (
            Self {
                exit_code,
                still_running,
            },
            controller,
        )
    }
}

impl AgentChild for MockAgentChild {
    fn id(&self) -> u32 {
        12345 // Mock PID
    }

    fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        // Wait until process is no longer running
        while self
            .still_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // On Unix, wait status encoding: exit code is in bits 8-15, so shift left by 8
        #[cfg(unix)]
        return Ok(ExitStatus::from_raw(self.exit_code << 8));
        #[cfg(not(unix))]
        return Ok(std::process::ExitStatus::default());
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        if self
            .still_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Ok(None); // Still running
        }

        // On Unix, wait status encoding: exit code is in bits 8-15, so shift left by 8
        #[cfg(unix)]
        return Ok(Some(ExitStatus::from_raw(self.exit_code << 8)));
        #[cfg(not(unix))]
        return Ok(Some(std::process::ExitStatus::default()));
    }
}

impl ProcessExecutor for MockProcessExecutor {
    fn spawn(
        &self,
        _command: &str,
        _args: &[&str],
        _env: &[(String, String)],
        _workdir: Option<&Path>,
    ) -> io::Result<std::process::Child> {
        // Mock executor doesn't support real spawning
        // This is only used in production code (clipboard, etc.)
        // Tests use execute() instead which is properly mocked
        Err(io::Error::other(
            "MockProcessExecutor doesn't support spawn() - use execute() instead",
        ))
    }

    fn spawn_agent(&self, config: &AgentSpawnConfig) -> io::Result<AgentChildHandle> {
        // Record the call for assertions
        self.agent_calls.lock().unwrap().push(config.clone());

        // Find the appropriate mock result
        let result = self.find_agent_result(&config.command);

        // Generate minimal valid JSON output based on parser type
        // This prevents the streaming parser from hanging on empty input
        let mock_output = generate_mock_agent_output(config.parser_type, &config.command);

        // Return a mock handle with valid stdout that provides complete JSON
        Ok(AgentChildHandle {
            stdout: Box::new(Cursor::new(mock_output)),
            // Provide stderr content so tests can exercise error classification
            // paths that depend on stderr (e.g., 429 rate limits).
            stderr: Box::new(Cursor::new(result.stderr)),
            inner: Box::new(MockAgentChild::new(result.exit_code)),
        })
    }

    fn execute(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<ProcessOutput> {
        // Capture the call
        let workdir_str = workdir.map(|p| p.display().to_string());
        self.execute_calls.lock().unwrap().push((
            command.to_string(),
            args.iter().map(|s| s.to_string()).collect(),
            env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            workdir_str,
        ));

        // Return the mock result
        if let Some(result) = self.results.lock().unwrap().get(command) {
            result.to_io_result()
        } else {
            self.default_result.lock().unwrap().to_io_result()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_executor_captures_calls() {
        let mock = MockProcessExecutor::new();
        let _ = mock.execute("echo", &["hello"], &[], None);

        assert_eq!(mock.execute_count(), 1);
        let calls = mock.execute_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "echo");
        assert_eq!(calls[0].1, vec!["hello"]);
    }

    #[test]
    fn test_mock_executor_returns_output() {
        let mock = MockProcessExecutor::new().with_output("git", "git version 2.40.0");

        let result = mock.execute("git", &["--version"], &[], None).unwrap();
        assert_eq!(result.stdout, "git version 2.40.0");
        assert!(result.status.success());
    }

    #[test]
    fn test_mock_executor_returns_error() {
        let mock = MockProcessExecutor::new().with_io_error(
            "git",
            io::ErrorKind::NotFound,
            "git not found",
        );

        let result = mock.execute("git", &["--version"], &[], None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert_eq!(err.to_string(), "git not found");
    }

    #[test]
    fn test_mock_executor_can_be_reset() {
        let mock = MockProcessExecutor::new();
        let _ = mock.execute("echo", &["test"], &[], None);

        assert_eq!(mock.execute_count(), 1);
        mock.reset_calls();
        assert_eq!(mock.execute_count(), 0);
    }

    #[test]
    fn test_mock_executor_command_exists() {
        let mock = MockProcessExecutor::new().with_output("which", "/usr/bin/git");

        assert!(mock.command_exists("which"));
    }

    #[test]
    fn test_mock_executor_command_not_exists() {
        let mock = MockProcessExecutor::new_error();
        assert!(!mock.command_exists("nonexistent"));
    }
}
