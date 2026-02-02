// Tests for rebase state machine.
//
// This file contains all test code for the rebase state machine module.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_new() {
        let machine = RebaseStateMachine::new("main".to_string());
        assert_eq!(machine.phase(), &RebasePhase::NotStarted);
        assert_eq!(machine.upstream_branch(), "main");
        assert!(machine.can_recover());
        assert!(!machine.should_abort());
    }

    #[test]
    fn test_state_machine_transition() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());
            machine
                .transition_to(RebasePhase::RebaseInProgress)
                .unwrap();
            assert_eq!(machine.phase(), &RebasePhase::RebaseInProgress);
        });
    }

    #[test]
    fn test_state_machine_record_conflict() {
        let mut machine = RebaseStateMachine::new("main".to_string());
        machine.record_conflict("file1.rs".to_string());
        machine.record_conflict("file2.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 2);
    }

    #[test]
    fn test_state_machine_record_resolution() {
        let mut machine = RebaseStateMachine::new("main".to_string());
        machine.record_conflict("file1.rs".to_string());
        machine.record_conflict("file2.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 2);

        machine.record_resolution("file1.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 1);
        assert!(!machine.all_conflicts_resolved());

        machine.record_resolution("file2.rs".to_string());
        assert_eq!(machine.unresolved_conflict_count(), 0);
        assert!(machine.all_conflicts_resolved());
    }

    #[test]
    fn test_state_machine_record_error() {
        let mut machine = RebaseStateMachine::new("main".to_string());
        assert!(machine.can_recover());
        assert!(!machine.should_abort());

        machine.record_error("First error".to_string());
        assert!(machine.can_recover());

        machine.record_error("Second error".to_string());
        assert!(machine.can_recover());

        machine.record_error("Third error".to_string());
        assert!(!machine.can_recover());
        assert!(machine.should_abort());
    }

    #[test]
    fn test_state_machine_custom_max_attempts() {
        let machine = RebaseStateMachine::new("main".to_string()).with_max_recovery_attempts(1);

        assert!(machine.can_recover());
    }

    #[test]
    fn test_state_machine_save_load() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine1 = RebaseStateMachine::new("feature-branch".to_string());
            machine1
                .transition_to(RebasePhase::ConflictDetected)
                .unwrap();

            // Note: record_conflict only updates in-memory state, need to save checkpoint
            // For the test, let's create a checkpoint with conflicts and save it
            use super::super::rebase_checkpoint::{
                save_rebase_checkpoint, RebaseCheckpoint, RebasePhase,
            };
            let checkpoint = RebaseCheckpoint::new("feature-branch".to_string())
                .with_phase(RebasePhase::ConflictDetected)
                .with_conflicted_file("test.rs".to_string());
            save_rebase_checkpoint(&checkpoint).unwrap();

            // Load a new machine from the checkpoint
            let machine2 = RebaseStateMachine::load_or_create("main".to_string()).unwrap();
            assert_eq!(machine2.phase(), &RebasePhase::ConflictDetected);
            assert_eq!(machine2.upstream_branch(), "feature-branch");
            assert_eq!(machine2.unresolved_conflict_count(), 1);
        });
    }

    #[test]
    fn test_state_machine_clear_checkpoint() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());
            machine
                .transition_to(RebasePhase::RebaseInProgress)
                .unwrap();
            assert!(rebase_checkpoint_exists());

            machine.clear_checkpoint().unwrap();
            assert!(!rebase_checkpoint_exists());
        });
    }

    #[test]
    fn test_state_machine_abort() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());
            machine
                .transition_to(RebasePhase::ConflictDetected)
                .unwrap();
            machine.abort().unwrap();

            let loaded = RebaseStateMachine::load_or_create("main".to_string()).unwrap();
            assert_eq!(loaded.phase(), &RebasePhase::RebaseAborted);
        });
    }

    #[test]
    fn test_recovery_action_variants_exist() {
        let _ = RecoveryAction::Continue;
        let _ = RecoveryAction::Retry;
        let _ = RecoveryAction::Abort;
        let _ = RecoveryAction::Skip;
    }

    #[test]
    fn test_acquire_and_release_rebase_lock() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Acquire lock
            acquire_rebase_lock().unwrap();

            // Verify lock file exists
            let lock_path = rebase_lock_path();
            assert!(Path::new(&lock_path).exists());

            // Release lock
            release_rebase_lock().unwrap();

            // Verify lock file is gone
            assert!(!Path::new(&lock_path).exists());
        });
    }

    #[test]
    fn test_rebase_lock_prevents_duplicate() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Acquire first lock
            acquire_rebase_lock().unwrap();

            // Trying to acquire again should fail
            let result = acquire_rebase_lock();
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("already in progress"));
        });
    }

    #[test]
    fn test_rebase_lock_guard_auto_releases() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            {
                // Create lock guard
                let _lock = RebaseLock::new().unwrap();
                let lock_path = rebase_lock_path();
                assert!(Path::new(&lock_path).exists());
            }
            // Lock should be released when guard goes out of scope

            let lock_path = rebase_lock_path();
            assert!(!Path::new(&lock_path).exists());
        });
    }

    #[test]
    fn test_rebase_lock_guard_leak() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            {
                let lock = RebaseLock::new().unwrap();
                let lock_path = rebase_lock_path();
                assert!(Path::new(&lock_path).exists());

                // Leak the lock - it won't be released
                let _ = lock.leak();
            }

            // Lock should still exist after guard is dropped
            let lock_path = rebase_lock_path();
            assert!(Path::new(&lock_path).exists());

            // Clean up
            let _ = release_rebase_lock();
        });
    }

    #[test]
    fn test_stale_lock_is_replaced() {
        use test_helpers::with_temp_cwd;

        with_temp_cwd(|_dir| {
            // Create a lock file with an old timestamp
            let lock_path = rebase_lock_path();
            let old_timestamp = chrono::Utc::now()
                - chrono::Duration::seconds(DEFAULT_LOCK_TIMEOUT_SECONDS as i64 + 60);
            let lock_content = format!("pid=12345\ntimestamp={}\n", old_timestamp.to_rfc3339());

            fs::create_dir_all(".agent").unwrap();
            fs::write(&lock_path, lock_content).unwrap();

            // Acquire lock should succeed since old lock is stale
            acquire_rebase_lock().unwrap();

            // Verify new lock file exists
            assert!(Path::new(&lock_path).exists());

            // Clean up
            release_rebase_lock().unwrap();
        });
    }

    #[test]
    fn test_recovery_action_decide_content_conflict() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::ContentConflict {
            files: vec!["file1.rs".to_string()],
        };

        // Content conflict should always return Continue (to AI resolution)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Continue);

        // Even at max attempts, ContentConflict should Continue
        let action = RecoveryAction::decide(&error, 2, 3);
        assert_eq!(action, RecoveryAction::Continue);

        // But if we exceed max attempts, it should Abort
        let action = RecoveryAction::decide(&error, 3, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_concurrent_operation() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::ConcurrentOperation {
            operation: "rebase".to_string(),
        };

        // Concurrent operation should be retried
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Retry);

        // Should keep retrying until max attempts
        let action = RecoveryAction::decide(&error, 2, 3);
        assert_eq!(action, RecoveryAction::Retry);

        // At max attempts, should abort
        let action = RecoveryAction::decide(&error, 3, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_invalid_revision() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::InvalidRevision {
            revision: "nonexistent".to_string(),
        };

        // Invalid revision should always abort (not recoverable)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_dirty_working_tree() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::DirtyWorkingTree;

        // Dirty working tree should always abort (user needs to commit/stash)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_empty_commit() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::EmptyCommit;

        // Empty commit should be skipped
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Skip);

        // Even at high error counts, should still skip
        let action = RecoveryAction::decide(&error, 5, 10);
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[test]
    fn test_recovery_action_decide_process_terminated() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::ProcessTerminated {
            reason: "agent crashed".to_string(),
        };

        // Process termination should continue (recover from checkpoint)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Continue);
    }

    #[test]
    fn test_recovery_action_decide_inconsistent_state() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::InconsistentState {
            details: "HEAD detached unexpectedly".to_string(),
        };

        // Inconsistent state should retry (after cleanup)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Retry);

        // Should keep retrying until max attempts
        let action = RecoveryAction::decide(&error, 2, 3);
        assert_eq!(action, RecoveryAction::Retry);

        // At max attempts, should abort
        let action = RecoveryAction::decide(&error, 3, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_patch_application_failed() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::PatchApplicationFailed {
            reason: "context mismatch".to_string(),
        };

        // Patch application failure should retry
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Retry);
    }

    #[test]
    fn test_recovery_action_decide_validation_failed() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::ValidationFailed {
            reason: "tests failed".to_string(),
        };

        // Validation failure should abort (needs manual fix)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_unknown() {
        use super::super::rebase::RebaseErrorKind;

        let error = RebaseErrorKind::Unknown {
            details: "something went wrong".to_string(),
        };

        // Unknown errors should abort (safe default)
        let action = RecoveryAction::decide(&error, 0, 3);
        assert_eq!(action, RecoveryAction::Abort);
    }

    #[test]
    fn test_recovery_action_decide_max_attempts_exceeded() {
        use super::super::rebase::RebaseErrorKind;

        let retryable_errors = [
            RebaseErrorKind::ConcurrentOperation {
                operation: "merge".to_string(),
            },
            RebaseErrorKind::PatchApplicationFailed {
                reason: "fuzz failure".to_string(),
            },
            RebaseErrorKind::AutostashFailed {
                reason: "stash pop failed".to_string(),
            },
        ];

        // All retryable errors should abort when max attempts exceeded
        for error in retryable_errors {
            let action = RecoveryAction::decide(&error, 5, 3);
            assert_eq!(
                action,
                RecoveryAction::Abort,
                "Expected Abort for error: {error:?}"
            );
        }
    }

    #[test]
    fn test_recovery_action_decide_category_1_non_recoverable() {
        use super::super::rebase::RebaseErrorKind;

        let non_recoverable_errors = [
            RebaseErrorKind::InvalidRevision {
                revision: "bad-ref".to_string(),
            },
            RebaseErrorKind::RepositoryCorrupt {
                details: "missing objects".to_string(),
            },
            RebaseErrorKind::EnvironmentFailure {
                reason: "no editor configured".to_string(),
            },
            RebaseErrorKind::HookRejection {
                hook_name: "pre-rebase".to_string(),
            },
        ];

        // All these should abort regardless of error count
        for error in non_recoverable_errors {
            let action = RecoveryAction::decide(&error, 0, 3);
            assert_eq!(
                action,
                RecoveryAction::Abort,
                "Expected Abort for error: {error:?}"
            );
        }
    }

    #[test]
    fn test_recovery_action_decide_category_2_mixed() {
        use super::super::rebase::RebaseErrorKind;

        // Interactive stop should abort (manual intervention needed)
        let interactive = RebaseErrorKind::InteractiveStop {
            command: "edit".to_string(),
        };
        assert_eq!(
            RecoveryAction::decide(&interactive, 0, 3),
            RecoveryAction::Abort
        );

        // Reference update failure should retry (transient)
        let ref_fail = RebaseErrorKind::ReferenceUpdateFailed {
            reason: "concurrent update".to_string(),
        };
        assert_eq!(
            RecoveryAction::decide(&ref_fail, 0, 3),
            RecoveryAction::Retry
        );

        // Commit creation failure should retry (transient)
        let commit_fail = RebaseErrorKind::CommitCreationFailed {
            reason: "hook failed".to_string(),
        };
        assert_eq!(
            RecoveryAction::decide(&commit_fail, 0, 3),
            RecoveryAction::Retry
        );
    }
}
