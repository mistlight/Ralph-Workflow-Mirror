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
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// macOS Homebrew installation paths
const MACOS_HOMEBREW_PATHS: &[&str] = &[
    "/opt/homebrew/bin", // Apple Silicon
    "/opt/homebrew/sbin",
    "/usr/local/bin", // Intel
    "/usr/local/sbin",
];

/// Homebrew environment variables to preserve
const HOMEBREW_ENV_VARS: &[&str] = &[
    "HOMEBREW_PREFIX",
    "HOMEBREW_CELLAR",
    "HOMEBREW_REPOSITORY",
    "HOMEBREW_SHELLENV_PATH",
];

/// Language manager shims that should be in PATH
const LANGUAGE_MANAGER_SHIMS: &[&str] = &[
    // Python
    ".pyenv/shims",
    ".local/share/pyenv/shims",
    // Ruby
    ".rbenv/shims",
    ".local/share/rbenv/shims",
    // Node.js
    ".nodenv/shims",
    ".local/share/nodenv/shims",
    ".nvm/versions/node", // For direct node version access
    // Go
    ".local/share/goenv/shims",
    // Java
    ".jenv/shims",
    ".sdkman/candidates/java", // For direct Java version access
    // Elixir
    ".mix/escripts",
];

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
}

#[cfg(test)]
/// User account information
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// User name
    pub name: String,
    /// User info string from `id` command
    pub _info: String,
}

