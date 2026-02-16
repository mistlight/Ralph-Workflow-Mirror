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

// TODO: Add full pipeline integration test that:
// - Runs ralph-workflow with cloud mode disabled (no env vars)
// - Verifies no cloud effects are emitted
// - Verifies commits remain local (no push)
// - Verifies behavior is identical to pre-cloud-support CLI
// This requires setting up a full MockAppEffectHandler test with mocked git operations
