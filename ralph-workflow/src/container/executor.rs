//! Container command executor
//!
//! Translates agent commands into container execution.
#![cfg_attr(feature = "security-mode", expect(dead_code))]

use crate::container::codex;
use crate::container::config::{validate_working_dir, ContainerConfig, ExecutionOptions};
use crate::container::engine::{ContainerEngine, RunOptions};
use crate::container::error::{ContainerError, ContainerResult};
use crate::container::port::{detect_ports_from_command, PortMapping};
use crate::container::tool::ToolManager;
use crate::container::volume::VolumeManager;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// macOS-specific binaries that won't work in Linux containers
const MACOS_SPECIFIC_BINARIES: &[&str] = &[
    // macOS system utilities
    "brew",
    "sw_vers",
    "system_profiler",
    "defaults",
    "plutil",
    "pbcopy",
    "pbpaste",
    "osascript",
    "launchctl",
    "launchd",
    "kextstat",
    "kextload",
    "kextunload",
    "dscl",
    "dscacheutil",
    "scutil",
    "networksetup",
    "diskutil",
    "hdiutil",
    "tmutil",
    "afsutil",
    "fsck",
    "diskutil",
    "pwpolicy",
    "security",
    "codesign",
    "sip",
    // macOS-specific development tools
    "xcodebuild",
    "xcrun",
    "swift",
    "swiftc",
    "swift-package",
    "swift-test",
    // Homebrew macOS-only tools
    "mas",
    "brewcask",
];

/// Check if a command appears to be a macOS-specific binary
///
/// This helps detect cases where the agent is trying to run a binary
/// that only works on macOS and won't work in a Linux container.
fn is_macos_specific_command(command: &str) -> Option<&'static str> {
    let command_lower = command.to_lowercase();

    MACOS_SPECIFIC_BINARIES
        .iter()
        .find(|&&binary| command_lower.contains(binary))
        .copied()
}

