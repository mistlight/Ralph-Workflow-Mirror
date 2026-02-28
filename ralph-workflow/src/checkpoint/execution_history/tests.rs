use super::*;

#[test]
fn test_execution_step_new() {
    let outcome = StepOutcome::success(None, vec!["test.txt".to_string()]);
    let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
    assert_eq!(&*step.phase, "Development");
    assert_eq!(step.iteration, 1);
    assert_eq!(&*step.step_type, "dev_run");
    assert!(step.agent.is_none());
    assert!(step.duration_secs.is_none());
    // Verify new fields are None by default
    assert!(step.git_commit_oid.is_none());
    assert!(step.modified_files_detail.is_none());
    assert!(step.prompt_used.is_none());
    assert!(step.issues_summary.is_none());
}

#[test]
fn test_execution_step_with_agent() {
    let outcome = StepOutcome::success(None, vec![]);
    let step = ExecutionStep::new("Development", 1, "dev_run", outcome)
        .with_agent("claude")
        .with_duration(120);
    assert_eq!(step.agent.as_deref(), Some("claude"));
    assert_eq!(step.duration_secs, Some(120));
}

#[test]
fn test_execution_step_new_fields_default() {
    let outcome = StepOutcome::success(None, vec![]);
    let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
    // Verify new fields are None by default
    assert!(step.git_commit_oid.is_none());
    assert!(step.modified_files_detail.is_none());
    assert!(step.prompt_used.is_none());
    assert!(step.issues_summary.is_none());
}

#[test]
fn test_modified_files_detail_default() {
    let detail = ModifiedFilesDetail::default();
    assert!(detail.added.is_none());
    assert!(detail.modified.is_none());
    assert!(detail.deleted.is_none());
}

#[test]
fn test_issues_summary_default() {
    let summary = IssuesSummary::default();
    assert_eq!(summary.found, 0);
    assert_eq!(summary.fixed, 0);
    assert!(summary.description.is_none());
}

#[test]
fn test_file_snapshot() {
    let snapshot = FileSnapshot::new("test.txt", "abc123".to_string(), 100, true);
    assert_eq!(snapshot.path, "test.txt");
    assert_eq!(snapshot.checksum, "abc123");
    assert_eq!(snapshot.size, 100);
    assert!(snapshot.exists);
}

#[test]
fn test_file_snapshot_not_found() {
    let snapshot = FileSnapshot::not_found("missing.txt");
    assert_eq!(snapshot.path, "missing.txt");
    assert!(!snapshot.exists);
    assert_eq!(snapshot.size, 0);
}

#[test]
fn test_decompress_data_rejects_oversized_payload() {
    // Safety invariant: checkpoint resume must not allow decompression bombs.
    // We enforce an upper bound on decompressed payload size.
    let max_bytes = 1024 * 1024;
    let data = "a".repeat(max_bytes + 1);
    let encoded = compress_data(data.as_bytes()).unwrap();

    let err = decompress_data(&encoded).expect_err("oversized payload should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_execution_history_add_step_bounded() {
    let mut history = ExecutionHistory::new();
    let outcome = StepOutcome::success(None, vec![]);
    let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
    history.add_step_bounded(step, 1000);
    assert_eq!(history.steps.len(), 1);
    assert_eq!(&*history.steps[0].phase, "Development");
    assert_eq!(history.steps[0].iteration, 1);
}

#[test]
fn test_execution_step_serialization_omits_none_option_fields() {
    let outcome = StepOutcome::success(None, vec![]);
    let step = ExecutionStep::new("Development", 1, "dev_run", outcome);
    let json = serde_json::to_string(&step).unwrap();

    assert!(!json.contains("\"agent\":null"));
    assert!(!json.contains("\"duration_secs\":null"));
    assert!(!json.contains("\"checkpoint_saved_at\":null"));
    assert!(!json.contains("\"git_commit_oid\":null"));
    assert!(!json.contains("\"modified_files_detail\":null"));
    assert!(!json.contains("\"prompt_used\":null"));
    assert!(!json.contains("\"issues_summary\":null"));
}

#[test]
fn test_execution_step_serialization_with_new_fields() {
    // Create a step with new fields via JSON to test backward compatibility
    let json_str = r#"{"phase":"Review","iteration":1,"step_type":"review","timestamp":"2025-01-20 12:00:00","outcome":{"Success":{"output":null,"files_modified":[],"exit_code":0}},"agent":null,"duration_secs":null,"checkpoint_saved_at":null,"git_commit_oid":"abc123","modified_files_detail":{"added":["a.rs"],"modified":[],"deleted":[]},"prompt_used":"Fix issues","issues_summary":{"found":2,"fixed":2,"description":"All fixed"}}"#;
    let deserialized: ExecutionStep = serde_json::from_str(json_str).unwrap();
    assert_eq!(deserialized.git_commit_oid, Some("abc123".to_string()));
    let added = deserialized
        .modified_files_detail
        .as_ref()
        .unwrap()
        .added
        .as_ref()
        .unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0], "a.rs");

    // Empty arrays in legacy JSON should preserve the None-for-empty canonical form.
    let detail = deserialized.modified_files_detail.as_ref().unwrap();
    assert!(detail.modified.is_none());
    assert!(detail.deleted.is_none());
    assert_eq!(deserialized.prompt_used, Some("Fix issues".to_string()));
    assert_eq!(deserialized.issues_summary.as_ref().unwrap().found, 2);
}

