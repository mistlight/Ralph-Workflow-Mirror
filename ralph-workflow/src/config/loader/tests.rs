use super::*;
use crate::config::path_resolver::MemoryConfigEnvironment;
use serial_test::serial;
use std::path::Path;

#[test]
#[serial]
fn test_load_config_with_env_from_custom_path() {
    let toml_str = r#"
[general]
verbosity = 3
interactive = false
developer_iters = 10
review_depth = "standard"
"#;
    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_file("/custom/config.toml", toml_str);

    let Ok((config, unified, warnings)) =
        load_config_from_path_with_env(Some(Path::new("/custom/config.toml")), &env)
    else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
    assert!(unified.is_some());
    assert_eq!(config.developer_iters, 10);
    assert!(!config.behavior.interactive);
}

#[test]
#[serial]
fn test_load_config_with_env_missing_file() {
    let env =
        MemoryConfigEnvironment::new().with_unified_config_path("/test/config/ralph-workflow.toml");

    let Ok((config, unified, warnings)) =
        load_config_from_path_with_env(Some(Path::new("/missing/config.toml")), &env)
    else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert!(unified.is_none());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("not found"));
    // Should fall back to defaults
    assert_eq!(config.developer_iters, 5);
}

#[test]
#[serial]
fn test_load_config_with_env_from_default_path() {
    let toml_str = r#"
[general]
verbosity = 4
developer_iters = 8
review_depth = "standard"
"#;
    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", toml_str);

    let Ok((config, unified, warnings)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
    assert!(unified.is_some());
    assert_eq!(config.developer_iters, 8);
    assert_eq!(config.verbosity, Verbosity::Debug);
}

#[test]
fn test_default_config() {
    let config = default_config();
    assert!(config.developer_agent.is_none());
    assert!(config.reviewer_agent.is_none());
    assert_eq!(config.developer_iters, 5);
    assert_eq!(config.reviewer_reviews, 2);
    assert!(config.behavior.interactive);
    assert!(config.isolation_mode);
    assert_eq!(config.verbosity, Verbosity::Verbose);
    assert_eq!(config.max_same_agent_retries, Some(2));
}

#[test]
#[serial]
fn test_apply_env_overrides() {
    // Set some env vars
    env::set_var("RALPH_DEVELOPER_ITERS", "10");
    env::set_var("RALPH_ISOLATION_MODE", "false");

    let mut warnings = Vec::new();
    let config = apply_env_overrides(default_config(), &mut warnings);
    assert_eq!(config.developer_iters, 10);
    assert!(!config.isolation_mode);
    assert!(warnings.is_empty());

    // Clean up
    env::remove_var("RALPH_DEVELOPER_ITERS");
    env::remove_var("RALPH_ISOLATION_MODE");
}

#[test]
fn test_unified_config_exists_with_env_returns_false_when_no_path() {
    // Test when there's no unified config path configured
    let env = MemoryConfigEnvironment::new();
    assert!(!unified_config_exists_with_env(&env));
}

#[test]
fn test_unified_config_exists_with_env_returns_false_when_file_missing() {
    // Test when path is configured but file doesn't exist
    let env =
        MemoryConfigEnvironment::new().with_unified_config_path("/test/config/ralph-workflow.toml");
    assert!(!unified_config_exists_with_env(&env));
}

#[test]
fn test_unified_config_exists_with_env_returns_true_when_file_exists() {
    // Test when path is configured and file exists
    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", "[general]");
    assert!(unified_config_exists_with_env(&env));
}

#[test]
#[serial]
fn test_max_dev_continuations_zero_falls_back_to_default() {
    let toml_str = r#"
[general]
verbosity = 4
developer_iters = 8
review_depth = "standard"
max_dev_continuations = 0
"#;

    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", toml_str);

    let Ok((config, _unified, warnings)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert_eq!(config.max_dev_continuations, Some(2));
    assert!(
        warnings
            .iter()
            .any(|w: &String| w.contains("max_dev_continuations") && w.contains(">= 1")),
        "Expected warning about invalid max_dev_continuations, got: {:?}",
        warnings
    );
}

#[test]
#[serial]
fn test_max_xsd_retries_zero_is_valid() {
    // max_xsd_retries=0 is valid and means "disable XSD retries" (immediate agent fallback)
    let toml_str = r#"
[general]
verbosity = 4
developer_iters = 8
review_depth = "standard"
max_xsd_retries = 0
"#;

    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", toml_str);

    let Ok((config, _unified, warnings)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    // 0 should be accepted (not rejected with warning)
    assert_eq!(config.max_xsd_retries, Some(0));
    assert!(
        !warnings
            .iter()
            .any(|w: &String| w.contains("max_xsd_retries")),
        "Should not warn about max_xsd_retries=0, got: {:?}",
        warnings
    );
}

#[test]
#[serial]
fn test_max_same_agent_retries_zero_is_valid() {
    // max_same_agent_retries=0 is valid and means "disable same-agent retries"
    let toml_str = r#"
[general]
verbosity = 4
developer_iters = 8
review_depth = "standard"
max_same_agent_retries = 0
"#;

    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", toml_str);

    let Ok((config, _unified, warnings)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert_eq!(config.max_same_agent_retries, Some(0));
    assert!(
        !warnings
            .iter()
            .any(|w: &String| w.contains("max_same_agent_retries")),
        "Should not warn about max_same_agent_retries=0, got: {:?}",
        warnings
    );
}

#[test]
#[serial]
fn test_load_config_returns_defaults_without_file() {
    // Clear env vars that might affect the test
    env::remove_var("RALPH_DEVELOPER_AGENT");
    env::remove_var("RALPH_REVIEWER_AGENT");
    env::remove_var("RALPH_DEVELOPER_ITERS");
    env::remove_var("RALPH_VERBOSITY");

    let env = MemoryConfigEnvironment::new();
    let Ok((config, _unified, _warnings)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };
    assert_eq!(config.developer_iters, 5);
    assert_eq!(config.verbosity, Verbosity::Verbose);
}

#[test]
#[serial]
fn test_load_config_with_local_override() {
    let global_toml = r#"
[general]
verbosity = 2
developer_iters = 5
reviewer_reviews = 2
"#;

    let local_toml = r#"
[general]
developer_iters = 10
reviewer_reviews = 3
"#;

    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_local_config_path("/test/project/.agent/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", global_toml)
        .with_file("/test/project/.agent/ralph-workflow.toml", local_toml);

    let Ok((config, _, _)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    // Local overrides take effect
    assert_eq!(config.developer_iters, 10);
    assert_eq!(config.reviewer_reviews, 3);
    // Global value preserved (not overridden)
    assert_eq!(config.verbosity as u8, 2);
}

#[test]
#[serial]
fn test_load_config_local_only() {
    let local_toml = r#"
[general]
verbosity = 4
developer_iters = 8
"#;

    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_local_config_path("/test/project/.agent/ralph-workflow.toml")
        .with_file("/test/project/.agent/ralph-workflow.toml", local_toml);
    // Global config doesn't exist

    let Ok((config, _, _)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert_eq!(config.verbosity as u8, 4);
    assert_eq!(config.developer_iters, 8);
}

#[test]
#[serial]
fn test_load_config_global_only_no_local() {
    let global_toml = r#"
[general]
verbosity = 3
developer_iters = 7
"#;

    let env = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_local_config_path("/test/project/.agent/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", global_toml);
    // Local config doesn't exist

    let Ok((config, _, _)) = load_config_from_path_with_env(None, &env) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    assert_eq!(config.verbosity as u8, 3);
    assert_eq!(config.developer_iters, 7);
}

#[test]
#[serial]
fn test_load_config_precedence_env_vars_override_local() {
    let global_toml = r#"
[general]
developer_iters = 5
"#;

    let local_toml = r#"
[general]
developer_iters = 10
"#;

    let env_impl = MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_local_config_path("/test/project/.agent/ralph-workflow.toml")
        .with_file("/test/config/ralph-workflow.toml", global_toml)
        .with_file("/test/project/.agent/ralph-workflow.toml", local_toml);

    // Set env var to override
    std::env::set_var("RALPH_DEVELOPER_ITERS", "15");

    let Ok((config, _, _)) = load_config_from_path_with_env(None, &env_impl) else {
        panic!("load_config_from_path_with_env should succeed");
    };

    // Env var wins over local config
    assert_eq!(config.developer_iters, 15);

    std::env::remove_var("RALPH_DEVELOPER_ITERS");
}
