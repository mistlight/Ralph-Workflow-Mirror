mod tests {
    use super::*;

    // =========================================================================
    // Workspace-based tests (for testability without real filesystem)
    // =========================================================================

    #[cfg(feature = "test-utils")]
    mod workspace_tests {
        use super::*;
        use crate::workspace::MemoryWorkspace;

        #[test]
        fn test_file_system_state_new() {
            let state = FileSystemState::new();
            assert!(state.files.is_empty());
            assert!(state.git_head_oid.is_none());
            assert!(state.git_branch.is_none());
        }

        #[test]
        fn test_capture_file_with_workspace() {
            let workspace = MemoryWorkspace::new_test().with_file("test.txt", "content");

            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace, "test.txt");

            assert!(state.files.contains_key("test.txt"));
            let snapshot = &state.files["test.txt"];
            assert!(snapshot.exists);
            assert_eq!(snapshot.size, 7);
        }

        #[test]
        fn test_capture_file_with_workspace_nonexistent() {
            let workspace = MemoryWorkspace::new_test();

            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace, "nonexistent.txt");

            assert!(state.files.contains_key("nonexistent.txt"));
            let snapshot = &state.files["nonexistent.txt"];
            assert!(!snapshot.exists);
            assert_eq!(snapshot.size, 0);
        }

        #[test]
        fn test_validate_with_workspace_success() {
            let workspace = MemoryWorkspace::new_test().with_file("test.txt", "content");

            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace, "test.txt");

            let errors = state.validate_with_workspace(&workspace, None);
            assert!(errors.is_empty());
        }

        #[test]
        fn test_validate_with_workspace_file_missing() {
            // Create workspace with file, capture state
            let workspace_with_file = MemoryWorkspace::new_test().with_file("test.txt", "content");
            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace_with_file, "test.txt");

            // Create new workspace without the file (simulating file deletion)
            let workspace_without_file = MemoryWorkspace::new_test();

            // Validation should fail because file is missing
            let errors = state.validate_with_workspace(&workspace_without_file, None);
            assert!(!errors.is_empty());
            assert!(matches!(errors[0], ValidationError::FileMissing { .. }));
        }

        #[test]
        fn test_validate_with_workspace_file_changed() {
            // Create workspace with original file
            let workspace_original = MemoryWorkspace::new_test().with_file("test.txt", "content");
            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace_original, "test.txt");

            // Create new workspace with modified content
            let workspace_modified = MemoryWorkspace::new_test().with_file("test.txt", "modified");

            let errors = state.validate_with_workspace(&workspace_modified, None);
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0],
                ValidationError::FileContentChanged { .. }
            ));
        }

        #[test]
        fn test_validate_with_workspace_file_unexpectedly_exists() {
            // Create state with non-existent file
            let workspace_empty = MemoryWorkspace::new_test();
            let mut state = FileSystemState::new();
            state.capture_file_with_workspace(&workspace_empty, "test.txt");

            // Create new workspace with the file (simulating unexpected file creation)
            let workspace_with_file = MemoryWorkspace::new_test().with_file("test.txt", "content");

            let errors = state.validate_with_workspace(&workspace_with_file, None);
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0],
                ValidationError::FileUnexpectedlyExists { .. }
            ));
        }
    }

    // =========================================================================
    // Pure unit tests (no filesystem access)
    // =========================================================================

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::FileMissing {
            path: "test.txt".to_string(),
        };
        assert_eq!(err.to_string(), "File missing: test.txt");

        let err = ValidationError::FileContentChanged {
            path: "test.txt".to_string(),
        };
        assert_eq!(err.to_string(), "File content changed: test.txt");
    }

    #[test]
    fn test_validation_error_recovery_suggestion() {
        let err = ValidationError::FileMissing {
            path: "test.txt".to_string(),
        };
        let (problem, commands) = err.recovery_commands();
        assert!(problem.contains("test.txt"));
        assert!(!commands.is_empty());

        let err = ValidationError::GitHeadChanged {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let (problem, commands) = err.recovery_commands();
        assert!(problem.contains("abc123"));
        assert!(commands.iter().any(|c| c.contains("git reset")));
    }

    #[test]
    fn test_validation_error_recovery_commands_file_missing() {
        let err = ValidationError::FileMissing {
            path: "PROMPT.md".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("missing"));
        assert!(problem.contains("PROMPT.md"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("find")));
    }

    #[test]
    fn test_validation_error_recovery_commands_git_head_changed() {
        let err = ValidationError::GitHeadChanged {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("changed"));
        assert!(problem.contains("abc123"));
        assert!(problem.contains("def456"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git reset")));
        assert!(commands.iter().any(|c| c.contains("git log")));
    }

    #[test]
    fn test_validation_error_recovery_commands_working_tree_changed() {
        let err = ValidationError::GitWorkingTreeChanged {
            changes: "M file1.txt\nM file2.txt".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("uncommitted changes"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git status")));
        assert!(commands.iter().any(|c| c.contains("git stash")));
        assert!(commands.iter().any(|c| c.contains("git commit")));
    }

    #[test]
    fn test_validation_error_recovery_commands_git_state_invalid() {
        let err = ValidationError::GitStateInvalid {
            reason: "detached HEAD state".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("detached HEAD state"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git checkout")));
    }

    #[test]
    fn test_validation_error_recovery_commands_file_content_changed() {
        let err = ValidationError::FileContentChanged {
            path: "PROMPT.md".to_string(),
        };
        let (problem, commands) = err.recovery_commands();

        assert!(problem.contains("changed"));
        assert!(problem.contains("PROMPT.md"));
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.contains("git diff")));
    }
}
