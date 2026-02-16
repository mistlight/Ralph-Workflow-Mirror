//! Tests verifying that cloud mode disabled = zero behavior change.
//!
//! These tests verify the critical acceptance criterion: when RALPH_CLOUD_MODE
//! is unset or false, behavior is IDENTICAL to the CLI before cloud support was added.
//!
//! This means:
//! - No API calls occur
//! - No git push occurs
//! - Commits remain local
//! - No heartbeats
//! - No progress reporting
//! - No cloud-specific effects

use ralph_workflow::config::CloudConfig;
use ralph_workflow::reducer::effect::{Effect, EffectHandler, EffectResult};
use ralph_workflow::reducer::event::{LifecycleEvent, PipelineEvent};
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::ui_event::UIEvent;
use serial_test::serial;

use crate::test_timeout::with_default_timeout;

#[test]
#[serial]
fn test_cloud_mode_disabled_by_default() {
    with_default_timeout(|| {
        // Ensure no cloud env vars are set
        std::env::remove_var("RALPH_CLOUD_MODE");
        std::env::remove_var("RALPH_CLOUD_API_URL");
        std::env::remove_var("RALPH_CLOUD_API_TOKEN");
        std::env::remove_var("RALPH_CLOUD_RUN_ID");

        let config = CloudConfig::from_env();

        assert!(
            !config.enabled,
            "Cloud mode should be disabled by default when RALPH_CLOUD_MODE is not set"
        );
        assert!(
            config.api_url.is_none(),
            "API URL should be None when disabled"
        );
        assert!(
            config.api_token.is_none(),
            "API token should be None when disabled"
        );
        assert!(
            config.run_id.is_none(),
            "Run ID should be None when disabled"
        );
    });
}

#[test]
#[serial]
fn test_cloud_mode_explicitly_disabled() {
    with_default_timeout(|| {
        // Explicitly set RALPH_CLOUD_MODE to false
        std::env::set_var("RALPH_CLOUD_MODE", "false");

        let config = CloudConfig::from_env();

        assert!(
            !config.enabled,
            "Cloud mode should be disabled when RALPH_CLOUD_MODE=false"
        );

        std::env::remove_var("RALPH_CLOUD_MODE");
    });
}

#[test]
#[serial]
fn test_cloud_mode_disabled_ignores_other_vars() {
    with_default_timeout(|| {
        // Set other cloud vars but leave RALPH_CLOUD_MODE unset
        std::env::remove_var("RALPH_CLOUD_MODE");
        std::env::set_var("RALPH_CLOUD_API_URL", "https://api.example.com");
        std::env::set_var("RALPH_CLOUD_API_TOKEN", "secret");
        std::env::set_var("RALPH_CLOUD_RUN_ID", "run123");

        let config = CloudConfig::from_env();

        assert!(
            !config.enabled,
            "Cloud mode should be disabled even if other vars are set"
        );
        // The other vars are not loaded when disabled
        assert!(config.api_url.is_none());
        assert!(config.api_token.is_none());
        assert!(config.run_id.is_none());

        std::env::remove_var("RALPH_CLOUD_API_URL");
        std::env::remove_var("RALPH_CLOUD_API_TOKEN");
        std::env::remove_var("RALPH_CLOUD_RUN_ID");
    });
}

#[test]
fn test_cloud_config_disabled_validation_passes() {
    with_default_timeout(|| {
        let config = CloudConfig::disabled();

        assert!(
            config.validate().is_ok(),
            "Disabled cloud config should always validate without required fields"
        );
    });
}

#[test]
#[serial]
fn test_cloud_mode_case_insensitive() {
    with_default_timeout(|| {
        // Test various capitalizations
        for value in &["FALSE", "False", "false", "0"] {
            std::env::set_var("RALPH_CLOUD_MODE", value);
            let config = CloudConfig::from_env();
            assert!(
                !config.enabled,
                "Cloud mode should be disabled for value: {}",
                value
            );
        }

        std::env::remove_var("RALPH_CLOUD_MODE");
    });
}

