//! Container engine abstraction
//!
//! Provides a unified interface over Docker and Podman runtimes.

use crate::container::error::{ContainerError, ContainerResult};
use std::process::{Command, Output};

/// Container engine type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineType {
    /// Auto-detect available engine (Docker first, Podman fallback)
    Auto,
    /// Use Docker specifically
    Docker,
    /// Use Podman specifically
    Podman,
}

impl EngineType {
    /// Get the binary name for this engine type
    pub fn binary_name(&self) -> &str {
        match self {
            EngineType::Docker => "docker",
            EngineType::Podman => "podman",
            EngineType::Auto => "docker", // Default to docker for auto
        }
    }

    /// Get all engine types to try (in order of preference)
    pub fn detection_order(&self) -> Vec<EngineType> {
        match self {
            EngineType::Auto => vec![EngineType::Docker, EngineType::Podman],
            EngineType::Docker => vec![EngineType::Docker],
            EngineType::Podman => vec![EngineType::Podman],
        }
    }
}

/// Container engine abstraction
///
/// Provides a unified interface for running containers with either Docker or Podman.
pub struct ContainerEngine {
    /// The detected engine type
    engine_type: EngineType,
    /// The binary name (docker or podman)
    binary: String,
}

impl ContainerEngine {
    /// Detect and create a container engine
    ///
    /// Tries to find an available container runtime in the following order:
    /// 1. Docker (if engine_type is Auto or Docker)
    /// 2. Podman (if engine_type is Auto or Podman)
    pub fn detect(engine_type: EngineType) -> ContainerResult<Self> {
        for candidate in engine_type.detection_order() {
            let binary = candidate.binary_name();
            if Self::is_available(binary) {
                return Ok(Self {
                    engine_type: candidate,
                    binary: binary.to_string(),
                });
            }
        }

        // No engine found
        let order = engine_type.detection_order();
        let names: Vec<&str> = order.iter().map(|e| e.binary_name()).collect();
        Err(ContainerError::RuntimeNotFound(names.join(" or ")))
    }

    /// Create a container engine with a specific type
    ///
    /// This doesn't check if the engine is available. Use [`detect`] for auto-detection.
    pub fn new(engine_type: EngineType) -> Self {
        let binary = engine_type.binary_name().to_string();
        Self {
            engine_type,
            binary,
        }
    }

    /// Check if a container runtime binary is available
    fn is_available(binary: &str) -> bool {
        Command::new(binary)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Get the engine type
    pub fn engine_type(&self) -> EngineType {
        self.engine_type
    }

    /// Get the binary name
    pub fn binary(&self) -> &str {
        &self.binary
    }

    /// Check if this engine is available
    pub fn available(&self) -> bool {
        Self::is_available(&self.binary)
    }

    /// Run a container with the given configuration
    ///
    /// Builds and executes a container run command with the specified options.
    pub fn run_container(&self, opts: &RunOptions) -> ContainerResult<Output> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("run");

        // Remove container after exit
        cmd.arg("--rm");

        // Network configuration
        if opts.network_enabled {
            // Enable network (default)
        } else {
            cmd.arg("--network=none");
        }

        // Volume mounts
        for mount in &opts.mounts {
            cmd.arg("--mount");
            let mount_arg = if mount.read_only {
                format!("type=bind,source={},target={},readonly", mount.source, mount.target)
            } else {
                format!("type=bind,source={},target={}", mount.source, mount.target)
            };
            cmd.arg(mount_arg);
        }

        // Environment variables
        for (key, value) in &opts.env_vars {
            cmd.arg("--env");
            cmd.arg(format!("{}={}", key, value));
        }

        // Working directory
        if let Some(workdir) = &opts.working_dir {
            cmd.args(["-w", workdir]);
        }

        // Interactive flag for stdin
        cmd.arg("-i");

        // Image
        cmd.arg(&opts.image);

        // Command and arguments
        cmd.args(&opts.command);

        // Execute
        let output = cmd.output()?;

        Ok(output)
    }

    /// Run a container and capture stdout/stderr
    pub fn run_and_capture(
        &self,
        opts: &RunOptions,
        stdin: Option<&[u8]>,
    ) -> ContainerResult<(String, String, i32)> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("run");
        cmd.arg("--rm");

        // Network configuration
        if !opts.network_enabled {
            cmd.arg("--network=none");
        }

        // Volume mounts
        for mount in &opts.mounts {
            cmd.arg("--mount");
            let mount_arg = if mount.read_only {
                format!("type=bind,source={},target={},readonly", mount.source, mount.target)
            } else {
                format!("type=bind,source={},target={}", mount.source, mount.target)
            };
            cmd.arg(mount_arg);
        }

        // Environment variables
        for (key, value) in &opts.env_vars {
            cmd.arg("--env");
            cmd.arg(format!("{}={}", key, value));
        }

        // Working directory
        if let Some(workdir) = &opts.working_dir {
            cmd.args(["-w", workdir]);
        }

        // Stdin handling
        if stdin.is_some() {
            cmd.arg("-i");
        }

        // Image
        cmd.arg(&opts.image);

        // Command and arguments
        cmd.args(&opts.command);

        // Execute with stdin if provided
        use std::io::Write;
        use std::process::Stdio;

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        if let Some(stdin_data) = stdin {
            if let Some(mut stdin_pipe) = child.stdin.take() {
                stdin_pipe.write_all(stdin_data)?;
            }
        }

        let output = child.wait_with_output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(1);

        Ok((stdout, stderr, exit_code))
    }
}

/// Options for running a container
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Container image to use
    pub image: String,
    /// Command to run inside the container
    pub command: Vec<String>,
    /// Volume mounts
    pub mounts: Vec<Mount>,
    /// Environment variables
    pub env_vars: Vec<(String, String)>,
    /// Working directory inside container
    pub working_dir: Option<String>,
    /// Whether network is enabled
    pub network_enabled: bool,
}

/// Volume mount configuration
#[derive(Debug, Clone)]
pub struct Mount {
    /// Source path on host
    pub source: String,
    /// Target path inside container
    pub target: String,
    /// Whether mount is read-only
    pub read_only: bool,
}

impl Mount {
    /// Create a new volume mount
    pub fn new(source: String, target: String) -> Self {
        Self {
            source,
            target,
            read_only: false,
        }
    }

    /// Create a read-only mount
    pub fn read_only(source: String, target: String) -> Self {
        Self {
            source,
            target,
            read_only: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_type_binary_name() {
        assert_eq!(EngineType::Docker.binary_name(), "docker");
        assert_eq!(EngineType::Podman.binary_name(), "podman");
        assert_eq!(EngineType::Auto.binary_name(), "docker");
    }

    #[test]
    fn test_detection_order() {
        let auto_order = EngineType::Auto.detection_order();
        assert_eq!(auto_order, vec![EngineType::Docker, EngineType::Podman]);

        let docker_order = EngineType::Docker.detection_order();
        assert_eq!(docker_order, vec![EngineType::Docker]);

        let podman_order = EngineType::Podman.detection_order();
        assert_eq!(podman_order, vec![EngineType::Podman]);
    }

    #[test]
    fn test_mount_new() {
        let mount = Mount::new("/host".to_string(), "/container".to_string());
        assert_eq!(mount.source, "/host");
        assert_eq!(mount.target, "/container");
        assert!(!mount.read_only);
    }

    #[test]
    fn test_mount_read_only() {
        let mount = Mount::read_only("/host".to_string(), "/container".to_string());
        assert_eq!(mount.source, "/host");
        assert_eq!(mount.target, "/container");
        assert!(mount.read_only);
    }
}
