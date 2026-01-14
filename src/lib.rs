//! Ralph library
//!
//! This library exposes Ralph's internal modules for integration testing.

#![deny(unsafe_code)]

mod agents;
mod app;
mod banner;
mod checkpoint;
mod cli;
mod colors;
mod config;
mod container;
mod files;
mod git_helpers;
mod guidelines;
mod json_parser;
mod language_detector;
mod logger;
mod output;
mod phases;
mod pipeline;
mod platform;
mod prompts;
mod review_metrics;
mod templates;
mod timer;
mod utils;

// Public exports for integration tests
pub use container::{
    ContainerEngine, ContainerExecutor, EngineType, SecurityMode, ToolManager, UserAccountExecutor,
    UserAccountExecutor as UserAccountExecutorAlias, VolumeManager,
};

// Re-export specific container types for testing
pub use container::config::ContainerConfig;
pub use container::port::PortMapping;
pub use container::tool::ToolMount;
pub use container::user_executor::ExecutionResult;
