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

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

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

impl<T: Clone + Default> Default for MockResult<T> {
    fn default() -> Self {
        MockResult::Ok(T::default())
    }
}

/// Mock process executor for testing.
///
/// This implementation captures all calls and allows tests to control
/// what each execution returns.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
#[allow(clippy::type_complexity)]
pub struct MockProcessExecutor {
    /// Captured execute calls: (command, args, env, workdir).
    execute_calls: Mutex<Vec<(String, Vec<String>, Vec<(String, String)>, Option<String>)>>,
    /// Mock results indexed by command.
    results: Mutex<HashMap<String, MockResult<ProcessOutput>>>,
    /// Default result for commands not explicitly set.
    default_result: Mutex<MockResult<ProcessOutput>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for MockProcessExecutor {
    fn default() -> Self {
        Self {
            execute_calls: Mutex::new(Vec::new()),
            results: Mutex::new(HashMap::new()),
            default_result: Mutex::new(MockResult::Ok(ProcessOutput {
                status: ExitStatus::from_raw(0),
                stdout: String::new(),
                stderr: String::new(),
            })),
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
        self.with_result(
            command,
            Ok(ProcessOutput {
                status: ExitStatus::from_raw(0),
                stdout: stdout.to_string(),
                stderr: String::new(),
            }),
        )
    }

    /// Set a default failed output for a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command name
    /// * `stderr` - The stderr to return
    pub fn with_error(self, command: &str, stderr: &str) -> Self {
        self.with_result(
            command,
            Ok(ProcessOutput {
                status: ExitStatus::from_raw(1),
                stdout: String::new(),
                stderr: stderr.to_string(),
            }),
        )
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
    #[allow(clippy::type_complexity)]
    pub fn execute_calls(
        &self,
    ) -> Vec<(String, Vec<String>, Vec<(String, String)>, Option<String>)> {
        self.execute_calls.lock().unwrap().clone()
    }

    /// Get all execute calls for a specific command.
    #[allow(clippy::type_complexity)]
    pub fn execute_calls_for(
        &self,
        command: &str,
    ) -> Vec<(String, Vec<String>, Vec<(String, String)>, Option<String>)> {
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
        // This is only used in production code (clipboard, agent spawn)
        // Tests use execute() instead which is properly mocked
        Err(io::Error::other(
            "MockProcessExecutor doesn't support spawn() - use execute() instead",
        ))
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
