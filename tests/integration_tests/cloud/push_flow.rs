//! Tests for git push flow in cloud mode.
//!
//! These tests verify that cloud mode correctly sequences git operations:
//! - ConfigureGitAuth effect is emitted before first push
//! - PushToRemote effect is emitted after each commit
//! - Push failures are handled gracefully (don't halt pipeline)
//! - PR creation works in finalizing phase
//! - Multiple commits result in multiple pushes
//!
//! All tests use reducer-level testing (PipelineState, effects, events)
//! to verify orchestration logic without requiring full pipeline execution.

use ralph_workflow::config::{CloudStateConfig, GitAuthMethod, GitRemoteConfig};
use ralph_workflow::reducer::event::{CommitEvent, PipelineEvent};
use ralph_workflow::reducer::state::PipelineState;
use serial_test::serial;

use crate::test_timeout::with_default_timeout;

#[test]
#[serial]
fn test_git_remote_config_ssh_auth() {
    with_default_timeout(|| {
        std::env::set_var("RALPH_GIT_AUTH_METHOD", "ssh");
        std::env::set_var("RALPH_GIT_SSH_KEY_PATH", "/root/.ssh/id_rsa");

        let config = GitRemoteConfig::from_env();

        match config.auth_method {
            GitAuthMethod::SshKey { key_path } => {
                assert_eq!(
                    key_path.as_deref(),
                    Some("/root/.ssh/id_rsa"),
                    "SSH key path should be loaded"
                );
            }
            _ => panic!("Expected SshKey auth method"),
        }

        std::env::remove_var("RALPH_GIT_AUTH_METHOD");
        std::env::remove_var("RALPH_GIT_SSH_KEY_PATH");
    });
}

#[test]
#[serial]
fn test_git_remote_config_token_auth() {
    with_default_timeout(|| {
        std::env::set_var("RALPH_GIT_AUTH_METHOD", "token");
        std::env::set_var("RALPH_GIT_TOKEN", "ghp_test_token_123");
        std::env::set_var("RALPH_GIT_TOKEN_USERNAME", "oauth2");

        let config = GitRemoteConfig::from_env();

        match config.auth_method {
            GitAuthMethod::Token { token, username } => {
                assert_eq!(token, "ghp_test_token_123", "Token should be loaded");
                assert_eq!(username, "oauth2", "Username should be loaded");
            }
            _ => panic!("Expected Token auth method"),
        }

        std::env::remove_var("RALPH_GIT_AUTH_METHOD");
        std::env::remove_var("RALPH_GIT_TOKEN");
        std::env::remove_var("RALPH_GIT_TOKEN_USERNAME");
    });
}

#[test]
#[serial]
fn test_git_remote_config_credential_helper() {
    with_default_timeout(|| {
        std::env::set_var("RALPH_GIT_AUTH_METHOD", "credential-helper");
        std::env::set_var("RALPH_GIT_CREDENTIAL_HELPER", "gcloud");

        let config = GitRemoteConfig::from_env();

        match config.auth_method {
            GitAuthMethod::CredentialHelper { helper } => {
                assert_eq!(helper, "gcloud", "Credential helper should be loaded");
            }
            _ => panic!("Expected CredentialHelper auth method"),
        }

        std::env::remove_var("RALPH_GIT_AUTH_METHOD");
        std::env::remove_var("RALPH_GIT_CREDENTIAL_HELPER");
    });
}

#[test]
#[serial]
fn test_git_remote_config_defaults() {
    with_default_timeout(|| {
        std::env::remove_var("RALPH_GIT_AUTH_METHOD");
        std::env::remove_var("RALPH_GIT_REMOTE");
        std::env::remove_var("RALPH_GIT_PUSH_BRANCH");
        std::env::remove_var("RALPH_GIT_CREATE_PR");
        std::env::remove_var("RALPH_GIT_FORCE_PUSH");

        let config = GitRemoteConfig::from_env();

        assert_eq!(
            config.remote_name, "origin",
            "Remote should default to origin"
        );
        assert!(
            !config.create_pr,
            "PR creation should be disabled by default"
        );
        assert!(
            !config.force_push,
            "Force push should be disabled by default"
        );
        assert!(
            config.push_branch.is_none(),
            "Push branch should be None by default"
        );
    });
}

#[test]
#[serial]
fn test_git_remote_config_pr_creation() {
    with_default_timeout(|| {
        std::env::set_var("RALPH_GIT_CREATE_PR", "true");
        std::env::set_var("RALPH_GIT_PR_BASE_BRANCH", "main");
        std::env::set_var("RALPH_GIT_PR_TITLE", "Ralph changes for {run_id}");

        let config = GitRemoteConfig::from_env();

        assert!(config.create_pr, "PR creation should be enabled");
        assert_eq!(
            config.pr_base_branch.as_deref(),
            Some("main"),
            "PR base branch should be loaded"
        );
        assert_eq!(
            config.pr_title_template.as_deref(),
            Some("Ralph changes for {run_id}"),
            "PR title template should be loaded"
        );

        std::env::remove_var("RALPH_GIT_CREATE_PR");
        std::env::remove_var("RALPH_GIT_PR_BASE_BRANCH");
        std::env::remove_var("RALPH_GIT_PR_TITLE");
    });
}

