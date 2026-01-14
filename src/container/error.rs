//! Error types for container operations

use std::io;

/// Result type for container operations
pub(crate) type ContainerResult<T> = Result<T, ContainerError>;

/// Errors that can occur during container operations
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// Container runtime not found
    #[error("Container runtime '{0}' not found. Please install {0} or use --no-container-mode")]
    RuntimeNotFound(String),

    /// Container runtime execution failed
    #[error("Container runtime execution failed: {0}")]
    ExecutionFailed(String),

    /// Container image not found
    #[error("Container image '{0}' not found. Run: {1} pull {0}")]
    ImageNotFound(String, String),

    /// Invalid container configuration
    #[error("Invalid container configuration: {0}")]
    InvalidConfig(String),

    /// Volume mount error
    #[error("Volume mount error: {0}")]
    VolumeMount(String),

    /// Network configuration error
    #[error("Network configuration error: {0}")]
    NetworkConfig(String),

    /// IO error during container operation
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Permission denied (e.g., user not in docker group)
    #[error("Permission denied. Try: sudo usermod -aG docker $USER && newgrp docker")]
    PermissionDenied,

    /// Container command failed with exit code
    #[error("Container command failed with exit code {0}")]
    CommandFailed(i32),

    /// Timeout waiting for container
    #[error("Container operation timed out")]
    Timeout,

    /// Other container error
    #[error("Container error: {0}")]
    Other(String),
}

impl ContainerError {
    /// Check if this error indicates the runtime is not available
    pub(crate) fn is_runtime_unavailable(&self) -> bool {
        matches!(self, ContainerError::RuntimeNotFound(_))
    }

    /// Check if this error is recoverable by falling back to non-container mode
    pub(crate) fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ContainerError::RuntimeNotFound(_)
                | ContainerError::PermissionDenied
                | ContainerError::ImageNotFound(_, _)
        )
    }

    /// Get user-friendly recovery suggestion
    pub(crate) fn recovery_suggestion(&self) -> Option<String> {
        match self {
            ContainerError::RuntimeNotFound(runtime) => Some(format!(
                "Install {} or use --no-container-mode to run without container isolation",
                runtime
            )),
            ContainerError::PermissionDenied => Some(
                "Add your user to the docker group: sudo usermod -aG docker $USER".to_string(),
            ),
            ContainerError::ImageNotFound(image, runtime) => {
                Some(format!("Pull the image: {} pull {}", runtime, image))
            }
            _ => None,
        }
    }
}