#[test]
fn test_execution_step_with_string_pool() {
    use crate::checkpoint::StringPool;

    let mut pool = StringPool::new();
    let outcome = StepOutcome::success(None, vec![]);

    // Create multiple steps with the same phase and agent
    let step1 =
        ExecutionStep::new_with_pool("Development", 1, "dev_run", outcome.clone(), &mut pool)
            .with_agent_pooled("claude", &mut pool);
    let step2 = ExecutionStep::new_with_pool("Development", 2, "dev_run", outcome, &mut pool)
        .with_agent_pooled("claude", &mut pool);

    // Verify string pool deduplication works
    assert!(Arc::ptr_eq(&step1.phase, &step2.phase));
    assert!(Arc::ptr_eq(
        step1.agent.as_ref().unwrap(),
        step2.agent.as_ref().unwrap()
    ));

    // Verify content is correct
    assert_eq!(&*step1.phase, "Development");
    assert_eq!(&*step2.phase, "Development");
    assert_eq!(step1.agent.as_deref(), Some("claude"));
    assert_eq!(step2.agent.as_deref(), Some("claude"));
}

#[test]
fn test_execution_step_memory_optimization() {
    use crate::checkpoint::StringPool;

    let mut pool = StringPool::new();
    let outcome = StepOutcome::success(None, vec![]);

    // Create step with string pool
    let step = ExecutionStep::new_with_pool("Development", 1, "dev_run", outcome, &mut pool)
        .with_agent_pooled("claude", &mut pool);

    // Arc<str> and Box<str> should use len() not capacity()
    let phase_size = step.phase.len();
    let step_type_size = step.step_type.len();
    let agent_size = step.agent.as_ref().map_or(0, |s| s.len());

    // Verify sizes are reasonable
    assert_eq!(phase_size, "Development".len());
    assert_eq!(step_type_size, "dev_run".len());
    assert_eq!(agent_size, "claude".len());

    // Total size should be less than String capacity-based approach
    let optimized_size = phase_size + step_type_size + agent_size;
    assert!(optimized_size < 100); // Reasonable upper bound
}

#[test]
fn test_execution_step_serialization_roundtrip() {
    use crate::checkpoint::StringPool;

    let mut pool = StringPool::new();
    let outcome = StepOutcome::success(Some("output".to_string()), vec!["file.txt".to_string()]);

    let step = ExecutionStep::new_with_pool("Development", 1, "dev_run", outcome, &mut pool)
        .with_agent_pooled("claude", &mut pool)
        .with_duration(120);

    // Serialize to JSON
    let json = serde_json::to_string(&step).unwrap();

    // Deserialize back
    let deserialized: ExecutionStep = serde_json::from_str(&json).unwrap();

    // Verify all fields match
    assert_eq!(&*step.phase, &*deserialized.phase);
    assert_eq!(step.iteration, deserialized.iteration);
    assert_eq!(&*step.step_type, &*deserialized.step_type);
    assert_eq!(step.agent.as_deref(), deserialized.agent.as_deref());
    assert_eq!(step.duration_secs, deserialized.duration_secs);
    assert_eq!(step.outcome, deserialized.outcome);
}

