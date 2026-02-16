//! Integration tests for cloud mode functionality.
//!
//! This module contains tests verifying cloud support behavior:
//!
//! - `disabled_mode`: Verify cloud mode disabled = zero behavior change (CLI unchanged)
//! - `enabled_mode`: Verify cloud mode with MockCloudReporter works correctly
//! - `push_flow`: Verify commit->push sequencing in cloud mode
//!
//! These tests follow the integration test style guide:
//! - Use MockCloudReporter for cloud API calls (no real HTTP)
//! - Use MemoryWorkspace for filesystem operations
//! - Use MockProcessExecutor for git operations
//! - Test observable behavior, not implementation details

mod disabled_mode;
mod enabled_mode;
mod push_flow;
