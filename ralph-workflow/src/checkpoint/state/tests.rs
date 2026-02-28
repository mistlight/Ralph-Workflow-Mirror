// Tests for checkpoint state module.
//
// Split into topic-specific test modules for maintainability.

// =========================================================================
// Environment snapshot tests
// =========================================================================

use serial_test::serial;

struct EnvVarGuard {
    name: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(name: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let prior = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, prior }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(v) => std::env::set_var(self.name, v),
            None => std::env::remove_var(self.name),
        }
    }
}

#[test]
#[serial]
fn test_environment_snapshot_filters_sensitive_vars() {
    let _safe = EnvVarGuard::set("RALPH_SAFE_SETTING", "ok");
    let _token = EnvVarGuard::set("RALPH_API_TOKEN", "secret");
    let _editor = EnvVarGuard::set("EDITOR", "vim");

    let snapshot = EnvironmentSnapshot::capture_current();

    assert!(snapshot.ralph_vars.contains_key("RALPH_SAFE_SETTING"));
    assert!(!snapshot.ralph_vars.contains_key("RALPH_API_TOKEN"));
    assert!(snapshot.other_vars.contains_key("EDITOR"));
}

#[test]
#[serial]
fn test_environment_tests_do_not_clobber_prior_env_values() {
    let _original = EnvVarGuard::set("EDITOR", "original");

    {
        let vim_guard = EnvVarGuard::set("EDITOR", "vim");
        drop(vim_guard);
    }

    assert_eq!(
        std::env::var("EDITOR").ok().as_deref(),
        Some("original"),
        "env-muting tests must restore prior values"
    );
}

// Workspace-based tests (feature-gated)
#[path = "tests/workspace_tests.rs"]
#[cfg(feature = "test-utils")]
mod workspace_tests;

// Checkpoint construction and serialization tests
#[path = "tests/checkpoint_construction.rs"]
mod checkpoint_construction;