/// Check if a binary file is a macOS Mach-O binary
///
/// Reads the first few bytes of a file to check for Mach-O magic numbers.
fn is_macho_binary(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    // Read first 4 bytes to check for Mach-O magic numbers
    let magic = match std::fs::read(path) {
        Ok(mut bytes) if bytes.len() >= 4 => {
            bytes.truncate(4);
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        _ => return false,
    };

    // Mach-O magic numbers (32-bit and 64-bit, both byte orders)
    matches!(magic, 0xFEED_FACE | 0xFEED_FACF | 0xCEFA_EDFE | 0xCFFA_EDFE)
}

/// Check if an environment variable name is dangerous and should be filtered.
///
/// Dangerous environment variables are those that could be used to escape
/// isolation or compromise security, such as:
/// - PATH manipulation (to execute arbitrary binaries)
/// - Dynamic linker configuration (`LD_*`, `DYLD_*`)
/// - Shell configuration (`IFS`, `SHELLOPTS`, etc.)
/// - Library search paths (`LIBRARY_PATH`, `LD_LIBRARY_PATH`)
/// - Container configuration (`DOCKER_HOST`, `CONTAINER_*`)
fn is_dangerous_env_var_name(name: &str) -> bool {
    let name_upper = name.to_uppercase();

    // Block the main PATH variable (but allow things like NODE_PATH, GOPATH)
    if name_upper == "PATH" {
        return true;
    }

    // Block dynamic linker variables (Linux)
    if name_upper.starts_with("LD_")
        || name_upper == "LD_PRELOAD"
        || name_upper == "LD_LIBRARY_PATH"
    {
        return true;
    }

    // Block dynamic linker variables (macOS)
    if name_upper.starts_with("DYLD_") {
        return true;
    }

    // Block shell configuration variables
    if name_upper == "IFS"
        || name_upper == "SHELLOPTS"
        || name_upper == "BASH_ENV"
        || name_upper == "ENV"
        || name_upper == "PS1"
        || name_upper == "PS2"
        || name_upper == "PROMPT_COMMAND"
    {
        return true;
    }

    // Block library search paths
    if name_upper == "LIBRARY_PATH" || name_upper == "LIBPATH" {
        return true;
    }

    // Block PYTHONPATH specifically (allows arbitrary module loading)
    // but allow other PYTHON* variables like PYTHON_VERSION
    if name_upper == "PYTHONPATH" || name_upper == "PYTHONHOME" {
        return true;
    }

    // Block container-related variables that could be used for escape
    if name_upper == "DOCKER_HOST"
        || name_upper.starts_with("CONTAINER_")
        || name_upper.starts_with("DOCKER_")
        || name_upper.starts_with("PODMAN_")
    {
        return true;
    }

    false
}

/// Check if an environment variable value is safe to pass through.
///
/// This validates that a value doesn't contain characters that could
/// be used for command injection or path traversal.
fn is_safe_env_var_value(value: &str) -> bool {
    // Check for shell metacharacters that could be used for injection
    let dangerous_chars = ['$', '`', '\\', '\n', '\r', '\0'];
    for c in dangerous_chars {
        if value.contains(c) {
            return false;
        }
    }

    // Check for suspicious patterns
    if value.contains("..") || value.contains('~') {
        return false;
    }

    true
}

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

        let tool_manager = ToolManager::with_repo(config.repository_root.clone());

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

        // Check for macOS-specific commands (when running on macOS)
        if cfg!(target_os = "macos") {
            if let Some(binary) = is_macos_specific_command(agent_command) {
                return Err(ContainerError::Other(format!(
                    "The command '{binary}' is macOS-specific and won't work in a Linux container. \
                    Consider using --security-mode user-account instead, or ensure you're using cross-platform tools."
                )));
            }

            // Additionally check if the first argument is a Mach-O binary
            if let Some(first_arg) = argv.first() {
                let binary_path = Path::new(first_arg.as_str());
                if binary_path.is_file() && is_macho_binary(binary_path) {
                    return Err(ContainerError::Other(format!(
                        "The binary '{}' appears to be a macOS Mach-O executable and won't work in a Linux container. \
                        Consider using --security-mode user-account instead.",
                        first_arg
                    )));
                }
            }
        }

        // Build volume mounts (avoiding duplicate targets)
        let mut mounts = self.volume_manager.get_mounts()?;
        let mut seen_targets: HashSet<String> = mounts.iter().map(|m| m.target.clone()).collect();

        // Discover and add tool mounts
        let tool_mounts = self.tool_manager.discover_tool_mounts();
        for tool_mount in tool_mounts {
            if seen_targets.insert(tool_mount.target.clone()) {
                mounts.push(tool_mount.to_mount());
            }
        }

        // Add Codex-specific volume mounts if Codex is detected
        if codex::is_codex_command(agent_command) {
            for (source, target) in codex::get_codex_volume_mounts() {
                if seen_targets.insert(target.clone()) {
                    mounts.push(crate::container::engine::Mount::read_only(
                        source.display().to_string(),
                        target,
                    ));
                }
            }
        }

        // Validate working directory
        if let Some(ref wd) = options.working_dir {
            validate_working_dir(wd)?;
        }

        // Build environment variables
        let mut container_env = Vec::new();

        // Add environment variables from agent config
        for (key, value) in env_vars {
            // Skip dangerous environment variables
            if is_dangerous_env_var_name(key) {
                continue;
            }

            // Validate value safety
            if !is_safe_env_var_value(value) {
                return Err(ContainerError::Other(format!(
                    "Environment variable value for '{key}' contains dangerous characters"
                )));
            }

            container_env.push((key.clone(), value.clone()));
        }

        // Add environment variables from execution options
        for (key, value) in &options.env_vars {
            // Skip dangerous environment variables
            if is_dangerous_env_var_name(key) {
                continue;
            }

            // Validate value safety
            if !is_safe_env_var_value(value) {
                return Err(ContainerError::Other(
                    format!("Execution option environment variable value for '{key}' contains dangerous characters"),
                ));
            }

            container_env.push((key.clone(), value.clone()));
        }

        // Add environment variables from tool manager
        for (key, value) in crate::container::tool::ToolManager::get_env_vars() {
            // Tool manager variables are considered trusted, but still filter dangerous names
            if !is_dangerous_env_var_name(&key) {
                container_env.push((key, value));
            }
        }

        // Add Codex-specific environment variables if Codex is detected
        if codex::is_codex_command(agent_command) {
            for (key, value) in codex::get_codex_container_env() {
                if !is_dangerous_env_var_name(&key) {
                    container_env.push((key, value));
                }
            }
        }

        // Set working directory
        let workdir = options
            .working_dir
            .as_ref()
            .map_or_else(|| "/workspace".to_string(), |wd| format!("/workspace/{wd}"));

        // Detect and publish ports that the command might use
        let detected_ports = detect_ports_from_command(&argv);
        let published_ports: Vec<PortMapping> = detected_ports
            .into_iter()
            .map(PortMapping::auto_allocate)
            .collect();

        // Build container command with shell initialization
        let shell_init = self.tool_manager.get_shell_init_script();
        let command = if shell_init.is_empty() {
            // No shell init needed, use command directly
            let mut cmd = argv;
            cmd.push("<PROMPT>".to_string());
            cmd
        } else {
            // Wrap command with bash and shell init script
            let quoted_command = argv
                .iter()
                .map(|arg| shell_words::quote(arg).into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            let full_script = format!("{shell_init}\ncd {workdir}\n{quoted_command} \"$@\"");
            vec![
                "bash".to_string(),
                "-c".to_string(),
                full_script,
                "<PROMPT>".to_string(),
            ]
        };

        // Build container run options
        let run_opts = RunOptions {
            image: self.config.image.clone(),
            command,
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
