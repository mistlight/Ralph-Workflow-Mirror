//! Git worktree config discovery, agent chain merge, and init-local-config tests.
//!
//! Tests for worktree-aware config discovery, partial `agent_chain` merging
//! with global/built-in defaults, `--init-local-config` value population,
//! and prompt validation fail-fast behavior.
//!
//! **CRITICAL:** Follow the integration test style guide in
//! **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::config::{ConfigEnvironment, MemoryConfigEnvironment};
use std::path::PathBuf;

use crate::common::{
    create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_env,
    run_ralph_cli_with_handler,
};
use crate::test_timeout::with_default_timeout;

use super::STANDARD_PROMPT;

// ============================================================================
// Git Worktree Config Tests
// ============================================================================

/// Test that local config is found when running from worktree subdirectory.
///
/// This verifies that when ralph is run from a subdirectory of a git worktree,
/// the local config at the worktree root is discovered and used.
#[test]
fn test_worktree_config_discovery_from_subdirectory() {
    with_default_timeout(|| {
        // Simulate being in /test/worktree/src/components/
        // with config at /test/worktree/.agent/ralph-workflow.toml
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_worktree_root("/test/worktree")
            .with_prompt_path("/test/worktree/PROMPT.md")
            .with_file(
                "/test/worktree/.agent/ralph-workflow.toml",
                "[general]\ndeveloper_iters = 3",
            )
            .with_file("/test/worktree/PROMPT.md", STANDARD_PROMPT);

        // Validate discovery/merge (worktree root implies local config lives at worktree root).
        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("worktree config should load");
        let unified = merged.expect("expected unified config from local worktree config");

        assert_eq!(unified.general.developer_iters, 3);
    });
}

/// Test that --init-local-config creates config at worktree root.
///
/// This verifies that when --init-local-config is run from a subdirectory,
/// the config file is created at the worktree root, not in CWD.
#[test]
fn test_worktree_init_local_config_from_subdirectory() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/worktree/src"));

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_worktree_root("/test/worktree")
            .with_prompt_path("/test/worktree/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2",
            )
            .with_file("/test/worktree/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_env(
            &["--init-local-config"],
            executor,
            config,
            &mut handler,
            &env,
        )
        .unwrap();

        // Config should be created at worktree root, not in CWD
        assert!(
            env.was_written(std::path::Path::new(
                "/test/worktree/.agent/ralph-workflow.toml"
            )),
            "Local config should be created at worktree root"
        );
        assert!(
            !env.was_written(std::path::Path::new(
                "/test/worktree/src/.agent/ralph-workflow.toml"
            )),
            "Local config should NOT be created in subdirectory"
        );
    });
}

/// Test that config discovery falls back gracefully outside git repos.
///
/// This verifies that when not in a git repository, the system falls back
/// to the current CWD-relative behavior without errors.
#[test]
fn test_config_discovery_outside_git_repo() {
    with_default_timeout(|| {
        // No worktree_root set, simulating being outside a git repo
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path(".agent/ralph-workflow.toml")
            .with_prompt_path("PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2",
            )
            .with_file(
                ".agent/ralph-workflow.toml",
                "[general]\ndeveloper_iters = 5",
            )
            .with_file("PROMPT.md", STANDARD_PROMPT);

        // Validate config loading directly without starting the pipeline.
        // This isolates the behavior under test (path resolution and merge) and
        // avoids unrelated pipeline execution timeouts.
        let ok = ralph_workflow::cli::handle_check_config_with(
            ralph_workflow::logger::Colors::new(),
            &env,
            false,
        )
        .is_ok();

        assert!(ok, "Config discovery should work outside git repo");
    });
}

/// Test that partial local `agent_chain` merges with global chain.
///
/// This verifies that when local config defines only `developer` chain
/// and global config defines both `developer` and `reviewer`, the merged
/// config has both chains with local's developer overriding global's.
#[test]
fn test_partial_local_chain_with_global_completion() {
    with_default_timeout(|| {
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                r#"
[general]
verbosity = 2

[agent_chain]
developer = ["claude"]
reviewer = ["claude"]
commit = ["claude"]
"#,
            )
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                r#"
[agent_chain]
developer = ["codex"]
"#,
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("config should load");
        let unified = merged.expect("expected merged unified config");

        let chain = unified.agent_chain.expect("agent_chain should exist");
        // Local developer overrides global
        assert_eq!(chain.developer, vec!["codex"]);
        // Global reviewer preserved
        assert_eq!(chain.reviewer, vec!["claude"]);
        // Global commit preserved
        assert_eq!(chain.commit, vec!["claude"]);
    });
}

