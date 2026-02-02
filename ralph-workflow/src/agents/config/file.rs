use super::types::{AgentConfigToml, DEFAULT_AGENTS_TOML};
use crate::agents::ccs_env::CcsEnvVarsError;
use crate::agents::fallback::FallbackConfig;
use crate::workspace::{Workspace, WorkspaceFs};
use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

// Note: Legacy global config directory functions (global_config_dir, global_agents_config_path)
// have been removed. Use unified config path from the config module instead.

/// Root TOML configuration structure.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentsConfigFile {
    /// Map of agent name to configuration.
    #[serde(default)]
    pub agents: HashMap<String, AgentConfigToml>,
    /// Agent chain configuration (preferred agents + fallbacks).
    #[serde(default, rename = "agent_chain")]
    pub fallback: FallbackConfig,
}

/// Error type for agent configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum AgentConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] io::Error),
    #[error("Failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Built-in agents.toml template is invalid TOML: {0}")]
    DefaultTemplateToml(toml::de::Error),
    #[error("{0}")]
    CcsEnvVars(#[from] CcsEnvVarsError),
}

/// Result of checking/initializing the agents config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigInitResult {
    /// Config file already exists, no action taken.
    AlreadyExists,
    /// Config file was just created from template.
    Created,
}

impl AgentsConfigFile {
    /// Load agents config from a file, returning None if file doesn't exist.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Option<Self>, AgentConfigError> {
        let path = path.as_ref();
        let workspace = WorkspaceFs::new(PathBuf::from("."));

        if !workspace.exists(path) {
            return Ok(None);
        }

        let contents = workspace.read(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Load agents config from a file using workspace abstraction.
    ///
    /// This is the architecture-conformant version that uses the Workspace trait
    /// instead of direct filesystem access, allowing for proper testing with
    /// MemoryWorkspace.
    pub fn load_from_file_with_workspace(
        path: &Path,
        workspace: &dyn Workspace,
    ) -> Result<Option<Self>, AgentConfigError> {
        if !workspace.exists(path) {
            return Ok(None);
        }

        let contents = workspace
            .read(path)
            .map_err(|e| AgentConfigError::Io(io::Error::other(e)))?;
        let config: Self = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Ensure agents config file exists, creating it from template if needed.
    pub fn ensure_config_exists<P: AsRef<Path>>(path: P) -> io::Result<ConfigInitResult> {
        let path = path.as_ref();
        let workspace = WorkspaceFs::new(PathBuf::from("."));

        if workspace.exists(path) {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            workspace.create_dir_all(parent)?;
        }

        // Write the default template
        workspace.write(path, DEFAULT_AGENTS_TOML)?;

        Ok(ConfigInitResult::Created)
    }

    /// Ensure agents config file exists using workspace abstraction.
    ///
    /// This is the architecture-conformant version that uses the Workspace trait
    /// instead of direct filesystem access, allowing for proper testing with
    /// MemoryWorkspace.
    pub fn ensure_config_exists_with_workspace(
        path: &Path,
        workspace: &dyn Workspace,
    ) -> io::Result<ConfigInitResult> {
        if workspace.exists(path) {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            workspace.create_dir_all(parent)?;
        }

        // Write the default template
        workspace.write(path, DEFAULT_AGENTS_TOML)?;

        Ok(ConfigInitResult::Created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    #[test]
    fn load_from_file_with_workspace_returns_none_when_missing() {
        let workspace = MemoryWorkspace::new_test();
        let path = Path::new(".agent/agents.toml");

        let result = AgentsConfigFile::load_from_file_with_workspace(path, &workspace).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_from_file_with_workspace_parses_valid_config() {
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/agents.toml", DEFAULT_AGENTS_TOML);
        let path = Path::new(".agent/agents.toml");

        let result = AgentsConfigFile::load_from_file_with_workspace(path, &workspace).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().agents.contains_key("claude"));
    }

    #[test]
    fn ensure_config_exists_with_workspace_creates_file_when_missing() {
        let workspace = MemoryWorkspace::new_test();
        let path = Path::new(".agent/agents.toml");

        let result =
            AgentsConfigFile::ensure_config_exists_with_workspace(path, &workspace).unwrap();
        assert!(matches!(result, ConfigInitResult::Created));
        assert!(workspace.exists(path));
        assert_eq!(workspace.read(path).unwrap(), DEFAULT_AGENTS_TOML);
    }

    #[test]
    fn ensure_config_exists_with_workspace_does_not_overwrite_existing() {
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/agents.toml", "# custom config");
        let path = Path::new(".agent/agents.toml");

        let result =
            AgentsConfigFile::ensure_config_exists_with_workspace(path, &workspace).unwrap();
        assert!(matches!(result, ConfigInitResult::AlreadyExists));
        assert_eq!(workspace.read(path).unwrap(), "# custom config");
    }
}
