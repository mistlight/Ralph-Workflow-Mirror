//! Error types for container operations

use std::io;

/// Result type for container operations
pub type ContainerResult<T> = Result<T, ContainerError>;

/// Errors that can occur during container operations
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// Container runtime not found
    #[error("Container runtime '{0}' not found. Please install {0} or use --no-container-mode")]
    RuntimeNotFound(String),

    /// Container image pull failed
    #[error("Failed to pull container image '{image}': {error}")]
    ImagePullFailed { image: String, error: String },

    /// Invalid container configuration
    #[error("Invalid container configuration: {0}")]
    InvalidConfig(String),

    /// Volume mount error
    #[error("Volume mount error: {0}")]
    VolumeMount(String),

    /// IO error during container operation
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Other container error
    #[error("Container error: {0}")]
    Other(String),
}