#[test]
fn test_execution_step_backward_compatible_deserialization() {
    // Old checkpoint format with String fields
    let old_json = r#"{
        "phase": "Development",
        "iteration": 1,
        "step_type": "dev_run",
        "timestamp": "2025-01-20 12:00:00",
        "outcome": {"Success": {"output": null, "files_modified": [], "exit_code": 0}},
        "agent": "claude",
        "duration_secs": 120
    }"#;

    // Should deserialize successfully into new Arc<str> format
    let step: ExecutionStep = serde_json::from_str(old_json).unwrap();

    assert_eq!(&*step.phase, "Development");
    assert_eq!(step.iteration, 1);
    assert_eq!(&*step.step_type, "dev_run");
    assert_eq!(step.agent.as_deref(), Some("claude"));
    assert_eq!(step.duration_secs, Some(120));
}

#[test]
fn test_step_outcome_success_with_empty_files_uses_none() {
    // Empty files_modified should use None instead of empty Vec
    let outcome = StepOutcome::success(None, vec![]);

    match outcome {
        StepOutcome::Success { files_modified, .. } => {
            assert!(files_modified.is_none(), "Empty files should be None");
        }
        _ => panic!("Expected Success variant"),
    }
}

#[test]
fn test_step_outcome_success_with_files_uses_boxed_slice() {
    // Non-empty files_modified should use Box<[String]>
    let files = vec!["file1.txt".to_string(), "file2.txt".to_string()];
    let outcome = StepOutcome::success(None, files);

    match outcome {
        StepOutcome::Success { files_modified, .. } => {
            let files = files_modified.expect("Files should be present");
            assert_eq!(files.len(), 2);
            assert_eq!(files[0], "file1.txt");
            assert_eq!(files[1], "file2.txt");
        }
        _ => panic!("Expected Success variant"),
    }
}

#[test]
fn test_step_outcome_failure_with_no_signals_uses_none() {
    // Failure without signals should use None
    let outcome = StepOutcome::failure("error message".to_string(), true);

    match outcome {
        StepOutcome::Failure { signals, .. } => {
            assert!(signals.is_none(), "Empty signals should be None");
        }
        _ => panic!("Expected Failure variant"),
    }
}

#[test]
fn test_step_outcome_uses_box_str_for_strings() {
    // Verify that Box<str> is used for string fields
    let outcome = StepOutcome::failure("test error".to_string(), false);

    match outcome {
        StepOutcome::Failure { error, .. } => {
            assert_eq!(&*error, "test error");
            // Box<str> uses exactly the needed space
            assert_eq!(error.len(), "test error".len());
        }
        _ => panic!("Expected Failure variant"),
    }
}

#[test]
fn test_step_outcome_constructors_preserve_large_string_content() {
    // StepOutcome constructors accept owned String inputs and store them as Box<str>.
    // Allocation reuse is an optimization and is not guaranteed by Rust toolchains or
    // allocators, so this test asserts only semantic correctness.

    // Large strings avoid any small-string/allocator-size quirks.
    let make_string = |byte: u8| -> String {
        let bytes = vec![byte; 1024];
        String::from_utf8(bytes).expect("valid utf8")
    };

    // failure()
    let s = make_string(b'e');
    let s_expected = s.clone();
    let outcome = StepOutcome::failure(s, true);
    match outcome {
        StepOutcome::Failure { error, .. } => {
            assert_eq!(&*error, s_expected);
            assert_eq!(error.len(), s_expected.len());
        }
        _ => panic!("Expected Failure variant"),
    }

    // partial()
    let completed = make_string(b'c');
    let completed_expected = completed.clone();
    let remaining = make_string(b'r');
    let remaining_expected = remaining.clone();
    let outcome = StepOutcome::partial(completed, remaining);
    match outcome {
        StepOutcome::Partial {
            completed,
            remaining,
            ..
        } => {
            assert_eq!(&*completed, completed_expected);
            assert_eq!(completed.len(), completed_expected.len());
            assert_eq!(&*remaining, remaining_expected);
            assert_eq!(remaining.len(), remaining_expected.len());
        }
        _ => panic!("Expected Partial variant"),
    }

    // skipped()
    let reason = make_string(b's');
    let reason_expected = reason.clone();
    let outcome = StepOutcome::skipped(reason);
    match outcome {
        StepOutcome::Skipped { reason } => {
            assert_eq!(&*reason, reason_expected);
            assert_eq!(reason.len(), reason_expected.len());
        }
        _ => panic!("Expected Skipped variant"),
    }

    // success(Some(output), empty files)
    let output = make_string(b'o');
    let output_expected = output.clone();
    let outcome = StepOutcome::success(Some(output), vec![]);
    match outcome {
        StepOutcome::Success {
            output: Some(output),
            ..
        } => {
            assert_eq!(&*output, output_expected);
            assert_eq!(output.len(), output_expected.len());
        }
        _ => panic!("Expected Success variant with output"),
    }
}

