use crate::agents::AgentRegistry;
use crate::checkpoint::execution_history::ExecutionHistory;
use crate::checkpoint::RunContext;
use crate::config::types::{CloudConfig, GitAuthMethod, GitRemoteConfig};
use crate::config::Config;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::PipelineState;
use crate::workspace::MemoryWorkspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

struct Fixture {
    workspace: MemoryWorkspace,
    workspace_arc: Arc<dyn crate::workspace::Workspace>,
    executor: Arc<MockProcessExecutor>,
    config: Config,
    registry: AgentRegistry,
    colors: Colors,
    logger: Logger,
    template_context: TemplateContext,
    timer: Timer,
    repo_root: PathBuf,
    run_log_context: crate::logging::RunLogContext,
    cloud_config: CloudConfig,
}

impl Fixture {
    fn new(cloud_config: CloudConfig) -> Self {
        Self::new_with_executor(cloud_config, Arc::new(MockProcessExecutor::new()))
    }

    fn new_with_executor(cloud_config: CloudConfig, executor: Arc<MockProcessExecutor>) -> Self {
        let workspace = MemoryWorkspace::new_test();
        let workspace_arc = Arc::new(workspace.clone()) as Arc<dyn crate::workspace::Workspace>;
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let template_context = TemplateContext::default();
        let timer = Timer::new();
        let repo_root = PathBuf::from("/mock/repo");
        let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
        Self {
            workspace,
            workspace_arc,
            executor,
            config,
            registry,
            colors,
            logger,
            template_context,
            timer,
            repo_root,
            run_log_context,
            cloud_config,
        }
    }

    fn ctx(&mut self) -> crate::phases::PhaseContext<'_> {
        crate::phases::PhaseContext {
            config: &self.config,
            registry: &self.registry,
            logger: &self.logger,
            colors: &self.colors,
            timer: &mut self.timer,
            developer_agent: "dev",
            reviewer_agent: "rev",
            review_guidelines: None,
            template_context: &self.template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: HashMap::new(),
            executor: self.executor.as_ref(),
            executor_arc: Arc::clone(&self.executor) as Arc<dyn crate::executor::ProcessExecutor>,
            repo_root: self.repo_root.as_path(),
            workspace: &self.workspace,
            workspace_arc: Arc::clone(&self.workspace_arc),
            run_log_context: &self.run_log_context,
            cloud_reporter: None,
            cloud_config: &self.cloud_config,
        }
    }
}

#[test]
fn test_push_to_remote_token_auth_uses_ephemeral_credential_helper() {
    let cloud_config = CloudConfig {
        enabled: true,
        api_url: Some("https://api.example.com".to_string()),
        api_token: Some("secret".to_string()),
        run_id: Some("run_1".to_string()),
        heartbeat_interval_secs: 30,
        graceful_degradation: true,
        git_remote: GitRemoteConfig {
            auth_method: GitAuthMethod::Token {
                token: "ghp_test".to_string(),
                username: "x-access-token".to_string(),
            },
            push_branch: Some("main".to_string()),
            create_pr: false,
            pr_title_template: None,
            pr_body_template: None,
            pr_base_branch: None,
            force_push: false,
            remote_name: "origin".to_string(),
        },
    };

    let mut fixture = Fixture::new(cloud_config);
    let mut ctx = fixture.ctx();
    let handler = MainEffectHandler::new(PipelineState::initial(0, 0));

    let _ = handler
        .handle_push_to_remote(
            &mut ctx,
            "origin".to_string(),
            "main".to_string(),
            false,
            "abc123".to_string(),
        )
        .expect("push handler should succeed with mock executor");

    let calls = fixture.executor.execute_calls_for("git");
    assert_eq!(calls.len(), 1);
    let (_cmd, args, _env, _workdir) = &calls[0];

    assert!(
        args.iter().any(|a| a == "-c"),
        "expected per-command -c overrides for token auth"
    );
    assert!(
        args.iter().any(|a| a.starts_with("credential.helper=!")),
        "expected ephemeral credential helper for token auth"
    );
    assert!(args.contains(&"push".to_string()));
    assert!(args.contains(&"origin".to_string()));
    assert!(
        args.iter().any(|a| a.contains("refs/heads/main")),
        "expected refspec containing 'refs/heads/main', got {:?}",
        args
    );
}

