//! Container command executor
//!
//! Translates agent commands into container execution.

use crate::container::config::{ContainerConfig, ExecutionOptions};
use crate::container::engine::{ContainerEngine, RunOptions};
use crate::container::volume::VolumeManager;
use crate::container::error::{ContainerError, ContainerResult};
use std::collections::HashMap;

/// Container command executor
///
/// Wraps agent command execution in containers with controlled filesystem access.
pub struct ContainerExecutor {
    /// Container configuration
    config: ContainerConfig,
    /// Volume manager for mount handling
    volume_manager: VolumeManager,
}

impl ContainerExecutor {
    /// Create a new container executor
    pub fn new(config: ContainerConfig) -> Self {
        let volume_manager = VolumeManager::new(
            config.repository_root.clone(),
            config.agent_dir.clone(),
            config.config_dir.clone(),
        );

        Self {
            config,
            volume_manager,
        }
    }

    /// Execute a command in a container
    ///
    /// Takes the agent command and executes it inside a container with proper
    /// volume mounts, environment variables, and working directory.
    pub fn execute(
        &self,
        engine: &ContainerEngine,
        agent_command: &str,
        prompt: &str,
        env_vars: &HashMap<String, String>,
        options: &ExecutionOptions,
    ) -> ContainerResult<ExecutionResult> {
        // Skip container execution if disabled
        if !self.config.enabled {
            return Err(ContainerError::Other(
                "Container mode is disabled".to_string(),
            ));
        }

        // Parse the agent command
        let argv = Self::parse_command(agent_command)?;

        if argv.is_empty() {
            return Err(ContainerError::InvalidConfig(
                "Agent command is empty".to_string(),
            ));
        }

        // Build volume mounts
        let mut mounts = self.volume_manager.get_mounts()?;

        // Add prompt file mount
        // We'll pass the prompt as stdin instead of mounting it

        // Build environment variables
        let mut container_env = Vec::new();

        // Add environment variables from agent config
        for (key, value) in env_vars {
            container_env.push((key.clone(), value.clone()));
        }

        // Add environment variables from execution options
        for (key, value) in &options.env_vars {
            container_env.push((key.clone(), value.clone()));
        }

        // Set working directory
        let workdir = if let Some(ref wd) = options.working_dir {
            format!("/workspace/{}", wd)
        } else {
            "/workspace".to_string()
        };

        // Build container run options
        let run_opts = RunOptions {
            image: self.config.image.clone(),
            command: {
                let mut cmd = argv.clone();
                cmd.push("<PROMPT>".to_string()); // Placeholder
                cmd
            },
            mounts,
            env_vars: container_env,
            working_dir: Some(workdir),
            network_enabled: self.config.network_enabled,
        };

        // Execute in container
        let prompt_bytes = prompt.as_bytes();
        let (stdout, stderr, exit_code) =
            engine.run_and_capture(&run_opts, Some(prompt_bytes))?;

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Check if container mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the container configuration
    pub fn config(&self) -> &ContainerConfig {
        &self.config
    }

    /// Parse a command string into arguments
    fn parse_command(cmd_str: &str) -> ContainerResult<Vec<String>> {
        let args = shell_words::split(cmd_str).map_err(|_| {
            ContainerError::InvalidConfig(format!("Failed to parse command: {}", cmd_str))
        })?;

        Ok(args)
    }

    /// Get the repository root path
    pub fn repository_root(&self) -> &std::path::Path {
        &self.config.repository_root
    }

    /// Get the agent directory path
    pub fn agent_dir(&self) -> &std::path::Path {
        &self.config.agent_dir
    }
}

/// Result of container command execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Exit code from the container
    pub exit_code: i32,
    /// Standard output from the container
    pub stdout: String,
    /// Standard error from the container
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
                "Container command failed with exit code {}: {}",
                self.exit_code, self.stderr
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command() {
        let cmd = "claude -p --output-format=stream-json";
        let argv = ContainerExecutor::parse_command(cmd).unwrap();
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
