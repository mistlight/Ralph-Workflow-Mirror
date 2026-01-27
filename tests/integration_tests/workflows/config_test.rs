//! Test to reproduce the --init bug where ralph continues to run the pipeline
//! after --init should have exited.
//!
//! The bug: When --init is used, the system should exit cleanly after
//! initialization. It should NEVER continue to run the AI pipeline.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (exit codes, file creation)
//! - Uses `MemoryConfigEnvironment` for config file operations (no real filesystem)
//! - Tests are deterministic and isolated

use std::path::Path;

use ralph_workflow::config::MemoryConfigEnvironment;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use std::path::PathBuf;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_env,
};
use crate::test_timeout::with_default_timeout;

/// Test that `ralph --init` exits cleanly without running the pipeline.
///
/// This verifies that when --init flag is used, the system exits
/// successfully after initialization without running the AI pipeline.
#[test]
fn test_ralph_init_exits_cleanly() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        // Create in-memory environment with paths configured
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md");

        // Run ralph --init with injected config and environment
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_env(&["--init"], executor, config, &mut handler, &env).unwrap();

        // --init in non-interactive mode without template and no config
        // should create the config file
        assert!(
            env.was_written(Path::new("/test/config/ralph-workflow.toml")),
            "Config should be created when --init is used without existing config"
        );
    });
}

/// Test that `ralph --init bug-fix` creates PROMPT.md and exits.
///
/// This verifies that when --init=bug-fix is used, the system creates
/// the PROMPT.md template file and exits without running the pipeline.
#[test]
fn test_ralph_init_with_template_exits_cleanly() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        // Create in-memory environment with existing config (so --init focuses on PROMPT.md)
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/config/ralph-workflow.toml", "# existing config");

        // Run ralph --init bug-fix with injected config
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        run_ralph_cli_with_env(&["--init=bug-fix"], executor, config, &mut handler, &env).unwrap();

        // Should have created PROMPT.md from bug-fix template
        assert!(
            env.was_written(Path::new("/test/repo/PROMPT.md")),
            "PROMPT.md should be created"
        );

        // Verify it contains bug-fix template content
        let content = env.get_file(Path::new("/test/repo/PROMPT.md")).unwrap();
        assert!(
            content.contains("Bug") || content.contains("bug"),
            "PROMPT.md should contain bug-fix template content"
        );
    });
}

/// Test that `ralph --init` when both config and PROMPT.md exist exits cleanly.
///
/// This verifies that when setup is complete and --init is run, the system
/// shows "Setup complete" message and exits without running the pipeline.
#[test]
fn test_ralph_init_when_setup_complete_exits_cleanly() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        // Create in-memory environment with both files already existing
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/config/ralph-workflow.toml", "# existing config")
            .with_file("/test/repo/PROMPT.md", "## Goal\n\nTest task\n");

        // Run ralph --init with injected config
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();
        let result = run_ralph_cli_with_env(&["--init"], executor, config, &mut handler, &env);

        // Should exit successfully
        assert!(
            result.is_ok(),
            "ralph --init should succeed when setup is complete"
        );
    });
}

/// Test that `ralph --init` with an invalid template name exits cleanly.
///
/// This verifies that when an invalid template name is provided, the system
/// shows an error message and exits without running the pipeline.
#[test]
fn test_ralph_init_with_invalid_template_exits_cleanly() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        // Create in-memory environment
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/config/ralph-workflow.toml", "# existing config");

        // Run ralph --init with an invalid template name
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should exit successfully (even though template is invalid)
        let result = run_ralph_cli_with_env(
            &["--init=not-a-real-template"],
            executor,
            config,
            &mut handler,
            &env,
        );
        assert!(
            result.is_ok(),
            "ralph --init=not-a-real-template should exit successfully"
        );

        // PROMPT.md should NOT be created for invalid template
        assert!(
            !env.was_written(Path::new("/test/repo/PROMPT.md")),
            "PROMPT.md should not be created for invalid template"
        );
    });
}

/// Test that `ralph --init` with commit message treats it as template value.
///
/// This verifies that when --init is passed with a commit message positionally,
/// the system interprets it as the template value and exits without running pipeline.
#[test]
fn test_ralph_init_with_commit_message_exits_cleanly() {
    with_default_timeout(|| {
        // Create mock handler for app effects
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"));

        // Create in-memory environment
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file("/test/config/ralph-workflow.toml", "# existing config");

        // Run ralph --init "my commit message"
        // clap will interpret "my commit message" as the value for --init
        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        // Should exit successfully (treating "my commit message" as invalid template)
        let result = run_ralph_cli_with_env(
            &["--init", "my commit message"],
            executor,
            config,
            &mut handler,
            &env,
        );
        assert!(
            result.is_ok(),
            "ralph --init with commit message should exit successfully"
        );

        // PROMPT.md should NOT be created for invalid template value
        assert!(
            !env.was_written(Path::new("/test/repo/PROMPT.md")),
            "PROMPT.md should not be created for invalid template"
        );
    });
}
