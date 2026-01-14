//! User account security mode executor
//!
//! Runs agent commands as a dedicated user account for isolation.
//!
//! This provides an alternative to container isolation that:
//! - Works on macOS (where Linux containers can't run macOS binaries)
//! - Allows sharing all host tools without mounting
//! - Provides filesystem isolation through user permissions

use crate::container::config::ExecutionOptions;
use crate::container::error::{ContainerError, ContainerResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default user name for the agent account
pub const DEFAULT_AGENT_USER: &str = "ralph-agent";

/// User account executor
///
/// Runs commands as a dedicated user for filesystem isolation.
pub struct UserAccountExecutor {
    /// Workspace/repository root path
    workspace_path: PathBuf,
    /// User name to run commands as
    user_name: String,
    /// Path to .agent directory
    agent_dir: PathBuf,
}

impl UserAccountExecutor {
    /// Create a new user account executor
    ///
    /// Verifies the user exists and can be used for command execution.
    pub fn new(
        workspace_path: PathBuf,
        agent_dir: PathBuf,
        user_name: Option<String>,
    ) -> ContainerResult<Self> {
        let user_name = user_name.unwrap_or_else(|| DEFAULT_AGENT_USER.to_string());

        // Verify the user exists
        if !Self::user_exists(&user_name)? {
            return Err(ContainerError::Other(format!(
                "User account '{}' does not exist.\n\n\
                To set up user-account security mode:\n\
                1. Run: ralph --setup-security\n\
                2. Or manually: sudo useradd -m -s /bin/bash {}\n\
                3. Then: sudo echo '{} ALL=(ALL) NOPASSWD: ALL' | sudo tee /etc/sudoers.d/ralph-agent\n\
                4. And: sudo chmod 440 /etc/sudoers.d/ralph-agent\n\n\
                For more information, run: ralph --security-check",
                user_name, user_name, user_name
            )));
        }

        // Verify workspace access
        let has_access = Command::new("sudo")
            .arg("-u")
            .arg(&user_name)
            .arg("test")
            .arg("-r")
            .arg(&workspace_path)
            .output();

        match has_access {
            Ok(output) if !output.status.success() => {
                let parent_dir = workspace_path.parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| ".".to_string());

                return Err(ContainerError::Other(format!(
                    "User '{}' cannot read the workspace at: {}\n\n\
                    Fix with: sudo chmod +rx {}\n\
                    Or run: sudo chown -R $USER:{} {}",
                    user_name,
                    workspace_path.display(),
                    parent_dir,
                    user_name,
                    workspace_path.display()
                )));
            }
            Err(_) => {
                // Failed to check access - might be a sudo configuration issue
                // We'll let execution fail with better error message later
            }
            _ => {}
        }

        Ok(Self {
            workspace_path,
            user_name,
            agent_dir,
        })
    }

    /// Execute a command as the dedicated user
    pub fn execute(
        &self,
        agent_command: &str,
        prompt: &str,
        env_vars: &HashMap<String, String>,
        options: &ExecutionOptions,
    ) -> ContainerResult<ExecutionResult> {
        // Parse the command
        let argv = Self::parse_command(agent_command)?;

        if argv.is_empty() {
            return Err(ContainerError::InvalidConfig(
                "Agent command is empty".to_string(),
            ));
        }

        // Set up working directory
        let workdir = if let Some(ref wd) = options.working_dir {
            self.workspace_path.join(wd)
        } else {
            self.workspace_path.clone()
        };

        // Build the sudo command
        let mut cmd = Command::new("sudo");
        cmd.arg("-u").arg(&self.user_name);

        // Set working directory
        cmd.arg("--cwd").arg(&workdir);

        // Set environment variables
        for (key, value) in env_vars {
            cmd.arg("--env");
            cmd.arg(format!("{}={}", key, value));
        }

        // Add execution option environment variables
        for (key, value) in &options.env_vars {
            cmd.arg("--env");
            cmd.arg(format!("{}={}", key, value));
        }

        // Add the actual command
        cmd.arg(&argv[0]);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }

        // Add prompt argument
        cmd.arg(prompt);

        // Execute and capture output
        let output = cmd.output().map_err(|e| {
            ContainerError::Other(format!("Failed to execute command: {}", e))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Check if a user account exists on the system
    pub fn user_exists(user_name: &str) -> ContainerResult<bool> {
        let output = Command::new("id")
            .arg(user_name)
            .output();

        match output {
            Ok(out) => Ok(out.status.success()),
            Err(_) => Ok(false),
        }
    }

    /// Get information about a user account
    pub fn get_user_info(user_name: &str) -> ContainerResult<Option<UserInfo>> {
        let output = Command::new("id")
            .arg(user_name)
            .output();

        let output = output.map_err(|e| {
            ContainerError::Other(format!("Failed to get user info: {}", e))
        })?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse "uid=1001(ralph-agent) gid=1001(ralph-agent) groups=1001(ralph-agent)"
        let info = stdout.trim();
        Ok(Some(UserInfo {
            name: user_name.to_string(),
            info: info.to_string(),
        }))
    }

    /// Verify the user can access the workspace
    pub fn verify_workspace_access(&self) -> ContainerResult<bool> {
        // Check if the workspace exists
        if !self.workspace_path.exists() {
            return Ok(false);
        }

        // Try to see what access the user has
        let output = Command::new("sudo")
            .arg("-u")
            .arg(&self.user_name)
            .arg("test")
            .arg("-r")
            .arg(&self.workspace_path)
            .output();

        Ok(output.map(|o| o.status.success()).unwrap_or(false))
    }

    /// Parse a command string into arguments
    fn parse_command(cmd_str: &str) -> ContainerResult<Vec<String>> {
        let args = shell_words::split(cmd_str).map_err(|_| {
            ContainerError::InvalidConfig(format!("Failed to parse command: {}", cmd_str))
        })?;

        Ok(args)
    }

    /// Get the user name
    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    /// Get the workspace path
    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    /// Get the agent directory path
    pub fn agent_dir(&self) -> &Path {
        &self.agent_dir
    }
}

/// User account information
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// User name
    pub name: String,
    /// User info string from `id` command
    pub info: String,
}

/// Result of user account command execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Exit code from the command
    pub exit_code: i32,
    /// Standard output from the command
    pub stdout: String,
    /// Standard error from the command
    pub stderr: String,
}

impl ExecutionResult {
    /// Check if the execution was successful
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get error message if execution failed
    pub fn error_message(&self) -> Option<String> {
        if self.is_success() {
            None
        } else {
            Some(format!(
                "Command failed with exit code {}: {}",
                self.exit_code, self.stderr
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_exists_root() {
        // Root user should always exist
        assert!(UserAccountExecutor::user_exists("root").unwrap());
    }

    #[test]
    fn test_parse_command() {
        let cmd = "claude -p --output-format=stream-json";
        let argv = UserAccountExecutor::parse_command(cmd).unwrap();
        assert_eq!(argv, vec!["claude", "-p", "--output-format=stream-json"]);
    }

    #[test]
    fn test_execution_result_success() {
        let result = ExecutionResult {
            exit_code: 0,
            stdout: "Success".to_string(),
            stderr: String::new(),
        };
        assert!(result.is_success());
        assert!(result.error_message().is_none());
    }

    #[test]
    fn test_execution_result_failure() {
        let result = ExecutionResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "Error occurred".to_string(),
        };
        assert!(!result.is_success());
        assert!(result.error_message().is_some());
    }
}
