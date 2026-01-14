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

pub mod config;
pub mod engine;
pub mod error;
pub mod executor;
pub mod image;
pub mod network;
pub mod volume;

// Re-export main types for convenience
pub use engine::{ContainerEngine, EngineType};
pub use error::ContainerError;
pub use executor::ContainerExecutor;