#[test]
fn test_step_outcome_partial_uses_box_str() {
    let outcome = StepOutcome::partial("done".to_string(), "remaining".to_string());

    match outcome {
        StepOutcome::Partial {
            completed,
            remaining,
            ..
        } => {
            assert_eq!(&*completed, "done");
            assert_eq!(&*remaining, "remaining");
            // Verify Box<str> efficiency
            assert_eq!(completed.len(), "done".len());
            assert_eq!(remaining.len(), "remaining".len());
        }
        _ => panic!("Expected Partial variant"),
    }
}

#[test]
fn test_step_outcome_skipped_uses_box_str() {
    let outcome = StepOutcome::skipped("already done".to_string());

    match outcome {
        StepOutcome::Skipped { reason } => {
            assert_eq!(&*reason, "already done");
            assert_eq!(reason.len(), "already done".len());
        }
        _ => panic!("Expected Skipped variant"),
    }
}

#[test]
fn test_step_outcome_serialization_with_empty_collections() {
    // Test that empty collections serialize correctly
    let outcome = StepOutcome::success(None, vec![]);
    let json = serde_json::to_string(&outcome).unwrap();

    // Deserialize back
    let deserialized: StepOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(outcome, deserialized);

    // Verify None is preserved
    match deserialized {
        StepOutcome::Success { files_modified, .. } => {
            assert!(files_modified.is_none());
        }
        _ => panic!("Expected Success variant"),
    }
}

#[test]
fn test_step_outcome_backward_compatibility_with_empty_vec() {
    // Old checkpoints may have empty Vec serialized as []
    let old_json = r#"{"Success":{"output":null,"files_modified":[],"exit_code":0}}"#;
    let outcome: StepOutcome = serde_json::from_str(old_json).unwrap();

    // Canonical form: treat empty arrays as None to preserve the
    // None-for-empty optimization when resaving a legacy checkpoint.
    match outcome {
        StepOutcome::Success {
            ref files_modified, ..
        } => {
            assert!(
                files_modified.is_none(),
                "expected empty legacy array to deserialize as None"
            );
        }
        _ => panic!("Expected Success variant"),
    }

    // Round-trip should preserve the on-disk shape for compatibility.
    let json = serde_json::to_string(&outcome).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value.get("Success").and_then(|v| v.get("files_modified")),
        Some(&serde_json::Value::Array(vec![])),
        "expected serialization to use [] (not null) for compatibility"
    );
}

#[test]
fn test_step_outcome_failure_signals_serialize_as_empty_array_when_none() {
    let outcome = StepOutcome::failure("boom".to_string(), true);
    let json = serde_json::to_string(&outcome).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value.get("Failure").and_then(|v| v.get("signals")),
        Some(&serde_json::Value::Array(vec![])),
        "expected serialization to use [] (not null) for signals"
    );
}

#[test]
fn test_modified_files_detail_legacy_empty_arrays_deserialize_to_none() {
    let legacy = r#"{"added":[],"modified":[],"deleted":[]}"#;
    let detail: ModifiedFilesDetail = serde_json::from_str(legacy).unwrap();
    assert!(detail.added.is_none());
    assert!(detail.modified.is_none());
    assert!(detail.deleted.is_none());

    // Round-trip should omit empty fields.
    let json = serde_json::to_string(&detail).unwrap();
    assert_eq!(json, "{}", "expected empty fields to be omitted");
}

#[test]
fn test_step_outcome_memory_efficiency_vs_vec() {
    // Demonstrate memory efficiency of Box<str> and Option<Box<[T]>>
    // Vec<T> over-allocates capacity, Box<[T]> uses exact size

    let outcome = StepOutcome::success(
        Some("output".to_string()),
        vec!["file1.txt".to_string(), "file2.txt".to_string()],
    );

    match outcome {
        StepOutcome::Success {
            output,
            files_modified,
            ..
        } => {
            // Box<str> uses exact size
            let output_str = output.expect("Output should be present");
            assert_eq!(output_str.len(), "output".len());

            // Box<[String]> uses exact size (no excess capacity)
            let files = files_modified.expect("Files should be present");
            assert_eq!(files.len(), 2);
        }
        _ => panic!("Expected Success variant"),
    }
}
