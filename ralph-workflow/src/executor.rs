//! Process execution abstraction for dependency injection.
//!
//! This module provides a trait-based abstraction for executing external processes,
//! allowing production code to use real processes and test code to use mocks.
//! This follows the same pattern as GitOps trait for dependency injection.
//!
//! # Purpose
//!
//! - Production: `RealProcessExecutor` executes actual commands using `std::process::Command`
//! - Tests: `MockProcessExecutor` captures calls and returns controlled results
//!
//! # Benefits
//!
//! - Test isolation: Tests don't spawn real processes
//! - Determinism: Tests produce consistent results
//! - Speed: Tests run faster without subprocess overhead
//! - Mockability: Full control over process behavior in tests

use crate::agents::JsonParserType;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::ExitStatus;

#[cfg(any(test, feature = "test-utils"))]
use std::sync::Mutex;

#[cfg(any(test, feature = "test-utils"))]
use std::io::Cursor;

/// Output from an executed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// The exit status of process.
    pub status: ExitStatus,
    /// The captured stdout as a UTF-8 string.
    pub stdout: String,
    /// The captured stderr as a UTF-8 string.
    pub stderr: String,
}

/// Configuration for spawning an agent process with streaming support.
///
/// This struct contains all the parameters needed to spawn an agent subprocess,
/// including the command, arguments, environment variables, prompt, and parser type.
#[derive(Debug, Clone)]
pub struct AgentSpawnConfig {
    /// The command to execute (e.g., "claude", "codex").
    pub command: String,
    /// Arguments to pass to the command.
    pub args: Vec<String>,
    /// Environment variables to set for the process.
    pub env: HashMap<String, String>,
    /// The prompt to pass to the agent.
    pub prompt: String,
    /// Path to the log file for output.
    pub logfile: String,
    /// The JSON parser type to use for output.
    pub parser_type: JsonParserType,
}

/// Result of spawning an agent process.
///
/// This wraps the spawned child process with handles to stdout and stderr
/// for streaming output in real-time.
pub struct AgentChildHandle {
    /// The stdout stream for reading agent output.
    pub stdout: Box<dyn io::Read + Send>,
    /// The stderr stream for reading error output.
    pub stderr: Box<dyn io::Read + Send>,
    /// The inner child process handle.
    pub inner: Box<dyn AgentChild>,
}

/// Trait for interacting with a spawned agent child process.
///
/// This trait abstracts the `std::process::Child` operations needed for
/// agent monitoring and output collection. It allows mocking in tests.
pub trait AgentChild: Send + std::fmt::Debug {
    /// Get the process ID.
    fn id(&self) -> u32;

    /// Wait for the process to complete and return the exit status.
    fn wait(&mut self) -> io::Result<std::process::ExitStatus>;

    /// Try to wait without blocking.
    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>>;
}

/// Wrapper for real `std::process::Child`.
pub struct RealAgentChild(pub std::process::Child);

impl std::fmt::Debug for RealAgentChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealAgentChild")
            .field("id", &self.0.id())
            .finish()
    }
}

impl AgentChild for RealAgentChild {
    fn id(&self) -> u32 {
        self.0.id()
    }

    fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        self.0.wait()
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.0.try_wait()
    }
}

/// Result of an agent command execution (for testing).
///
/// This is used by MockProcessExecutor to return mock results without
/// actually spawning processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCommandResult {
    /// Exit code from the command (0 = success).
    pub exit_code: i32,
    /// Standard error from the command.
    pub stderr: String,
}

impl AgentCommandResult {
    /// Create a successful result.
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            stderr: String::new(),
        }
    }

    /// Create a failed result with the given exit code and stderr.
    pub fn failure(exit_code: i32, stderr: impl Into<String>) -> Self {
        Self {
            exit_code,
            stderr: stderr.into(),
        }
    }
}

/// Real process executor that uses `std::process::Command`.
///
/// This is the production implementation that spawns actual processes.
#[derive(Debug, Clone, Default)]
pub struct RealProcessExecutor;

impl RealProcessExecutor {
    /// Create a new RealProcessExecutor.
    pub fn new() -> Self {
        Self
    }
}