/// Test that explicitly empty local chain entries override global chain entries.
#[test]
fn test_local_empty_chain_entry_overrides_global() {
    with_default_timeout(|| {
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                r#"
[general]
verbosity = 2

[agent_chain]
developer = ["claude"]
reviewer = ["claude"]
commit = ["claude"]
"#,
            )
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                r"
[agent_chain]
reviewer = []
",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("config should load");
        let unified = merged.expect("expected merged unified config");
        let chain = unified.agent_chain.expect("agent_chain should exist");

        assert_eq!(chain.developer, vec!["claude"]);
        assert!(
            chain.reviewer.is_empty(),
            "explicit empty local reviewer should override global reviewer"
        );
        assert_eq!(chain.commit, vec!["claude"]);
    });
}

/// Test that worktree init and runtime use the same canonical path.
///
/// This verifies that both --init-local-config and runtime config loading
/// resolve to the same local config path when running inside a worktree.
#[test]
fn test_worktree_init_and_runtime_use_same_path() {
    with_default_timeout(|| {
        // Simulate worktree: canonical root is /test/main-repo
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_worktree_root("/test/main-repo")
            .with_prompt_path("/test/main-repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2",
            )
            .with_file(
                "/test/main-repo/.agent/ralph-workflow.toml",
                "[general]\ndeveloper_iters = 7",
            )
            .with_file("/test/main-repo/PROMPT.md", STANDARD_PROMPT);

        // Runtime loading should find config at canonical root
        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("config should load from worktree");
        let unified = merged.expect("expected merged unified config");
        assert_eq!(unified.general.developer_iters, 7);

        // Verify local_config_path() resolves to canonical root
        let local_path = env.local_config_path().unwrap();
        assert_eq!(
            local_path,
            std::path::PathBuf::from("/test/main-repo/.agent/ralph-workflow.toml"),
            "local_config_path should resolve to canonical repo root"
        );
    });
}

/// Test that local config only (no global) works with defaults fallback.
///
/// This verifies that when only a local config exists and no global config,
/// missing keys resolve from built-in defaults.
#[test]
fn test_local_config_only_with_defaults_fallback() {
    with_default_timeout(|| {
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                "[general]\nverbosity = 4",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("local-only config should load");
        let unified = merged.expect("expected merged config");

        // Explicitly set field
        assert_eq!(unified.general.verbosity, 4);
        // Default values for unset fields
        assert_eq!(unified.general.developer_iters, 5);
        assert_eq!(unified.general.reviewer_reviews, 2);
    });
}

/// Test that local-only partial `agent_chain` inherits missing roles from built-in defaults.
#[test]
fn test_local_only_partial_agent_chain_inherits_builtin_missing_roles() {
    with_default_timeout(|| {
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/repo/.agent/ralph-workflow.toml",
                r#"
[general]
verbosity = 4

[agent_chain]
developer = ["codex"]
"#,
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("local-only partial chain should load");
        let unified = merged.expect("expected merged config");
        let chain = unified.agent_chain.expect("agent_chain should exist");

        assert_eq!(chain.developer, vec!["codex"]);

        // Missing local roles should inherit non-empty built-in defaults.
        // We verify the behavioral property (chains are populated) rather than
        // pinning specific agent names, which are a configuration detail.
        assert!(
            !chain.reviewer.is_empty(),
            "missing local reviewer should inherit non-empty built-in reviewer chain"
        );
        assert!(
            !chain.commit.is_empty(),
            "missing local commit should inherit non-empty built-in commit chain"
        );
    });
}

/// Test that global-only partial `agent_chain` inherits missing roles from built-in defaults.
#[test]
fn test_global_only_partial_agent_chain_inherits_builtin_missing_roles() {
    with_default_timeout(|| {
        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                r#"
[general]
verbosity = 4

[agent_chain]
developer = ["codex"]
"#,
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let (_config, merged, _warnings) =
            ralph_workflow::config::loader::load_config_from_path_with_env(None, &env)
                .expect("global-only partial chain should load");
        let unified = merged.expect("expected merged config");
        let chain = unified.agent_chain.expect("agent_chain should exist");

        assert_eq!(chain.developer, vec!["codex"]);

        // Missing global roles should inherit non-empty built-in defaults.
        // We verify the behavioral property (chains are populated) rather than
        // pinning specific agent names, which are a configuration detail.
        assert!(
            !chain.reviewer.is_empty(),
            "missing global reviewer should inherit non-empty built-in reviewer chain"
        );
        assert!(
            !chain.commit.is_empty(),
            "missing global commit should inherit non-empty built-in commit chain"
        );
    });
}

