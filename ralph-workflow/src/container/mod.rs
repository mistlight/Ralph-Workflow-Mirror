//! Container-based security model for Ralph Workflow
//!
//! This module implements a container-based security model that runs AI agents
//! in isolated Docker/Podman containers while maintaining full functionality
//! and user experience.
//!
//! # Architecture
//!
//! - [`engine`]: Abstraction over Docker/Podman runtimes
//! - [`config`]: Container configuration types
//! - [`executor`]: Command execution wrapper that translates agent commands into container runs
//! - [`volume`]: Volume mount management for filesystem access control
//! - [`network`]: Network configuration for API access
//! - [`image`]: Container image management and selection
//!
//! # Security Model
//!
//! Agents run in containers with:
//! - Controlled filesystem access (repository mounted to `/workspace`)
//! - Network access for API calls (can be disabled)
//! - Environment variable passing for API keys and configs
//! - Proper isolation from host system
//!
//! # Usage
//!
//! ```ignore
//! use crate::container::{ContainerEngine, ContainerExecutor};
//!
//! // Auto-detect available engine (Docker or Podman)
//! let engine = ContainerEngine::detect()?;
//!
//! // Execute a command in a container
//! let result = ContainerExecutor::run(&engine, &cmd, &config)?;
//! ```

#[cfg(feature = "security-mode")]
pub mod codex;
#[cfg(feature = "security-mode")]
pub mod config;
#[cfg(feature = "security-mode")]
pub mod engine;
pub mod error;
#[cfg(feature = "security-mode")]
pub mod executor;
#[cfg(feature = "build-image")]
pub mod image;
pub mod network;
#[cfg(feature = "security-mode")]
pub mod port;
#[cfg(feature = "security-mode")]
pub mod tool;
#[cfg(feature = "security-mode")]
pub mod user_executor;
#[cfg(feature = "security-mode")]
pub mod volume;

// Re-export main types for convenience
#[cfg(feature = "security-mode")]
pub use config::SecurityMode;
#[cfg(feature = "security-mode")]
pub use engine::{ContainerEngine, EngineType};
#[cfg(feature = "security-mode")]
pub use executor::ContainerExecutor;
#[cfg(feature = "security-mode")]
pub use user_executor::UserAccountExecutor;
