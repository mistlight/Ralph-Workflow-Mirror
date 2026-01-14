//! Container command executor
//!
//! Translates agent commands into container execution.

use crate::container::config::{ContainerConfig, ExecutionOptions};
use crate::container::engine::{ContainerEngine, RunOptions};
use crate::container::error::{ContainerError, ContainerResult};
use crate::container::port::{detect_ports_from_command, PortMapping};
use crate::container::tool::ToolManager;
use crate::container::volume::VolumeManager;
use std::collections::{HashMap, HashSet};

/// Container command executor
///
/// Wraps agent command execution in containers with controlled filesystem access.
pub struct ContainerExecutor {
    /// Container configuration
    config: ContainerConfig,
    /// Volume manager for mount handling
    volume_manager: VolumeManager,
    /// Tool manager for host tool discovery
    tool_manager: ToolManager,
}

impl ContainerExecutor {
    /// Create a new container executor
    pub fn new(config: ContainerConfig) -> Self {
        let volume_manager = VolumeManager::new(
            config.repository_root.clone(),
            config.agent_dir.clone(),
            config.config_dir.clone(),
        );

        let tool_manager = ToolManager::new();

        Self {
            config,
            volume_manager,
            tool_manager,
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

        // Build volume mounts (avoiding duplicate targets)
        let mut mounts = self.volume_manager.get_mounts()?;
        let mut seen_targets: HashSet<String> = mounts.iter().map(|m| m.target.clone()).collect();

        // Discover and add tool mounts
        let tool_mounts = self.tool_manager.discover_tool_mounts()?;
        for tool_mount in tool_mounts {
            if seen_targets.insert(tool_mount.target.clone()) {
                mounts.push(tool_mount.to_mount());
            }
        }

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

        // Add environment variables from tool manager
        for (key, value) in self.tool_manager.get_env_vars() {
            container_env.push((key, value));
        }

        // Set working directory
        let workdir = if let Some(ref wd) = options.working_dir {
            format!("/workspace/{wd}")
        } else {
            "/workspace".to_string()
        };

        // Detect and publish ports that the command might use
        let detected_ports = detect_ports_from_command(&argv);
        let published_ports: Vec<PortMapping> = detected_ports
            .into_iter()
            .map(PortMapping::auto_allocate)
            .collect();

        // Build container run options
        let run_opts = RunOptions {
            image: self.config.image.clone(),
            command: {
                let mut cmd = argv;
                cmd.push("<PROMPT>".to_string()); // Placeholder
                cmd
            },
            mounts,
            env_vars: container_env,
            working_dir: Some(workdir),
            network_enabled: self.config.network_enabled,
            published_ports,
        };

        // Execute in container
        let prompt_bytes = prompt.as_bytes();
        let (stdout, stderr, exit_code) = engine.run_and_capture(&run_opts, Some(prompt_bytes))?;

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Get the container configuration
    pub const fn config(&self) -> &ContainerConfig {
        &self.config
    }

    /// Parse a command string into arguments
    fn parse_command(cmd_str: &str) -> ContainerResult<Vec<String>> {
        let args = shell_words::split(cmd_str).map_err(|_| {
            ContainerError::InvalidConfig(format!("Failed to parse command: {cmd_str}"))
        })?;

        Ok(args)
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

#[cfg(test)]
mod tests {
    use super::*;

    // Test-only helper methods for ExecutionResult
    impl ExecutionResult {
        /// Check if the execution was successful
        pub(crate) const fn is_success(&self) -> bool {
            self.exit_code == 0
        }

        /// Get error message if execution failed
        pub(crate) fn error_message(&self) -> Option<String> {
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
