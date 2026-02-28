//! Init and agent chain defaults tests.
//!
//! Tests for `--init-global` and agent chain first-entry defaults.
//!
//! **CRITICAL:** Follow the integration test style guide in
//! **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::config::MemoryConfigEnvironment;
use std::path::PathBuf;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_env,
    run_ralph_cli_with_handlers,
};
use crate::test_timeout::with_default_timeout;

use super::create_config_test_handlers;

// ============================================================================
// Config and Init Tests
// ============================================================================

/// Test that ralph --init-global creates unified config file.
///
/// This verifies that when ralph --init-global is run, the system
/// creates ralph-workflow.toml using the injected `ConfigEnvironment`.
#[test]
fn test_init_global_creates_config() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"))
            .with_file("/test/repo/PROMPT.md", "## Goal\n\nTest task\n");

        // Create in-memory environment - no config exists yet
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/repo/PROMPT.md", "## Goal\n\nTest task\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_env(&["--init-global"], executor, config, &mut handler, &env).unwrap();

        // Should have created the config file
        assert!(
            env.was_written(std::path::Path::new("/test/config/ralph-workflow.toml")),
            "Unified config file should be created"
        );

        // Verify it contains expected content
        let content = env
            .get_file(std::path::Path::new("/test/config/ralph-workflow.toml"))
            .unwrap();
        assert!(
            content.contains("[general]") || content.contains("[agents"),
            "Config file should contain expected sections"
        );
    });
}

/// Test that agent chain first entries are used as default agents.
///
/// This verifies that when no explicit agent selection is made, the system
/// uses the first entry in the `agent_chain` configuration.
#[test]
fn test_uses_agent_chain_first_entries_as_defaults() {
    with_default_timeout(|| {
        let (mut app_handler, mut effect_handler) = create_config_test_handlers();

        // Create config with specific agents (simulating first entries from chain)
        let config = create_test_config_struct()
            .with_developer_agent("claude".to_string())
            .with_reviewer_agent("aider".to_string());

        let executor = mock_executor_with_success();
        let result = run_ralph_cli_with_handlers(
            &[],
            executor,
            config,
            &mut app_handler,
            &mut effect_handler,
        );

        assert!(
            result.is_ok(),
            "Pipeline should succeed with custom agent chain"
        );
    });
}