#[test]
fn test_disabled_config_has_safe_defaults() {
    with_default_timeout(|| {
        let config = CloudConfig::disabled();

        assert!(!config.enabled);
        assert!(config.api_url.is_none());
        assert!(config.api_token.is_none());
        assert!(config.run_id.is_none());
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert!(config.graceful_degradation);
    });
}

#[test]
#[serial]
fn test_git_remote_config_defaults_when_disabled() {
    with_default_timeout(|| {
        std::env::remove_var("RALPH_CLOUD_MODE");
        std::env::remove_var("RALPH_GIT_AUTH_METHOD");
        std::env::remove_var("RALPH_GIT_TOKEN");
        std::env::remove_var("RALPH_GIT_CREATE_PR");
        std::env::remove_var("RALPH_GIT_REMOTE");

        let config = CloudConfig::from_env();

        // Git remote config should have safe defaults even when cloud disabled
        assert!(!config.git_remote.create_pr);
        assert!(!config.git_remote.force_push);
        assert_eq!(config.git_remote.remote_name, "origin");
    });
}

#[test]
#[serial]
fn test_cloud_env_var_variations_respected() {
    with_default_timeout(|| {
        // Test that empty string counts as disabled
        std::env::set_var("RALPH_CLOUD_MODE", "");
        let config = CloudConfig::from_env();
        assert!(!config.enabled, "Empty string should disable cloud mode");

        // Test that random values count as disabled
        std::env::set_var("RALPH_CLOUD_MODE", "maybe");
        let config = CloudConfig::from_env();
        assert!(
            !config.enabled,
            "Non-true/1 values should disable cloud mode"
        );

        std::env::remove_var("RALPH_CLOUD_MODE");
    });
}

#[test]
fn test_cloud_mode_disabled_does_not_report_progress_even_if_reporter_is_injected() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRegistry;
        use ralph_workflow::app::event_loop::{
            run_event_loop_with_handler, EventLoopConfig, StatefulHandler,
        };
        use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
        use ralph_workflow::cloud::MockCloudReporter;
        use ralph_workflow::config::Config;
        use ralph_workflow::executor::MockProcessExecutor;
        use ralph_workflow::logger::{Colors, Logger};
        use ralph_workflow::pipeline::Timer;
        use ralph_workflow::prompts::template_context::TemplateContext;
        use ralph_workflow::workspace::MemoryWorkspace;
        use std::path::PathBuf;
        use std::sync::Arc;

        #[derive(Debug)]
        struct OneShotHandler {
            state: PipelineState,
        }

        impl<'ctx> EffectHandler<'ctx> for OneShotHandler {
            fn execute(
                &mut self,
                _effect: Effect,
                _ctx: &mut ralph_workflow::phases::PhaseContext<'_>,
            ) -> anyhow::Result<EffectResult> {
                Ok(
                    EffectResult::event(PipelineEvent::Lifecycle(LifecycleEvent::Completed))
                        .with_ui_event(UIEvent::AgentActivity {
                            agent: "dev-agent".to_string(),
                            message: "token=SECRET_VALUE".to_string(),
                        }),
                )
            }
        }

        impl StatefulHandler for OneShotHandler {
            fn update_state(&mut self, state: PipelineState) {
                self.state = state;
            }
        }

        let config = Config::default();
        let colors = Colors::new();
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let template_context = TemplateContext::default();
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());

        let repo_root = PathBuf::from("/test/repo");
        let workspace = MemoryWorkspace::new(repo_root.clone());
        let run_log_context = ralph_workflow::logging::RunLogContext::new(&workspace)
            .expect("Failed to create run log context");

        let cloud_config = CloudConfig::disabled();
        let reporter = MockCloudReporter::new();

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
            run_log_context: &run_log_context,
            cloud_reporter: Some(&reporter),
            cloud_config: &cloud_config,
        };

        let initial_state = PipelineState::initial(1, 0);
        let mut handler = OneShotHandler {
            state: initial_state.clone(),
        };

        let loop_config = EventLoopConfig { max_iterations: 5 };
        let _ =
            run_event_loop_with_handler(&mut ctx, Some(initial_state), loop_config, &mut handler)
                .expect("event loop should run");

        assert_eq!(
            reporter.progress_count(),
            0,
            "Cloud mode disabled must not emit progress updates"
        );
    });
}
