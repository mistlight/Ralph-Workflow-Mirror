//! Cloud integration for containerized deployments.
//!
//! This module provides abstractions for ralph-workflow to run in cloud
//! environments with external orchestration. All cloud functionality is:
//!
//! - **Environment-variable configured only** (not in config files)
//! - **Disabled by default**
//! - **Invisible to CLI users** (no CLI flags, no help text)
//! - **Purely additive** (zero behavior change when disabled)
//!
//! ## Architecture
//!
//! Cloud support is trait-based for testability:
//! - `CloudReporter` - Abstract interface for progress reporting
//! - `NoopCloudReporter` - Default (does nothing)
//! - `HttpCloudReporter` - Production HTTP API client
//! - `MockCloudReporter` - Testing (captures calls)

pub mod heartbeat;
pub mod redaction;
pub mod reporter;
pub mod types;

pub use heartbeat::HeartbeatGuard;
pub use reporter::{CloudReporter, HttpCloudReporter, NoopCloudReporter};
pub use types::{CloudError, PipelineResult, ProgressEventType, ProgressUpdate};

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;
#[cfg(any(test, feature = "test-utils"))]
pub use mock::MockCloudReporter;
