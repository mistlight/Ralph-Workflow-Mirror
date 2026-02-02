//! Process execution abstraction for dependency injection.
//!
//! This module provides a trait-based abstraction for executing external processes,
//! allowing production code to use real processes and test code to use mocks.
//! This follows the same pattern as [`crate::workspace::Workspace`] for dependency injection.
//!
//! # Purpose
//!
//! - Production: [`RealProcessExecutor`] executes actual commands using `std::process::Command`
//! - Tests: `MockProcessExecutor` captures calls and returns controlled results (with `test-utils` feature)
//!
//! # Benefits
//!
//! - Test isolation: Tests don't spawn real processes
//! - Determinism: Tests produce consistent results
//! - Speed: Tests run faster without subprocess overhead
//! - Mockability: Full control over process behavior in tests
//!
//! # Key Types
//!
//! - [`ProcessExecutor`] - The trait abstraction for process execution
//! - [`AgentSpawnConfig`] - Configuration for spawning agent processes
//! - [`AgentChildHandle`] - Handle to a spawned agent with streaming output
//! - [`ProcessOutput`] - Captured output from a completed process
//!
//! # Testing with MockProcessExecutor
//!
//! The `test-utils` feature enables `MockProcessExecutor` for integration tests:
//!
//! ```ignore
//! use ralph_workflow::{MockProcessExecutor, ProcessExecutor};
//!
//! // Create a mock that returns success for 'git' commands
//! let executor = MockProcessExecutor::new()
//!     .with_output("git", "On branch main\nnothing to commit");
//!
//! // Execute command (captured, returns mock result)
//! let result = executor.execute("git", &["status"], &[], None)?;
//! assert!(result.status.success());
//!
//! // Verify the call was captured
//! assert_eq!(executor.execute_count(), 1);
//! ```
//!
//! # See Also
//!
//! - [`crate::workspace::Workspace`] - Similar abstraction for filesystem operations

mod types;
mod executor_trait;
mod real;
#[cfg(any(test, feature = "test-utils"))]
mod mock;

// Re-export all public types
pub use types::{
    AgentChild, AgentChildHandle, AgentCommandResult, AgentSpawnConfig, ProcessOutput,
    RealAgentChild,
};
pub use executor_trait::ProcessExecutor;
pub use real::RealProcessExecutor;

#[cfg(any(test, feature = "test-utils"))]
pub use mock::{MockAgentChild, MockProcessExecutor};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_executor_can_be_created() {
        let executor = RealProcessExecutor::new();
        // Can't test actual execution without real commands
        let _ = executor;
    }

    #[test]
    fn test_real_executor_execute_basic() {
        let executor = RealProcessExecutor::new();
        // Use 'echo' command which should exist on all Unix systems
        let result = executor.execute("echo", &["hello"], &[], None);
        // Should succeed
        assert!(result.is_ok());
        if let Ok(output) = result {
            assert!(output.status.success());
            assert_eq!(output.stdout.trim(), "hello");
        }
    }
}