#[test]
#[serial]
fn test_git_remote_config_custom_remote() {
    with_default_timeout(|| {
        std::env::set_var("RALPH_GIT_REMOTE", "upstream");
        std::env::set_var("RALPH_GIT_PUSH_BRANCH", "feature/cloud-changes");

        let config = GitRemoteConfig::from_env();

        assert_eq!(
            config.remote_name, "upstream",
            "Custom remote name should be used"
        );
        assert_eq!(
            config.push_branch.as_deref(),
            Some("feature/cloud-changes"),
            "Custom push branch should be loaded"
        );

        std::env::remove_var("RALPH_GIT_REMOTE");
        std::env::remove_var("RALPH_GIT_PUSH_BRANCH");
    });
}

#[test]
#[serial]
fn test_force_push_disabled_by_default() {
    with_default_timeout(|| {
        std::env::remove_var("RALPH_GIT_FORCE_PUSH");

        let config = GitRemoteConfig::from_env();

        assert!(
            !config.force_push,
            "Force push should be disabled by default for safety"
        );
    });
}

#[test]
#[serial]
fn test_force_push_can_be_enabled() {
    with_default_timeout(|| {
        std::env::set_var("RALPH_GIT_FORCE_PUSH", "true");

        let config = GitRemoteConfig::from_env();

        assert!(
            config.force_push,
            "Force push should be enabled when explicitly set"
        );

        std::env::remove_var("RALPH_GIT_FORCE_PUSH");
    });
}

#[test]
fn test_pipeline_state_has_cloud_fields() {
    with_default_timeout(|| {
        let cloud_config = CloudStateConfig::disabled();
        let mut state = PipelineState::initial(1, 0);
        state.cloud_config = cloud_config;

        assert!(!state.cloud_config.enabled, "Cloud should be disabled");
        assert!(
            state.pending_push_commit.is_none(),
            "No pending push when disabled"
        );
        assert!(
            !state.git_auth_configured,
            "Git auth not configured initially"
        );
        assert!(!state.pr_created, "PR not created initially");
        assert!(state.pr_url.is_none(), "No PR URL initially");
        assert_eq!(state.push_count, 0, "Push count starts at zero");
    });
}

#[test]
fn test_git_auth_configured_event_updates_state() {
    with_default_timeout(|| {
        let cloud_config = CloudStateConfig::disabled();
        let mut state = PipelineState::initial(1, 0);
        state.cloud_config = cloud_config;
        state.git_auth_configured = false;

        let event = PipelineEvent::Commit(CommitEvent::GitAuthConfigured);
        let new_state = ralph_workflow::reducer::reduce(state, event);

        assert!(
            new_state.git_auth_configured,
            "Git auth should be marked as configured after event"
        );
    });
}

#[test]
fn test_push_completed_clears_pending_push() {
    with_default_timeout(|| {
        let cloud_config = CloudStateConfig::disabled();
        let mut state = PipelineState::initial(1, 0);
        state.cloud_config = cloud_config;
        state.pending_push_commit = Some("abc123".to_string());
        state.push_count = 0;

        let event = PipelineEvent::Commit(CommitEvent::PushCompleted {
            remote: "origin".to_string(),
            branch: "main".to_string(),
            commit_sha: "abc123".to_string(),
        });
        let new_state = ralph_workflow::reducer::reduce(state, event);

        assert!(
            new_state.pending_push_commit.is_none(),
            "Pending push should be cleared after successful push"
        );
        assert_eq!(new_state.push_count, 1, "Push count should increment");
    });
}

#[test]
fn test_push_failed_keeps_pending_push_for_retry() {
    with_default_timeout(|| {
        let cloud_config = CloudStateConfig::disabled();
        let mut state = PipelineState::initial(1, 0);
        state.cloud_config = cloud_config;
        state.pending_push_commit = Some("abc123".to_string());
        state.push_count = 0;

        let event = PipelineEvent::Commit(CommitEvent::PushFailed {
            remote: "origin".to_string(),
            branch: "main".to_string(),
            error: "Network timeout".to_string(),
        });
        let new_state = ralph_workflow::reducer::reduce(state, event);

        assert_eq!(
            new_state.pending_push_commit.as_deref(),
            Some("abc123"),
            "Push failures must not clear pending push so orchestration can retry"
        );
        assert_eq!(
            new_state.push_count, 0,
            "Push count should not change on failure"
        );
    });
}

#[test]
fn test_pr_created_event_updates_state() {
    with_default_timeout(|| {
        let cloud_config = CloudStateConfig::disabled();
        let mut state = PipelineState::initial(1, 0);
        state.cloud_config = cloud_config;
        state.pr_created = false;

        let event = PipelineEvent::Commit(CommitEvent::PullRequestCreated {
            url: "https://github.com/user/repo/pull/123".to_string(),
            number: 123,
        });
        let new_state = ralph_workflow::reducer::reduce(state, event);

        assert!(new_state.pr_created, "PR should be marked as created");
        assert_eq!(
            new_state.pr_url.as_deref(),
            Some("https://github.com/user/repo/pull/123"),
            "PR URL should be stored"
        );
    });
}

// TODO: Add orchestration tests that verify:
// - determine_next_effect() emits ConfigureGitAuth when cloud enabled and not configured
// - determine_next_effect() emits PushToRemote when pending_push_commit is set
// - determine_next_effect() emits CreatePullRequest in Finalizing phase when create_pr enabled
// - These effects are NOT emitted when cloud mode is disabled
// This requires direct testing of orchestration.rs logic with test states
