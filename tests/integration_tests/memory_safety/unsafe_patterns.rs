//! Process execution isolation tests (memory safety suite)
//!
//! Integration tests must NOT spawn external processes.
//!
//! Unsafe behavior in `RealProcessExecutor` is verified by unit tests in
//! `ralph-workflow/src/executor/` (allowed to spawn) and by system tests
//! when real process coverage is required.

use crate::test_timeout::with_default_timeout;

use ralph_workflow::executor::{MockProcessExecutor, ProcessExecutor};

#[test]
fn test_mock_executor_execute_is_deterministic_and_captures_calls() {
    with_default_timeout(|| {
        let executor = MockProcessExecutor::new().with_output("echo", "test");

        let output = executor
            .execute("echo", &["ignored"], &[], None)
            .expect("mock execution should succeed");

        assert_eq!(output.stdout, "test");
        assert_eq!(executor.execute_count(), 1);

        let calls = executor.execute_calls();
        assert_eq!(calls.len(), 1); // OK: content checked below
        assert_eq!(calls[0].0, "echo");
        assert_eq!(calls[0].1, vec!["ignored"]);
    });
}

#[test]
fn test_mock_executor_records_env_and_workdir() {
    with_default_timeout(|| {
        let executor = MockProcessExecutor::new().with_output("git", "ok");

        let _ = executor
            .execute(
                "git",
                &["status"],
                &[("A".to_string(), "1".to_string())],
                Some(std::path::Path::new("/tmp")),
            )
            .expect("mock execution should succeed");

        let calls = executor.execute_calls_for("git");
        assert_eq!(calls.len(), 1); // OK: content checked below

        let (_cmd, args, env, workdir) = &calls[0];
        assert_eq!(args, &vec!["status".to_string()]);
        assert_eq!(env, &vec![("A".to_string(), "1".to_string())]);
        assert_eq!(workdir.as_deref(), Some("/tmp"));
    });
}
