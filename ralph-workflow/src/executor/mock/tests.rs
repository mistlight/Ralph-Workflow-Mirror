use super::*;
use crate::executor::ProcessExecutor;
use std::io;

#[test]
fn test_mock_executor_captures_calls() {
    let mock = MockProcessExecutor::new();
    let _ = mock.execute("echo", &["hello"], &[], None);

    assert_eq!(mock.execute_count(), 1);
    let calls = mock.execute_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "echo");
    assert_eq!(calls[0].1, vec!["hello"]);
}

#[test]
fn test_mock_executor_returns_output() {
    let mock = MockProcessExecutor::new().with_output("git", "git version 2.40.0");

    let result = mock.execute("git", &["--version"], &[], None).unwrap();
    assert_eq!(result.stdout, "git version 2.40.0");
    assert!(result.status.success());
}

#[test]
fn test_mock_executor_returns_error() {
    let mock =
        MockProcessExecutor::new().with_io_error("git", io::ErrorKind::NotFound, "git not found");

    let result = mock.execute("git", &["--version"], &[], None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert_eq!(err.to_string(), "git not found");
}

#[test]
fn test_mock_executor_can_be_reset() {
    let mock = MockProcessExecutor::new();
    let _ = mock.execute("echo", &["test"], &[], None);

    assert_eq!(mock.execute_count(), 1);
    mock.reset_calls();
    assert_eq!(mock.execute_count(), 0);
}
