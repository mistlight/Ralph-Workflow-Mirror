//! Container configuration types

use crate::container::EngineType;
use std::path::PathBuf;

/// Container mode configuration
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    /// Whether container mode is enabled
    pub enabled: bool,
    /// Container engine type (auto-detect, docker, podman)
    pub engine: EngineType,
    /// Container image to use
    pub image: String,
    /// Whether network access is enabled
    pub network_enabled: bool,
    /// Repository root path (mounted to /workspace)
    pub repository_root: PathBuf,
    /// Path to .agent directory (for orchestrator communication)
    pub agent_dir: PathBuf,
    /// User's home directory config path (mounted read-only)
    pub config_dir: Option<PathBuf>,
}

impl ContainerConfig {
    /// Create a new container configuration
    pub fn new(
        repository_root: PathBuf,
        agent_dir: PathBuf,
        image: String,
    ) -> Self {
        Self {
            enabled: true,
            engine: EngineType::Auto,
            image,
            network_enabled: true,
            repository_root,
            agent_dir,
            config_dir: dirs::home_dir()
                .map(|d| d.join(".config").join("ralph")),
        }
    }

    /// Set whether container mode is enabled
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the container engine type
    pub fn with_engine(mut self, engine: EngineType) -> Self {
        self.engine = engine;
        self
    }

    /// Set whether network is enabled
    pub fn with_network(mut self, enabled: bool) -> Self {
        self.network_enabled = enabled;
        self
    }

    /// Set the config directory path
    pub fn with_config_dir(mut self, config_dir: Option<PathBuf>) -> Self {
        self.config_dir = config_dir;
        self
    }
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: EngineType::Auto,
            image: "ralph-agent:latest".to_string(),
            network_enabled: true,
            repository_root: PathBuf::from("."),
            agent_dir: PathBuf::from(".agent"),
            config_dir: dirs::home_dir()
                .map(|d| d.join(".config").join("ralph")),
        }
    }
}

/// Container execution options
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Environment variables to pass to the container
    pub env_vars: Vec<(String, String)>,
    /// Working directory inside the container (relative to /workspace)
    pub working_dir: Option<String>,
    /// Command timeout in seconds
    pub timeout: Option<u64>,
}

impl ExecutionOptions {
    /// Create new execution options
    pub fn new() -> Self {
        Self {
            env_vars: Vec::new(),
            working_dir: None,
            timeout: None,
        }
    }

    /// Add an environment variable
    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.env_vars.push((key, value));
        self
    }

    /// Set the working directory
    pub fn with_working_dir(mut self, dir: String) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout = Some(timeout_secs);
        self
    }
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self::new()
    }
}