impl UserAccountExecutor {
    /// Create a new user account executor
    ///
    /// Verifies the user exists and can be used for command execution.
    pub fn new(
        workspace_path: PathBuf,
        _agent_dir: PathBuf,
        user_name: Option<String>,
    ) -> ContainerResult<Self> {
        let user_name = user_name.unwrap_or_else(|| DEFAULT_AGENT_USER.to_string());

        // Verify the user exists
        if !Self::user_exists(&user_name)? {
            return Err(ContainerError::Other(format!(
                "User account '{user_name}' does not exist.\n\n\
                To set up user-account security mode:\n\
                1. Run: ralph --setup-security\n\
                2. Or manually: sudo useradd -m -s /bin/bash {user_name}\n\
                3. Then: sudo echo '{user_name} ALL=(ALL) NOPASSWD: ALL' | sudo tee /etc/sudoers.d/ralph-agent\n\
                4. And: sudo chmod 440 /etc/sudoers.d/ralph-agent\n\n\
                For more information, run: ralph --security-check"
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
                let parent_dir = workspace_path
                    .parent()
                    .map_or_else(|| ".".to_string(), |p| p.display().to_string());

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
        // Validate prompt for null bytes which are universally invalid in command arguments
        if prompt.contains('\0') {
            return Err(ContainerError::Other(
                "Prompt contains null byte which is invalid for command execution".to_string(),
            ));
        }

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

        // Build the enhanced environment with platform-specific paths
        let enhanced_env = self.build_enhanced_environment(env_vars, options)?;

        // Build the sudo command
        let mut cmd = Command::new("sudo");
        cmd.arg("-u").arg(&self.user_name);

        // Set working directory
        cmd.arg("--cwd").arg(&workdir);

        // Set environment variables
        for (key, value) in &enhanced_env {
            cmd.arg("--env");
            cmd.arg(format!("{key}={value}"));
        }

        // Add the actual command
        cmd.arg(&argv[0]);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }

        // Add prompt argument
        cmd.arg(prompt);

        // Execute and capture output
        let output = cmd
            .output()
            .map_err(|e| ContainerError::Other(format!("Failed to execute command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Build enhanced environment with platform-specific paths
    fn build_enhanced_environment(
        &self,
        env_vars: &HashMap<String, String>,
        options: &ExecutionOptions,
    ) -> ContainerResult<HashMap<String, String>> {
        let mut enhanced = HashMap::new();

        // First, add all provided environment variables
        for (key, value) in env_vars {
            // Validate
            if key.contains('\0') || value.contains('\0') {
                return Err(ContainerError::Other(
                    "Environment variable contains null byte which is invalid".to_string(),
                ));
            }
            if key.contains('\n')
                || value.contains('\n')
                || key.contains('\r')
                || value.contains('\r')
            {
                return Err(ContainerError::Other(
                    "Environment variable contains newline which is invalid for sudo --env"
                        .to_string(),
                ));
            }
            if key.contains('=') {
                return Err(ContainerError::Other(
                    "Environment variable name contains '=' which is invalid".to_string(),
                ));
            }
            enhanced.insert(key.clone(), value.clone());
        }

        // Add execution option environment variables
        for (key, value) in &options.env_vars {
            if key.contains('\0') || value.contains('\0') {
                return Err(ContainerError::Other(
                    "Execution option environment variable contains null byte which is invalid"
                        .to_string(),
                ));
            }
            if key.contains('\n')
                || value.contains('\n')
                || key.contains('\r')
                || value.contains('\r')
            {
                return Err(ContainerError::Other(
                    "Execution option environment variable contains newline which is invalid for sudo --env".to_string(),
                ));
            }
            if key.contains('=') {
                return Err(ContainerError::Other(
                    "Execution option environment variable name contains '=' which is invalid"
                        .to_string(),
                ));
            }
            enhanced.insert(key.clone(), value.clone());
        }

        // Get current user's home directory for language manager paths
        let current_home = env::var("HOME").unwrap_or_else(|_| {
            env::var("USER")
                .ok()
                .map(|user| format!("/home/{user}"))
                .unwrap_or_else(|| "/home/default".to_string())
        });

        // On macOS, inject Homebrew paths
        if cfg!(target_os = "macos") {
            // Add Homebrew to PATH if not already present
            let path = enhanced.entry("PATH".to_string()).or_insert_with(|| {
                env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
            });

            // Prepend Homebrew paths if they exist and aren't already in PATH
            for brew_path in MACOS_HOMEBREW_PATHS {
                if Path::new(brew_path).exists() && !path.contains(brew_path) {
                    *path = format!("{}:{}", brew_path, path.as_str());
                }
            }

            // Add language manager shims to PATH
            for shim_path in LANGUAGE_MANAGER_SHIMS {
                let full_path = format!("{}/{}", current_home, shim_path);
                if Path::new(&full_path).exists() && !path.contains(shim_path) {
                    *path = format!("{}:{}", full_path, path.as_str());
                }
            }

            // Preserve Homebrew environment variables
            for env_var in HOMEBREW_ENV_VARS {
                if let Ok(value) = env::var(env_var) {
                    enhanced.entry(env_var.to_string()).or_insert(value);
                }
            }

            // Preserve language-specific environment variables
            for (key, value) in env::vars() {
                let key_upper = key.to_uppercase();
                if key_upper.contains("NODE")
                    || key_upper.contains("NPM")
                    || key_upper.contains("PYTHON")
                    || key_upper.contains("RUBY")
                    || key_upper.contains("GEM")
                    || key_upper.contains("RBENV")
                    || key_upper.contains("RVM")
                    || key_upper.contains("JAVA")
                    || key_upper.contains("GO")
                    || key_upper.contains("GOPATH")
                    || key_upper.contains("GOROOT")
                    || key_upper.contains("CARGO")
                    || key_upper.contains("RUST")
                    || key_upper.contains("RUSTUP")
                    || key_upper.contains("PHP")
                    || key_upper.contains("COMPOSER")
                    || key_upper.contains("MIX")
                    || key_upper.contains("ELIXIR")
                    || key_upper.contains("ERL")
                    || key_upper.contains("GRADLE")
                    || key_upper.contains("MAVEN")
                    || key_upper.contains("M2")
                    || key_upper.contains("PIP")
                    || key_upper.contains("PYENV")
                    || key_upper.contains("VIRTUAL_ENV")
                    || key_upper.contains("CONDA")
                    || key_upper.contains("PERL")
                    || key_upper.contains("PERL5LIB")
                    || key_upper.contains("SCALA")
                    || key_upper.contains("SBT")
                    || key_upper.contains("NVM")
                    || key_upper.contains("NODE_VERSION")
                    || key_upper.contains("JENV")
                    || key_upper.contains("SDKMAN")
                    || key_upper.contains("SWIFT")
                    || key_upper.contains("SWIFTENV")
                {
                    enhanced.entry(key).or_insert(value);
                }
            }
        }

        // On Linux, also add language manager shims
        if cfg!(target_os = "linux") {
            let path = enhanced.entry("PATH".to_string()).or_insert_with(|| {
                env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
            });

            // Add language manager shims to PATH
            for shim_path in LANGUAGE_MANAGER_SHIMS {
                let full_path = format!("{}/{}", current_home, shim_path);
                if Path::new(&full_path).exists() && !path.contains(shim_path) {
                    *path = format!("{}:{}", full_path, path.as_str());
                }
            }

            // Preserve language-specific environment variables (same as macOS)
            for (key, value) in env::vars() {
                let key_upper = key.to_uppercase();
                if key_upper.contains("NODE")
                    || key_upper.contains("NPM")
                    || key_upper.contains("PYTHON")
                    || key_upper.contains("RUBY")
                    || key_upper.contains("GEM")
                    || key_upper.contains("RBENV")
                    || key_upper.contains("RVM")
                    || key_upper.contains("JAVA")
                    || key_upper.contains("GO")
                    || key_upper.contains("GOPATH")
                    || key_upper.contains("GOROOT")
                    || key_upper.contains("CARGO")
                    || key_upper.contains("RUST")
                    || key_upper.contains("RUSTUP")
                    || key_upper.contains("PHP")
                    || key_upper.contains("COMPOSER")
                    || key_upper.contains("MIX")
                    || key_upper.contains("ELIXIR")
                    || key_upper.contains("ERL")
                    || key_upper.contains("GRADLE")
                    || key_upper.contains("MAVEN")
                    || key_upper.contains("M2")
                    || key_upper.contains("PIP")
                    || key_upper.contains("PYENV")
                    || key_upper.contains("VIRTUAL_ENV")
                    || key_upper.contains("CONDA")
                    || key_upper.contains("PERL")
                    || key_upper.contains("PERL5LIB")
                    || key_upper.contains("SCALA")
                    || key_upper.contains("SBT")
                    || key_upper.contains("NVM")
                    || key_upper.contains("NODE_VERSION")
                    || key_upper.contains("JENV")
                    || key_upper.contains("SDKMAN")
                    || key_upper.contains("SWIFT")
                    || key_upper.contains("SWIFTENV")
                {
                    enhanced.entry(key).or_insert(value);
                }
            }
        }

        Ok(enhanced)
    }

    /// Check if a user account exists on the system
    pub fn user_exists(user_name: &str) -> ContainerResult<bool> {
        let output = Command::new("id").arg(user_name).output();

        match output {
            Ok(out) => Ok(out.status.success()),
            Err(_) => Ok(false),
        }
    }

    #[cfg(test)]
    /// Get information about a user account
    pub fn get_user_info(user_name: &str) -> ContainerResult<Option<UserInfo>> {
        let output = Command::new("id").arg(user_name).output();

        let output =
            output.map_err(|e| ContainerError::Other(format!("Failed to get user info: {e}")))?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse "uid=1001(ralph-agent) gid=1001(ralph-agent) groups=1001(ralph-agent)"
        let _info = stdout.trim();
        Ok(Some(UserInfo {
            name: user_name.to_string(),
            _info: _info.to_string(),
        }))
    }

    #[cfg(test)]
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
            ContainerError::InvalidConfig(format!("Failed to parse command: {cmd_str}"))
        })?;

        Ok(args)
    }

    /// Get the user name
    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    #[cfg(test)]
    /// Get the workspace path
    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }
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

#[cfg(test)]
impl ExecutionResult {
    /// Check if the execution was successful
    pub const fn is_success(&self) -> bool {
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

    #[test]
    fn test_workspace_path() {
        let temp = std::env::temp_dir();
        let workspace = temp.join("test-workspace");
        std::fs::create_dir_all(&workspace).ok();

        // Use current user which should exist
        let current_user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        let executor =
            UserAccountExecutor::new(workspace.clone(), temp.clone(), Some(current_user));

        // Only assert if executor creation succeeded
        // (it might fail if sudo is not configured)
        if let Ok(exec) = executor {
            assert_eq!(exec.workspace_path(), &workspace);
        }

        // Cleanup
        std::fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn test_get_user_info_root() {
        let info = UserAccountExecutor::get_user_info("root").unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "root");
    }

    #[test]
    fn test_verify_workspace_access() {
        let temp = std::env::temp_dir();
        let workspace = temp.join("test-workspace-access");
        std::fs::create_dir_all(&workspace).ok();

        // Use current user which should exist and have access
        let current_user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        let executor =
            UserAccountExecutor::new(workspace.clone(), temp.clone(), Some(current_user));

        // Only test verify_workspace_access if executor creation succeeded
        if let Ok(exec) = executor {
            let _ = exec.verify_workspace_access();
        }

        // Cleanup
        std::fs::remove_dir_all(&workspace).ok();
    }
}
