use crate::config::path_resolver::{ConfigEnvironment, RealConfigEnvironment};

/// Check if the unified config file exists.
pub fn unified_config_exists() -> bool {
    unified_config_exists_with_env(&RealConfigEnvironment)
}

/// Check if the unified config file exists using a [`ConfigEnvironment`].
///
/// This is the testable version of [`unified_config_exists`]. It uses the provided
/// environment for path resolution and filesystem operations.
pub fn unified_config_exists_with_env(env: &dyn ConfigEnvironment) -> bool {
    env.unified_config_path()
        .is_some_and(|p| env.file_exists(&p))
}
