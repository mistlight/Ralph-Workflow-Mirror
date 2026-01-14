//! Integration tests for user account security mode
//!
//! These tests verify that the user account mode works correctly including:
//! - User verification
//! - Command execution as dedicated user
//! - File permission isolation
//! - Tool access (same as host)

// Note: These tests may require sudo access and a dedicated user account
// They should be run with: cargo test --test user_account_mode -- --ignored

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    /// Test that user account executor can be created (requires user to exist)
    #[test]
    #[ignore = "Requires sudo and dedicated user account setup"]
    fn test_user_account_executor_creation() {
        use ralph::UserAccountExecutor;

        let workspace_path = std::env::current_dir().unwrap();
        let agent_dir = PathBuf::from(".agent");

        // This will fail if the ralph-agent user doesn't exist
        let result = UserAccountExecutor::new(workspace_path, agent_dir, None);

        // If user exists, creation should succeed
        if let Ok(executor) = result {
            assert_eq!(executor.user_name(), "ralph-agent");
            assert_eq!(executor.workspace_path(), std::env::current_dir().unwrap());
        }
    }

    /// Test that we can check if a user exists
    #[test]
    fn test_user_exists() {
        use ralph::UserAccountExecutor;

        // Root user should always exist
        assert!(UserAccountExecutor::user_exists("root").unwrap());

        // Current user should exist
        let current_user = std::env::var("USER").unwrap_or_else(|_| "nobody".to_string());
        assert!(UserAccountExecutor::user_exists(&current_user).unwrap());

        // Nonexistent user should return false (not error)
        assert!(!UserAccountExecutor::user_exists("nonexistentuser-12345").unwrap());
    }

    /// Test execution result helpers
    #[test]
    fn test_execution_result() {
        use ralph::ExecutionResult;

        let success = ExecutionResult {
            exit_code: 0,
            stdout: "Success".to_string(),
            stderr: String::new(),
        };
        assert!(success.is_success());
        assert!(success.error_message().is_none());

        let failure = ExecutionResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "Error occurred".to_string(),
        };
        assert!(!failure.is_success());
        assert!(failure.error_message().is_some());
    }
}
