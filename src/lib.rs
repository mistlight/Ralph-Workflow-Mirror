//! Ralph library
//!
//! This library exposes Ralph's internal modules for integration testing.

#![deny(unsafe_code)]

mod container;

// Public exports for integration tests
pub use container::tool::ToolManager;
pub use container::volume::VolumeManager;
pub use container::{
    ContainerEngine, ContainerExecutor, EngineType, SecurityMode, UserAccountExecutor,
    UserAccountExecutor as UserAccountExecutorAlias,
};

// Re-export specific container types for testing
pub use container::config::ContainerConfig;
pub use container::port::{detect_ports_from_command, PortMapping};
pub use container::tool::ToolMount;
pub use container::user_executor::ExecutionResult;

// When build-image feature is enabled, export image types to avoid dead code warnings
#[cfg(feature = "build-image")]
pub use container::image::{detect_project_stack, ContainerImage};
