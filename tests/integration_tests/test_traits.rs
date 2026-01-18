//! Integration tests for test trait exports.
//!
//! These tests verify that test traits like MockGit and MockAgentExecutor
//! are properly exported from the ralph-workflow crate and can be used
//! in integration tests.

use ralph_workflow::git_helpers::ops::{CommitResult, GitOps, RebaseResult as GitRebaseResult};
use ralph_workflow::git_helpers::test_trait::{
    MockGit, TestCommitResult, TestGit, TestRebaseResult,
};
use ralph_workflow::pipeline::test_trait::{AgentCommandResult, AgentExecutor, MockAgentExecutor};
use std::path::PathBuf;

/// Test that MockGit can be created and used via TestGit trait.
#[test]
fn test_mock_git_creation() {
    let mock = MockGit::new();
    assert!(TestGit::require_repo(&mock).is_ok());
}

/// Test that MockGit builder pattern works.
#[test]
fn test_mock_git_builder() {
    let mock = MockGit::new()
        .with_repo_root(Ok(PathBuf::from("/test/repo")))
        .with_diff(Ok("test diff".to_string()))
        .with_snapshot(Ok("M file.txt".to_string()));

    assert_eq!(
        TestGit::repo_root(&mock).unwrap(),
        PathBuf::from("/test/repo")
    );
    assert_eq!(TestGit::diff(&mock).unwrap(), "test diff");
    assert_eq!(TestGit::snapshot(&mock).unwrap(), "M file.txt");
}

/// Test that MockGit implements GitOps trait.
#[test]
fn test_mock_git_implements_git_ops() {
    let mock = MockGit::new()
        .with_commit(Ok(TestCommitResult::Success("abc123".to_string())))
        .with_rebase_onto(Ok(TestRebaseResult::Success));

    // Test via GitOps trait
    let commit_result = GitOps::commit(&mock, "test message", None, None).unwrap();
    assert_eq!(commit_result, CommitResult::Success("abc123".to_string()));

    let rebase_result = GitOps::rebase_onto(&mock, "main").unwrap();
    assert_eq!(rebase_result, GitRebaseResult::Success);
}

/// Test that MockGit call capture works.
#[test]
fn test_mock_git_call_capture() {
    let mock = MockGit::new();

    let _ = TestGit::diff(&mock);
    let _ = TestGit::diff(&mock);
    let _ = TestGit::commit(&mock, "first");
    let _ = TestGit::commit(&mock, "second");

    assert_eq!(mock.diff_count(), 2);
    assert_eq!(mock.commit_calls().len(), 2);
    assert_eq!(mock.commit_calls()[0], "first");
    assert_eq!(mock.commit_calls()[1], "second");
}

/// Test that MockAgentExecutor can be created and used.
#[test]
fn test_mock_agent_executor_creation() {
    let mock = MockAgentExecutor::new();
    assert!(!mock.was_called());
}

/// Test that MockAgentExecutor builder pattern works.
#[test]
fn test_mock_agent_executor_builder() {
    let mock = MockAgentExecutor::new()
        .with_response(Ok(AgentCommandResult::success("output")))
        .with_response(Ok(AgentCommandResult::failure(1, "error")));

    // Create a config for testing
    use ralph_workflow::agents::JsonParserType;
    use ralph_workflow::pipeline::test_trait::AgentCommandConfig;
    use std::collections::HashMap;

    let config = AgentCommandConfig {
        cmd: "test-cmd".to_string(),
        prompt: "test prompt".to_string(),
        env_vars: HashMap::new(),
        parser_type: JsonParserType::Claude,
        logfile: "/tmp/test.log".to_string(),
        display_name: "test".to_string(),
    };

    let r1 = mock.execute(&config).unwrap();
    assert_eq!(r1.exit_code, 0);
    assert_eq!(r1.stdout, "output");

    let r2 = mock.execute(&config).unwrap();
    assert_eq!(r2.exit_code, 1);
    assert_eq!(r2.stderr, "error");
}

/// Test that MockAgentExecutor call capture works.
#[test]
fn test_mock_agent_executor_call_capture() {
    let mock = MockAgentExecutor::new();

    use ralph_workflow::agents::JsonParserType;
    use ralph_workflow::pipeline::test_trait::AgentCommandConfig;
    use std::collections::HashMap;

    let config1 = AgentCommandConfig {
        cmd: "claude -p".to_string(),
        prompt: "prompt1".to_string(),
        env_vars: HashMap::new(),
        parser_type: JsonParserType::Claude,
        logfile: "/tmp/test.log".to_string(),
        display_name: "test".to_string(),
    };

    let config2 = AgentCommandConfig {
        cmd: "codex run".to_string(),
        prompt: "prompt2".to_string(),
        env_vars: HashMap::new(),
        parser_type: JsonParserType::Codex,
        logfile: "/tmp/test.log".to_string(),
        display_name: "test".to_string(),
    };

    let _ = mock.execute(&config1);
    let _ = mock.execute(&config2);

    assert_eq!(mock.call_count(), 2);
    assert_eq!(mock.prompts(), vec!["prompt1", "prompt2"]);
    assert_eq!(mock.commands(), vec!["claude -p", "codex run"]);

    // Test filtering
    let claude_calls = mock.calls_matching("claude");
    assert_eq!(claude_calls.len(), 1);
}

/// Test that mock error variants work.
#[test]
fn test_mock_error_variants() {
    let mock_git = MockGit::new_error();
    assert!(TestGit::repo_root(&mock_git).is_err());
    assert!(TestGit::diff(&mock_git).is_err());

    let mock_executor = MockAgentExecutor::new_error();
    use ralph_workflow::agents::JsonParserType;
    use ralph_workflow::pipeline::test_trait::AgentCommandConfig;
    use std::collections::HashMap;

    let config = AgentCommandConfig {
        cmd: "test".to_string(),
        prompt: "test".to_string(),
        env_vars: HashMap::new(),
        parser_type: JsonParserType::Claude,
        logfile: "/tmp/test.log".to_string(),
        display_name: "test".to_string(),
    };

    assert!(mock_executor.execute(&config).is_err());
}