#[test]
fn test_push_to_remote_credential_helper_sets_credential_helper_override() {
    let cloud_config = CloudConfig {
        enabled: true,
        api_url: Some("https://api.example.com".to_string()),
        api_token: Some("secret".to_string()),
        run_id: Some("run_1".to_string()),
        heartbeat_interval_secs: 30,
        graceful_degradation: true,
        git_remote: GitRemoteConfig {
            auth_method: GitAuthMethod::CredentialHelper {
                helper: "gcloud".to_string(),
            },
            push_branch: Some("main".to_string()),
            create_pr: false,
            pr_title_template: None,
            pr_body_template: None,
            pr_base_branch: None,
            force_push: false,
            remote_name: "origin".to_string(),
        },
    };

    let mut fixture = Fixture::new(cloud_config);
    let mut ctx = fixture.ctx();
    let handler = MainEffectHandler::new(PipelineState::initial(0, 0));

    let _ = handler
        .handle_push_to_remote(
            &mut ctx,
            "origin".to_string(),
            "main".to_string(),
            false,
            "abc123".to_string(),
        )
        .expect("push handler should succeed with mock executor");

    let calls = fixture.executor.execute_calls_for("git");
    assert_eq!(calls.len(), 1);
    let (_cmd, args, _env, _workdir) = &calls[0];
    assert!(
        args.iter().any(|a| a == "credential.helper=gcloud"),
        "expected credential.helper override for credential-helper auth"
    );
}

#[test]
fn test_push_to_remote_emits_ui_event_on_success() {
    let cloud_config = CloudConfig {
        enabled: true,
        api_url: Some("https://api.example.com".to_string()),
        api_token: Some("secret".to_string()),
        run_id: Some("run_1".to_string()),
        heartbeat_interval_secs: 30,
        graceful_degradation: true,
        git_remote: GitRemoteConfig {
            auth_method: GitAuthMethod::SshKey { key_path: None },
            push_branch: Some("main".to_string()),
            create_pr: false,
            pr_title_template: None,
            pr_body_template: None,
            pr_base_branch: None,
            force_push: false,
            remote_name: "origin".to_string(),
        },
    };

    let mut fixture = Fixture::new(cloud_config);
    let mut ctx = fixture.ctx();
    let handler = MainEffectHandler::new(PipelineState::initial(0, 0));

    let result = handler
        .handle_push_to_remote(
            &mut ctx,
            "origin".to_string(),
            "main".to_string(),
            false,
            "abc123".to_string(),
        )
        .expect("push handler should run");

    assert!(
        result.ui_events.iter().any(|e| matches!(
            e,
            crate::reducer::ui_event::UIEvent::PushCompleted {
                remote,
                branch,
                commit_sha
            } if remote == "origin" && branch == "main" && commit_sha == "abc123"
        )),
        "expected PushCompleted UIEvent"
    );
}

#[test]
fn test_push_to_remote_emits_ui_event_on_failure_with_redacted_error() {
    let cloud_config = CloudConfig {
        enabled: true,
        api_url: Some("https://api.example.com".to_string()),
        api_token: Some("secret".to_string()),
        run_id: Some("run_1".to_string()),
        heartbeat_interval_secs: 30,
        graceful_degradation: true,
        git_remote: GitRemoteConfig {
            auth_method: GitAuthMethod::SshKey { key_path: None },
            push_branch: Some("main".to_string()),
            create_pr: false,
            pr_title_template: None,
            pr_body_template: None,
            pr_base_branch: None,
            force_push: false,
            remote_name: "origin".to_string(),
        },
    };

    let executor = Arc::new(MockProcessExecutor::new().with_error(
        "git",
        "HTTP 401: Bearer SECRET_TOKEN https://user:pass@example.com?access_token=abc",
    ));
    let mut fixture = Fixture::new_with_executor(cloud_config, executor);
    let mut ctx = fixture.ctx();
    let handler = MainEffectHandler::new(PipelineState::initial(0, 0));

    let result = handler
        .handle_push_to_remote(
            &mut ctx,
            "origin".to_string(),
            "main".to_string(),
            false,
            "abc123".to_string(),
        )
        .expect("push handler should run");

    let mut saw = false;
    for e in &result.ui_events {
        if let crate::reducer::ui_event::UIEvent::PushFailed { error, .. } = e {
            assert!(
                !error.contains("SECRET_TOKEN"),
                "should redact token: {error}"
            );
            assert!(
                !error.contains("user:pass"),
                "should redact userinfo: {error}"
            );
            assert!(
                error.contains("<redacted>"),
                "should contain redaction marker: {error}"
            );
            saw = true;
        }
    }
    assert!(saw, "expected PushFailed UIEvent");
}
