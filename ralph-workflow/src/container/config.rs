//! Container configuration types

#[cfg(feature = "security-mode")]
use crate::container::error::{ContainerError, ContainerResult};
#[cfg(feature = "security-mode")]
use crate::container::EngineType;
#[cfg(feature = "security-mode")]
use std::path::PathBuf;
#[cfg(feature = "security-mode")]
use std::str::FromStr;

/// Check if a relative path component is safe for use as a working directory.
///
/// This validates that a path string (intended to be relative to /workspace)
/// doesn't contain dangerous patterns like path traversal.
#[cfg(feature = "security-mode")]
fn is_safe_working_dir(path: &str) -> bool {
    // Reject absolute paths
    if path.starts_with('/') {
        return false;
    }

    // Reject path traversal
    if path.contains("..") {
        return false;
    }

    // Reject null bytes
    if path.contains('\0') {
        return false;
    }

    // Reject shell metacharacters that could be used for injection
    let dangerous_chars = ['$', '`', '\\', '\n', '\r'];
    for c in dangerous_chars {
        if path.contains(c) {
            return false;
        }
    }

    true
}

/// Validate a working directory path for use in container execution.
///
/// Returns an error if the path is not safe.
#[cfg(feature = "security-mode")]
pub fn validate_working_dir(path: &str) -> ContainerResult<()> {
    if is_safe_working_dir(path) {
        Ok(())
    } else {
        Err(ContainerError::InvalidConfig(format!(
            "Invalid working directory '{path}': must be a relative path without .. or shell metacharacters"
        )))
    }
}

/// Security mode for agent isolation
///
/// Defines how the agent is isolated from the host system.
#[cfg(feature = "security-mode")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMode {
    /// Run agent in a container (Docker/Podman)
    Container,
    /// Run agent as a dedicated user account
    UserAccount,
    /// No isolation (run as current user)
    None,
    /// Auto-detect based on platform and availability
    Auto,
}

#[cfg(feature = "security-mode")]
impl SecurityMode {
    /// Get the default security mode for the current platform
    pub const fn default_for_platform() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::Container
        }
        #[cfg(target_os = "macos")]
        {
            Self::UserAccount
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Self::None
        }
    }
}

#[cfg(feature = "security-mode")]
impl FromStr for SecurityMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "container" => Ok(Self::Container),
            "user-account" | "useraccount" | "user" => Ok(Self::UserAccount),
            "none" => Ok(Self::None),
            _ => Err(format!(
                "Invalid security mode: {s}. Valid options: auto, container, user-account, none"
            )),
        }
    }
}

#[cfg(feature = "security-mode")]
impl std::fmt::Display for SecurityMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Container => write!(f, "container"),
            Self::UserAccount => write!(f, "user-account"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Container mode configuration
#[cfg(feature = "security-mode")]
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

#[cfg(feature = "security-mode")]
impl ContainerConfig {
    /// Create a new container configuration
    pub fn new(repository_root: PathBuf, agent_dir: PathBuf, image: String) -> Self {
        Self {
            enabled: true,
            engine: EngineType::Auto,
            image,
            network_enabled: true,
            repository_root,
            agent_dir,
            config_dir: dirs::home_dir().map(|d| d.join(".config").join("ralph")),
        }
    }

    /// Set whether container mode is enabled
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the container engine type
    pub const fn with_engine(mut self, engine: EngineType) -> Self {
        self.engine = engine;
        self
    }

    /// Set whether network is enabled
    pub const fn with_network(mut self, enabled: bool) -> Self {
        self.network_enabled = enabled;
        self
    }
}

#[cfg(feature = "security-mode")]
impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: EngineType::Auto,
            image: "ralph-agent:latest".to_string(),
            network_enabled: true,
            repository_root: PathBuf::from("."),
            agent_dir: PathBuf::from(".agent"),
            config_dir: dirs::home_dir().map(|d| d.join(".config").join("ralph")),
        }
    }
}

/// Container execution options
#[cfg(feature = "security-mode")]
#[derive(Debug, Clone, Default)]
pub struct ExecutionOptions {
    /// Environment variables to pass to the container
    pub env_vars: Vec<(String, String)>,
    /// Working directory inside the container (relative to /workspace)
    pub working_dir: Option<String>,
}