impl ProcessExecutor for RealProcessExecutor {
    fn execute(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<ProcessOutput> {
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);

        for (key, value) in env {
            cmd.env(key, value);
        }

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        let output = cmd.output()?;

        Ok(ProcessOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    fn spawn(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<std::process::Child> {
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);

        for (key, value) in env {
            cmd.env(key, value);
        }

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }
}

/// Trait for executing external processes.
///
/// This trait abstracts process execution to allow dependency injection.
/// Production code uses `RealProcessExecutor` which calls actual commands.
/// Test code can use `MockProcessExecutor` to control process behavior.
///
/// Only external process execution is abstracted. Internal code logic is never mocked.
pub trait ProcessExecutor: Send + Sync + std::fmt::Debug {
    /// Execute a command with given arguments and return its output.
    ///
    /// # Arguments
    ///
    /// * `command` - The program to execute
    /// * `args` - Command-line arguments to pass to the program
    /// * `env` - Environment variables to set for the process (optional)
    /// * `workdir` - Working directory for the process (optional)
    ///
    /// # Returns
    ///
    /// Returns a `ProcessOutput` containing exit status, stdout, and stderr.
    ///
    /// # Errors
    ///
    /// Returns an error if command cannot be spawned or if output capture fails.
    fn execute(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<ProcessOutput>;

    /// Spawn a process with stdin input and return the child handle.
    ///
    /// This method is used when you need to write to the process's stdin
    /// or stream its output in real-time. Unlike `execute()`, this returns
    /// a `Child` handle for direct interaction.
    ///
    /// # Arguments
    ///
    /// * `command` - The program to execute
    /// * `args` - Command-line arguments to pass to the program
    /// * `env` - Environment variables to set for the process (optional)
    /// * `workdir` - Working directory for the process (optional)
    ///
    /// # Returns
    ///
    /// Returns a `Child` handle that can be used to interact with the process.
    ///
    /// # Errors
    ///
    /// Returns an error if command cannot be spawned.
    fn spawn(
        &self,
        command: &str,
        args: &[&str],
        env: &[(String, String)],
        workdir: Option<&Path>,
    ) -> io::Result<std::process::Child> {
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);

        for (key, value) in env {
            cmd.env(key, value);
        }

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }

    /// Spawn an agent process with streaming output support.
    ///
    /// This method is specifically designed for spawning AI agent subprocesses
    /// that need to output streaming JSON in real-time. Unlike `spawn()`, this
    /// returns a handle with boxed stdout for trait object compatibility.
    ///
    /// # Arguments
    ///
    /// * `config` - Agent spawn configuration including command, args, env, prompt, etc.
    ///
    /// # Returns
    ///
    /// Returns an `AgentChildHandle` with stdout, stderr, and the child process.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent cannot be spawned.
    ///
    /// # Default Implementation
    ///
    /// The default implementation uses the `spawn()` method with additional
    /// configuration for agent-specific needs. Mock implementations should
    /// override this to return mock results without spawning real processes.
    fn spawn_agent(&self, config: &AgentSpawnConfig) -> io::Result<AgentChildHandle> {
        let mut cmd = std::process::Command::new(&config.command);
        cmd.args(&config.args);

        // Set environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // Add the prompt as the final argument
        cmd.arg(&config.prompt);

        // Set buffering variables for real-time streaming
        cmd.env("PYTHONUNBUFFERED", "1");
        cmd.env("NODE_ENV", "production");

        // Spawn the process with piped stdout/stderr
        let mut child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("Failed to capture stderr"))?;

        Ok(AgentChildHandle {
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            inner: Box::new(RealAgentChild(child)),
        })
    }

    /// Check if a command exists and can be executed.
    ///
    /// This is a convenience method that executes a command with a
    /// `--version` or similar flag to check if it's available.
    ///
    /// # Arguments
    ///
    /// * `command` - The program to check
    ///
    /// # Returns
    ///
    /// Returns `true` if command exists, `false` otherwise.
    fn command_exists(&self, command: &str) -> bool {
        match self.execute(command, &[], &[], None) {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

/// Clonable representation of an io::Result.
///
/// Since io::Error doesn't implement Clone, we store error info as strings
/// and reconstructs the error on demand.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
enum MockResult<T: Clone> {
    Ok(T),
    Err {
        kind: io::ErrorKind,
        message: String,
    },
}

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
impl<T: Clone + Default> Default for MockResult<T> {
    fn default() -> Self {
        MockResult::Ok(T::default())
    }
}

/// Type alias for captured execute calls.
///
/// Each call is a tuple of (command, args, env, workdir).
#[cfg(any(test, feature = "test-utils"))]
type ExecuteCall = (String, Vec<String>, Vec<(String, String)>, Option<String>);

/// Mock process executor for testing.
///
/// This implementation captures all calls and allows tests to control
/// what each execution returns.
#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
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
#[cfg(any(test, feature = "test-utils"))]
fn generate_mock_agent_output(parser_type: JsonParserType, _command: &str) -> String {
    // Valid commit message in XML format for commit generation tests
    let commit_message = r#"<ralph-commit>
<ralph-subject>test: commit message</ralph-subject>
<ralph-body>Test commit message for integration tests.</ralph-body>
</ralph-commit>"#;

    match parser_type {
        JsonParserType::Claude => {
            // Claude expects events with "type" field
            // Include the commit message in the result
            format!(
                r#"{{"type":"result","result":"{}"}}
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
/// This simulates a real Child process but returns a predetermined exit code.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct MockAgentChild {
    exit_code: i32,
}

#[cfg(any(test, feature = "test-utils"))]
impl MockAgentChild {
    fn new(exit_code: i32) -> Self {
        Self { exit_code }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl AgentChild for MockAgentChild {
    fn id(&self) -> u32 {
        0 // Mock PID
    }

    fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        // On Unix, wait status encoding: exit code is in bits 8-15, so shift left by 8
        #[cfg(unix)]
        return Ok(ExitStatus::from_raw(self.exit_code << 8));
        #[cfg(not(unix))]
        return Ok(std::process::ExitStatus::default());
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        // On Unix, wait status encoding: exit code is in bits 8-15, so shift left by 8
        #[cfg(unix)]
        return Ok(Some(ExitStatus::from_raw(self.exit_code << 8)));
        #[cfg(not(unix))]
        return Ok(Some(std::process::ExitStatus::default()));
    }
}

#[cfg(any(test, feature = "test-utils"))]
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
            stderr: Box::new(io::empty()),
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

#[cfg(any(test, feature = "test-utils"))]
impl MockProcessExecutor {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_executor_can_be_created() {
        let executor = RealProcessExecutor::new();
        // Can't test actual execution without real commands
        let _ = executor;
    }

    #[test]
    fn test_real_executor_execute_basic() {
        let executor = RealProcessExecutor::new();
        // Use 'echo' command which should exist on all Unix systems
        let result = executor.execute("echo", &["hello"], &[], None);
        // Should succeed
        assert!(result.is_ok());
        if let Ok(output) = result {
            assert!(output.status.success());
            assert_eq!(output.stdout.trim(), "hello");
        }
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod mock_tests {
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