/// Test that --init-local-config populates values from global config.
///
/// When global config has custom values (e.g. `developer_iters = 8`),
/// the generated local config should show those values (commented out)
/// rather than built-in defaults.
#[test]
fn test_init_local_config_populates_from_global() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"))
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\ndeveloper_iters = 8\nreviewer_reviews = 4",
            )
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_env(
            &["--init-local-config"],
            executor,
            config,
            &mut handler,
            &env,
        )
        .unwrap();

        let local_path = std::path::Path::new("/test/repo/.agent/ralph-workflow.toml");
        assert!(
            env.was_written(local_path),
            "local config should be created"
        );

        let content = env.get_file(local_path).expect("local config content");
        assert!(
            content.contains("developer_iters = 8"),
            "should reflect global value 8, got:\n{content}"
        );
        assert!(
            content.contains("reviewer_reviews = 4"),
            "should reflect global value 4, got:\n{content}"
        );
    });
}

/// Test that --init-local-config uses built-in defaults when no global config exists.
#[test]
fn test_init_local_config_uses_defaults_when_no_global() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/repo"))
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_local_config_path("/test/repo/.agent/ralph-workflow.toml")
            .with_prompt_path("/test/repo/PROMPT.md")
            // No global config file exists
            .with_file("/test/repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_env(
            &["--init-local-config"],
            executor,
            config,
            &mut handler,
            &env,
        )
        .unwrap();

        let local_path = std::path::Path::new("/test/repo/.agent/ralph-workflow.toml");
        let content = env.get_file(local_path).expect("local config content");
        // Should use built-in defaults (developer_iters=5, reviewer_reviews=2)
        assert!(
            content.contains("developer_iters = 5"),
            "should use default developer_iters=5, got:\n{content}"
        );
        assert!(
            content.contains("reviewer_reviews = 2"),
            "should use default reviewer_reviews=2, got:\n{content}"
        );

        // Verify that agent_chain section is present with non-empty default chains.
        // We check for the structural property (chains are populated) rather than
        // pinning specific agent names derived from production code at runtime.
        assert!(
            content.contains("developer = ["),
            "should show developer chain in generated config, got:\n{content}"
        );
        assert!(
            content.contains("reviewer = ["),
            "should show reviewer chain in generated config, got:\n{content}"
        );
    });
}

/// Test that --init-local-config from deep worktree subdirectory writes to canonical root.
///
/// When running from a deeply nested directory inside a worktree,
/// the config must be written at the canonical repository root.
#[test]
fn test_init_local_config_from_deep_worktree_subdirectory() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/test/worktree/src/components/deep"));

        let env = MemoryConfigEnvironment::new()
            .with_unified_config_path("/test/config/ralph-workflow.toml")
            .with_worktree_root("/test/main-repo")
            .with_prompt_path("/test/main-repo/PROMPT.md")
            .with_file(
                "/test/config/ralph-workflow.toml",
                "[general]\nverbosity = 2",
            )
            .with_file("/test/main-repo/PROMPT.md", STANDARD_PROMPT);

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        run_ralph_cli_with_env(
            &["--init-local-config"],
            executor,
            config,
            &mut handler,
            &env,
        )
        .unwrap();

        assert!(
            env.was_written(std::path::Path::new(
                "/test/main-repo/.agent/ralph-workflow.toml"
            )),
            "Config should be at canonical repo root, not deep subdirectory"
        );
    });
}

/// Regression test: prompt validation failures must abort pipeline startup.
///
/// This guards against accidentally swallowing `validate_prompt_and_setup_backup()`
/// errors during `execution_core` refactors.
#[test]
fn test_pipeline_fails_fast_on_invalid_prompt_content() {
    with_default_timeout(|| {
        let mut handler = MockAppEffectHandler::new()
            .with_head_oid("a".repeat(40))
            .with_cwd(PathBuf::from("/mock/repo"))
            // Empty prompt is always a hard validation error (strict/non-strict)
            .with_file("PROMPT.md", "")
            .with_file(".agent/PLAN.md", "Test plan\n");

        let config = create_test_config_struct();
        let executor = mock_executor_with_success();

        let result = run_ralph_cli_with_handler(&[], executor, config, &mut handler);

        assert!(
            result.is_err(),
            "Pipeline should fail when PROMPT.md validation fails"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("PROMPT.md validation errors"),
            "Error should include PROMPT.md validation failure: {err_msg}"
        );

        // Ensure backup creation was never reached after validation failure.
        assert!(
            !handler.file_exists(&PathBuf::from(".agent/PROMPT.md.backup")),
            "Backup must not be created when validation fails"
        );
    });
}
