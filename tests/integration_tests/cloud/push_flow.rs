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

use ralph_workflow::config::{CloudConfig, Config};
use ralph_workflow::config::{CloudStateConfig, GitAuthMethod, GitRemoteConfig};
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::logger::{Colors, Logger};
use ralph_workflow::pipeline::Timer;
use ralph_workflow::prompts::template_context::TemplateContext;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::event::{CommitEvent, PipelineEvent};
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::workspace::MemoryWorkspace;
use serial_test::serial;

use crate::test_timeout::with_default_timeout;
use std::path::PathBuf;
use std::sync::Arc;

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

#[test]
fn test_push_to_remote_effect_pushes_head_to_named_remote_branch() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRegistry;
        use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
        use ralph_workflow::logging::RunLogContext;
        use ralph_workflow::reducer::effect::{Effect, EffectHandler};
        use ralph_workflow::reducer::handler::MainEffectHandler;

        let config = Config::default();
        let colors = Colors::new();
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let template_context = TemplateContext::default();
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());

        let repo_root = PathBuf::from("/test/repo");
        let workspace = MemoryWorkspace::new(repo_root.clone());
        let run_log_context =
            RunLogContext::new(&workspace).expect("Failed to create run log context");

        let cloud_config = CloudConfig {
            enabled: true,
            api_url: Some("https://example.com".to_string()),
            api_token: Some("token".to_string()),
            run_id: Some("run-123".to_string()),
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteConfig {
                auth_method: GitAuthMethod::SshKey { key_path: None },
                push_branch: None,
                create_pr: false,
                pr_title_template: None,
                pr_body_template: None,
                pr_base_branch: None,
                force_push: false,
                remote_name: "origin".to_string(),
            },
        };

        let mut ctx = ralph_workflow::phases::PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            developer_agent: "test-developer",
            reviewer_agent: "test-reviewer",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*executor,
            executor_arc: Arc::clone(&executor)
                as Arc<dyn ralph_workflow::executor::ProcessExecutor>,
            repo_root: &repo_root,
            workspace: &workspace,
            workspace_arc: Arc::new(workspace.clone())
                as Arc<dyn ralph_workflow::workspace::Workspace>,
            run_log_context: &run_log_context,
            cloud_reporter: None,
            cloud_config: &cloud_config,
        };

        let state = PipelineState::initial(1, 0);
        let mut handler = MainEffectHandler::new(state);

        let effect = Effect::PushToRemote {
            remote: "origin".to_string(),
            branch: "feature/run-123".to_string(),
            force: false,
            commit_sha: "abc123".to_string(),
        };

        let _res = handler
            .execute(effect, &mut ctx)
            .expect("effect execution should succeed");

        let calls = executor.execute_calls_for("git");
        assert_eq!(calls.len(), 1, "Should execute exactly one git command");

        let (_cmd, args, _env, _workdir) = &calls[0];

        let expected_refspec = "HEAD:refs/heads/feature/run-123".to_string();
        assert!(
            args.iter().any(|a| a == &expected_refspec),
            "git push should use an explicit HEAD refspec to avoid requiring a local branch"
        );
        assert!(
            !args.iter().any(|a| a == "feature/run-123"),
            "git push should not reference a local branch by name"
        );
    });
}

fn orchestration_ready_state() -> PipelineState {
    let mut state = PipelineState::initial(1, 0);
    state.prompt_permissions.locked = true;
    state.gitignore_entries_ensured = true;
    state.context_cleaned = true;
    state
}

#[test]
fn test_orchestration_cloud_disabled_never_emits_push_or_pr_effects() {
    with_default_timeout(|| {
        let mut state = orchestration_ready_state();
        state.cloud_config = CloudStateConfig::disabled();
        state.pending_push_commit = Some("abc123".to_string());
        state.git_auth_configured = false;
        state.phase = PipelinePhase::Finalizing;
        state.cloud_config.git_remote.create_pr = true;

        let effect = determine_next_effect(&state);

        assert!(
            !matches!(effect, Effect::ConfigureGitAuth { .. }),
            "Cloud disabled must not emit ConfigureGitAuth"
        );
        assert!(
            !matches!(effect, Effect::PushToRemote { .. }),
            "Cloud disabled must not emit PushToRemote"
        );
        assert!(
            !matches!(effect, Effect::CreatePullRequest { .. }),
            "Cloud disabled must not emit CreatePullRequest"
        );
    });
}

#[test]
fn test_orchestration_cloud_enabled_sequences_auth_then_push() {
    with_default_timeout(|| {
        let mut state = orchestration_ready_state();
        state.cloud_config.enabled = true;
        state.cloud_config.run_id = Some("run_123".to_string());
        state.cloud_config.git_remote.push_branch = "feature/run-123".to_string();
        state.pending_push_commit = Some("abc123".to_string());
        state.git_auth_configured = false;

        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::ConfigureGitAuth { .. }),
            "Should configure auth before pushing"
        );

        state.git_auth_configured = true;
        let effect = determine_next_effect(&state);

        match effect {
            Effect::PushToRemote {
                remote,
                branch,
                force: _,
                commit_sha,
            } => {
                assert_eq!(remote, "origin");
                assert_eq!(branch, "feature/run-123");
                assert_eq!(commit_sha, "abc123");
            }
            other => panic!("Expected PushToRemote, got: {other:?}"),
        }
    });
}

#[test]
fn test_orchestration_cloud_enabled_creates_pr_in_finalizing_when_configured() {
    with_default_timeout(|| {
        let mut state = orchestration_ready_state();
        state.cloud_config.enabled = true;
        state.cloud_config.run_id = Some("run_123".to_string());
        state.cloud_config.git_remote.push_branch = "feature/run-123".to_string();
        state.cloud_config.git_remote.create_pr = true;
        state.cloud_config.git_remote.pr_base_branch = Some("main".to_string());
        state.cloud_config.git_remote.pr_title_template =
            Some("Ralph changes for {run_id}".to_string());
        state.cloud_config.git_remote.pr_body_template =
            Some("Summary: {prompt_summary}".to_string());

        state.phase = PipelinePhase::Finalizing;
        state.pr_created = false;
        state.pending_push_commit = None;
        state.push_count = 1;
        state.last_pushed_commit = Some("abc123".to_string());
        state.metrics.commits_created_total = 1;

        let effect = determine_next_effect(&state);

        match effect {
            Effect::CreatePullRequest {
                base_branch,
                head_branch,
                title,
                body,
            } => {
                assert_eq!(base_branch, "main");
                assert_eq!(head_branch, "feature/run-123");
                assert_eq!(title, "Ralph changes for run_123");
                assert_eq!(body, "Summary: Ralph workflow run run_123");
            }
            other => panic!("Expected CreatePullRequest, got: {other:?}"),
        }
    });
}
