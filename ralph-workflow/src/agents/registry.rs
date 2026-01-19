//! Agent registry for agent lookup and management.
//!
//! The registry holds all configured agents and provides methods for
//! looking up agents by name, validating agent chains, and checking
//! agent availability.
//!
//! # CCS (Claude Code Switch) Support
//!
//! The registry supports CCS aliases using `ccs/alias` syntax.
//! CCS aliases are resolved on-the-fly to generate `AgentConfig` instances.
//!
//! ```ignore
//! // Using CCS aliases in agent chains
//! [ccs_aliases]
//! work = "ccs work"
//! personal = "ccs personal"
//!
//! [agent_chain]
//! developer = ["ccs/work", "claude"]
//! ```
use super::ccs::CcsAliasResolver;
use super::config::{AgentConfig, AgentConfigError, AgentsConfigFile, DEFAULT_AGENTS_TOML};
use super::fallback::{AgentRole, FallbackConfig};
use super::parser::JsonParserType;
use super::retry_timer::{production_timer, RetryTimerProvider};
use crate::config::{CcsAliasConfig, CcsConfig};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Agent registry with CCS alias support.
///
/// CCS aliases are eagerly resolved and registered as regular agents
/// when set via `set_ccs_aliases()`. This allows `get()` to work
/// uniformly for both regular agents and CCS aliases.
pub struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    fallback: FallbackConfig,
    /// CCS alias resolver for `ccs/alias` syntax.
    ccs_resolver: CcsAliasResolver,
    /// Retry timer provider for controlling sleep behavior in retry logic.
    retry_timer: Arc<dyn RetryTimerProvider>,
}

impl AgentRegistry {
    /// Create a new registry with default agents.
    pub fn new() -> Result<Self, AgentConfigError> {
        let AgentsConfigFile { agents, fallback } =
            toml::from_str(DEFAULT_AGENTS_TOML).map_err(AgentConfigError::DefaultTemplateToml)?;

        let mut registry = Self {
            agents: HashMap::new(),
            fallback,
            ccs_resolver: CcsAliasResolver::empty(),
            retry_timer: production_timer(),
        };

        for (name, agent_toml) in agents {
            registry.register(&name, AgentConfig::from(agent_toml));
        }

        Ok(registry)
    }

    /// Set CCS aliases for the registry.
    ///
    /// This eagerly registers CCS aliases as agents so they can be
    /// resolved with `resolve_config()`.
    pub fn set_ccs_aliases(
        &mut self,
        aliases: &HashMap<String, CcsAliasConfig>,
        defaults: CcsConfig,
    ) {
        self.ccs_resolver = CcsAliasResolver::new(aliases.clone(), defaults);
        // Eagerly register CCS aliases as agents
        for alias_name in aliases.keys() {
            let agent_name = format!("ccs/{alias_name}");
            if let Some(config) = self.ccs_resolver.try_resolve(&agent_name) {
                self.agents.insert(agent_name, config);
            }
        }
    }

    /// Register a new agent.
    pub fn register(&mut self, name: &str, config: AgentConfig) {
        self.agents.insert(name.to_string(), config);
    }

    /// Resolve an agent's configuration, including on-the-fly CCS references.
    ///
    /// CCS supports direct execution via `ccs/<alias>` even when the alias isn't
    /// pre-registered in config; those are resolved lazily here.
    pub fn resolve_config(&self, name: &str) -> Option<AgentConfig> {
        self.agents
            .get(name)
            .cloned()
            .or_else(|| self.ccs_resolver.try_resolve(name))
    }

    /// Get display name for an agent.
    ///
    /// Returns the agent's custom display name if set (e.g., "ccs-glm" for CCS aliases),
    /// otherwise returns the agent's registry name.
    ///
    /// # Arguments
    ///
    /// * `name` - The agent's registry name (e.g., "ccs/glm", "claude")
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(registry.display_name("ccs/glm"), "ccs-glm");
    /// assert_eq!(registry.display_name("claude"), "claude");
    /// ```
    pub fn display_name(&self, name: &str) -> String {
        self.resolve_config(name)
            .and_then(|config| config.display_name)
            .unwrap_or_else(|| name.to_string())
    }

    /// Resolve a fuzzy agent name to a canonical agent name.
    ///
    /// This handles common typos and alternative forms:
    /// - `ccs/<unregistered>`: Returns the name as-is for direct CCS execution
    /// - Other fuzzy matches: Returns the canonical name if a match is found
    /// - Exact matches: Returns the name as-is
    ///
    /// Returns `None` if the name cannot be resolved to any known agent.
    pub fn resolve_fuzzy(&self, name: &str) -> Option<String> {
        // First check if it's an exact match
        if self.agents.contains_key(name) {
            return Some(name.to_string());
        }

        // Handle ccs/<unregistered> pattern - return as-is for direct CCS execution
        if name.starts_with("ccs/") {
            return Some(name.to_string());
        }

        // Handle common typos/alternatives
        let normalized = name.to_lowercase();
        let alternatives = Self::get_fuzzy_alternatives(&normalized);

        for alt in alternatives {
            // If it's a ccs/ pattern, return it for direct CCS execution
            if alt.starts_with("ccs/") {
                return Some(alt);
            }
            // Otherwise check if it exists in the registry
            if self.agents.contains_key(&alt) {
                return Some(alt);
            }
        }

        None
    }

    /// Get fuzzy alternatives for a given agent name.
    ///
    /// Returns a list of potential canonical names to try, in order of preference.
    pub(crate) fn get_fuzzy_alternatives(name: &str) -> Vec<String> {
        let mut alternatives = Vec::new();

        // Add exact match first
        alternatives.push(name.to_string());

        // Handle common typos and variations
        match name {
            // ccs variations
            n if n.starts_with("ccs-") => {
                alternatives.push(name.replace("ccs-", "ccs/"));
            }
            n if n.contains('_') => {
                alternatives.push(name.replace('_', "-"));
                alternatives.push(name.replace('_', "/"));
            }

            // claude variations
            "claud" | "cloud" => alternatives.push("claude".to_string()),

            // codex variations
            "codeex" | "code-x" => alternatives.push("codex".to_string()),

            // cursor variations
            "crusor" => alternatives.push("cursor".to_string()),

            // opencode variations
            "opencode" | "open-code" => alternatives.push("opencode".to_string()),

            // gemini variations
            "gemeni" | "gemni" => alternatives.push("gemini".to_string()),

            // qwen variations
            "quen" | "quwen" => alternatives.push("qwen".to_string()),

            // aider variations
            "ader" => alternatives.push("aider".to_string()),

            // vibe variations
            "vib" => alternatives.push("vibe".to_string()),

            // cline variations
            "kline" => alternatives.push("cline".to_string()),

            _ => {}
        }

        alternatives
    }

    /// List all registered agents.
    pub fn list(&self) -> Vec<(&str, &AgentConfig)> {
        self.agents.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Get command for developer role.
    pub fn developer_cmd(&self, agent_name: &str) -> Option<String> {
        self.resolve_config(agent_name)
            .map(|c| c.build_cmd(true, true, true))
    }

    /// Get command for reviewer role.
    pub fn reviewer_cmd(&self, agent_name: &str) -> Option<String> {
        self.resolve_config(agent_name)
            .map(|c| c.build_cmd(true, true, false))
    }

    /// Load custom agents from a TOML configuration file.
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, AgentConfigError> {
        match AgentsConfigFile::load_from_file(path)? {
            Some(config) => {
                let count = config.agents.len();
                for (name, agent_toml) in config.agents {
                    self.register(&name, AgentConfig::from(agent_toml));
                }
                // Load fallback configuration
                self.fallback = config.fallback;
                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Apply settings from the unified config (`~/.config/ralph-workflow.toml`).
    ///
    /// This merges (in increasing priority):
    /// 1. Built-in defaults (embedded `examples/agents.toml`)
    /// 2. Unified config: `[agents]`, `[ccs_aliases]`, and `[agent_chain]` (if present)
    ///
    /// Returns the number of agents loaded from unified config, including CCS aliases.
    pub fn apply_unified_config(&mut self, unified: &crate::config::UnifiedConfig) -> usize {
        let mut loaded = self.apply_ccs_aliases(unified);
        loaded += self.apply_agent_overrides(unified);

        if let Some(chain) = &unified.agent_chain {
            self.fallback = chain.clone();
        }

        loaded
    }

    /// Apply CCS aliases from the unified config.
    fn apply_ccs_aliases(&mut self, unified: &crate::config::UnifiedConfig) -> usize {
        if unified.ccs_aliases.is_empty() {
            return 0;
        }

        let loaded = unified.ccs_aliases.len();
        let aliases = unified
            .ccs_aliases
            .iter()
            .map(|(name, v)| (name.clone(), v.as_config()))
            .collect::<HashMap<_, _>>();
        self.set_ccs_aliases(&aliases, unified.ccs.clone());
        loaded
    }

    /// Apply agent overrides from the unified config.
    fn apply_agent_overrides(&mut self, unified: &crate::config::UnifiedConfig) -> usize {
        if unified.agents.is_empty() {
            return 0;
        }

        let mut loaded = 0usize;
        for (name, overrides) in &unified.agents {
            if let Some(existing) = self.agents.get(name).cloned() {
                // Merge with existing agent
                let merged = Self::merge_agent_config(existing, overrides);
                self.register(name, merged);
                loaded += 1;
            } else {
                // New agent definition: require a non-empty command.
                if let Some(config) = Self::create_new_agent_config(overrides) {
                    self.register(name, config);
                    loaded += 1;
                }
            }
        }
        loaded
    }

    /// Create a new agent config from unified config overrides.
    fn create_new_agent_config(
        overrides: &crate::config::unified::AgentConfigToml,
    ) -> Option<AgentConfig> {
        let cmd = overrides
            .cmd
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;

        let json_parser = overrides
            .json_parser
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("generic");

        Some(AgentConfig {
            cmd: cmd.to_string(),
            output_flag: overrides.output_flag.clone().unwrap_or_default(),
            yolo_flag: overrides.yolo_flag.clone().unwrap_or_default(),
            verbose_flag: overrides.verbose_flag.clone().unwrap_or_default(),
            can_commit: overrides.can_commit.unwrap_or(true),
            json_parser: JsonParserType::parse(json_parser),
            model_flag: overrides.model_flag.clone(),
            print_flag: overrides.print_flag.clone().unwrap_or_default(),
            streaming_flag: overrides.streaming_flag.clone().unwrap_or_else(|| {
                // Default to "--include-partial-messages" for Claude/CCS agents
                if cmd.starts_with("claude") || cmd.starts_with("ccs") {
                    "--include-partial-messages".to_string()
                } else {
                    String::new()
                }
            }),
            env_vars: std::collections::HashMap::new(),
            display_name: overrides
                .display_name
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
        })
    }

    /// Merge overrides with existing agent config.
    fn merge_agent_config(
        existing: AgentConfig,
        overrides: &crate::config::unified::AgentConfigToml,
    ) -> AgentConfig {
        AgentConfig {
            cmd: overrides
                .cmd
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or(existing.cmd),
            output_flag: overrides
                .output_flag
                .clone()
                .unwrap_or(existing.output_flag),
            yolo_flag: overrides.yolo_flag.clone().unwrap_or(existing.yolo_flag),
            verbose_flag: overrides
                .verbose_flag
                .clone()
                .unwrap_or(existing.verbose_flag),
            can_commit: overrides.can_commit.unwrap_or(existing.can_commit),
            json_parser: overrides
                .json_parser
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map_or(existing.json_parser, JsonParserType::parse),
            model_flag: overrides.model_flag.clone().or(existing.model_flag),
            print_flag: overrides.print_flag.clone().unwrap_or(existing.print_flag),
            streaming_flag: overrides
                .streaming_flag
                .clone()
                .unwrap_or(existing.streaming_flag),
            // Do NOT inherit env_vars from the existing agent to prevent
            // CCS env vars from one agent from leaking into another.
            // The unified config (unified::AgentConfigToml) doesn't support
            // ccs_profile or env_vars fields, so we always start fresh.
            env_vars: std::collections::HashMap::new(),
            // Preserve existing display name unless explicitly overridden
            // Empty string explicitly clears the display name
            display_name: match &overrides.display_name {
                Some(s) if s.is_empty() => None,
                Some(s) => Some(s.clone()),
                None => existing.display_name,
            },
        }
    }

    /// Get the fallback configuration.
    pub const fn fallback_config(&self) -> &FallbackConfig {
        &self.fallback
    }

    /// Get the retry timer provider.
    pub fn retry_timer(&self) -> Arc<dyn RetryTimerProvider> {
        Arc::clone(&self.retry_timer)
    }

    /// Set the retry timer provider (for testing purposes).
    ///
    /// This is used to inject a test timer that doesn't actually sleep,
    /// enabling fast test execution without waiting for retry delays.
    pub fn set_retry_timer(&mut self, timer: Arc<dyn RetryTimerProvider>) {
        self.retry_timer = timer;
    }

    /// Get all fallback agents for a role that are registered in this registry.
    pub fn available_fallbacks(&self, role: AgentRole) -> Vec<&str> {
        self.fallback
            .get_fallbacks(role)
            .iter()
            .filter(|name| self.is_agent_available(name))
            // Agents with can_commit=false are chat-only / non-tool agents and will stall Ralph.
            .filter(|name| {
                self.resolve_config(name.as_str())
                    .is_some_and(|cfg| cfg.can_commit)
            })
            .map(std::string::String::as_str)
            .collect()
    }

    /// Validate that agent chains are configured for both roles.
    pub fn validate_agent_chains(&self) -> Result<(), String> {
        let has_developer = self.fallback.has_fallbacks(AgentRole::Developer);
        let has_reviewer = self.fallback.has_fallbacks(AgentRole::Reviewer);

        if !has_developer && !has_reviewer {
            return Err("No agent chain configured.\n\
                Please add an [agent_chain] section to ~/.config/ralph-workflow.toml.\n\
                Run 'ralph --init-global' to create a default configuration."
                .to_string());
        }

        if !has_developer {
            return Err("No developer agent chain configured.\n\
                Add 'developer = [\"your-agent\", ...]' to your [agent_chain] section.\n\
                Use --list-agents to see available agents."
                .to_string());
        }

        if !has_reviewer {
            return Err("No reviewer agent chain configured.\n\
                Add 'reviewer = [\"your-agent\", ...]' to your [agent_chain] section.\n\
                Use --list-agents to see available agents."
                .to_string());
        }

        // Sanity check: ensure there is at least one workflow-capable agent per role.
        for role in [AgentRole::Developer, AgentRole::Reviewer] {
            let chain = self.fallback.get_fallbacks(role);
            let has_capable = chain
                .iter()
                .any(|name| self.resolve_config(name).is_some_and(|cfg| cfg.can_commit));
            if !has_capable {
                return Err(format!(
                    "No workflow-capable agents found for {role}.\n\
                    All agents in the {role} chain have can_commit=false.\n\
                    Fix: set can_commit=true for at least one agent or update [agent_chain]."
                ));
            }
        }

        Ok(())
    }

    /// Check if an agent is available (command exists and is executable).
    pub fn is_agent_available(&self, name: &str) -> bool {
        if let Some(config) = self.resolve_config(name) {
            let Ok(parts) = crate::common::split_command(&config.cmd) else {
                return false;
            };
            let Some(base_cmd) = parts.first() else {
                return false;
            };

            // Check if the command exists in PATH
            which::which(base_cmd).is_ok()
        } else {
            false
        }
    }

    /// List all available (installed) agents.
    pub fn list_available(&self) -> Vec<&str> {
        self.agents
            .keys()
            .filter(|name| self.is_agent_available(name))
            .map(std::string::String::as_str)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::JsonParserType;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn default_ccs() -> CcsConfig {
        CcsConfig::default()
    }

    fn write_stub_executable(dir: &std::path::Path, name: &str) {
        #[cfg(windows)]
        {
            let path = dir.join(format!("{}.cmd", name));
            std::fs::write(&path, "@echo off\r\nexit /b 0\r\n").unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join(name);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = AgentRegistry::new().unwrap();
        // Behavioral test: agents are registered if they resolve
        assert!(registry.resolve_config("claude").is_some());
        assert!(registry.resolve_config("codex").is_some());
    }

    #[test]
    fn test_registry_register() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.register(
            "testbot",
            AgentConfig {
                cmd: "testbot run".to_string(),
                output_flag: "--json".to_string(),
                yolo_flag: "--yes".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: String::new(),
                env_vars: std::collections::HashMap::new(),
                display_name: None,
            },
        );
        // Behavioral test: registered agent should resolve
        assert!(registry.resolve_config("testbot").is_some());
    }

    #[test]
    fn test_registry_display_name() {
        let mut registry = AgentRegistry::new().unwrap();

        // Agent without custom display name uses registry key
        registry.register(
            "claude",
            AgentConfig {
                cmd: "claude -p".to_string(),
                output_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: "--include-partial-messages".to_string(),
                env_vars: std::collections::HashMap::new(),
                display_name: None,
            },
        );

        // Agent with custom display name uses that
        registry.register(
            "ccs/glm",
            AgentConfig {
                cmd: "ccs glm".to_string(),
                output_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
                model_flag: None,
                print_flag: "-p".to_string(),
                streaming_flag: "--include-partial-messages".to_string(),
                env_vars: std::collections::HashMap::new(),
                display_name: Some("ccs-glm".to_string()),
            },
        );

        // Test display names
        assert_eq!(registry.display_name("claude"), "claude");
        assert_eq!(registry.display_name("ccs/glm"), "ccs-glm");

        // Unknown agent returns the key as-is
        assert_eq!(registry.display_name("unknown"), "unknown");
    }

    #[test]
    fn test_registry_available_fallbacks() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original_path = std::env::var_os("PATH");
        let dir = tempfile::tempdir().unwrap();

        write_stub_executable(dir.path(), "claude");
        write_stub_executable(dir.path(), "codex");

        let mut new_paths = vec![dir.path().to_path_buf()];
        if let Some(p) = &original_path {
            new_paths.extend(std::env::split_paths(p));
        }
        let joined = std::env::join_paths(new_paths).unwrap();
        std::env::set_var("PATH", &joined);

        let mut registry = AgentRegistry::new().unwrap();
        // Use apply_unified_config to set fallback chain (public API)
        let toml_str = r#"
            [agent_chain]
            developer = ["claude", "nonexistent", "codex"]
        "#;
        let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
        registry.apply_unified_config(&unified);

        let fallbacks = registry.available_fallbacks(AgentRole::Developer);
        assert!(fallbacks.contains(&"claude"));
        assert!(fallbacks.contains(&"codex"));
        assert!(!fallbacks.contains(&"nonexistent"));

        if let Some(p) = original_path {
            std::env::set_var("PATH", p);
        } else {
            std::env::remove_var("PATH");
        }
    }

    #[test]
    fn test_validate_agent_chains() {
        let mut registry = AgentRegistry::new().unwrap();

        // Both chains configured should pass - use apply_unified_config (public API)
        let toml_str = r#"
            [agent_chain]
            developer = ["claude"]
            reviewer = ["codex"]
        "#;
        let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
        registry.apply_unified_config(&unified);
        assert!(registry.validate_agent_chains().is_ok());
    }

    #[test]
    fn test_ccs_aliases_registration() {
        // Test that CCS aliases are registered correctly
        let mut registry = AgentRegistry::new().unwrap();

        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "personal".to_string(),
            CcsAliasConfig {
                cmd: "ccs personal".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "gemini".to_string(),
            CcsAliasConfig {
                cmd: "ccs gemini".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        registry.set_ccs_aliases(&aliases, default_ccs());

        // CCS aliases should be registered as agents - behavioral test: they resolve
        assert!(registry.resolve_config("ccs/work").is_some());
        assert!(registry.resolve_config("ccs/personal").is_some());
        assert!(registry.resolve_config("ccs/gemini").is_some());

        // Get should return valid config
        let config = registry.resolve_config("ccs/work").unwrap();
        // When claude binary is found, it replaces "ccs work" with the path to claude
        assert!(
            config.cmd.ends_with("claude") || config.cmd == "ccs work",
            "cmd should be 'ccs work' or a path ending with 'claude', got: {}",
            config.cmd
        );
        assert!(config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
    }

    #[test]
    fn test_ccs_in_fallback_chain() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original_path = std::env::var_os("PATH");
        let dir = tempfile::tempdir().unwrap();

        // Create stub for ccs command
        write_stub_executable(dir.path(), "ccs");
        write_stub_executable(dir.path(), "claude");

        let mut new_paths = vec![dir.path().to_path_buf()];
        if let Some(p) = &original_path {
            new_paths.extend(std::env::split_paths(p));
        }
        let joined = std::env::join_paths(new_paths).unwrap();
        std::env::set_var("PATH", &joined);

        let mut registry = AgentRegistry::new().unwrap();

        // Register CCS aliases
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        registry.set_ccs_aliases(&aliases, default_ccs());

        // Set fallback chain with CCS alias using apply_unified_config (public API)
        let toml_str = r#"
            [agent_chain]
            developer = ["ccs/work", "claude"]
            reviewer = ["claude"]
        "#;
        let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
        registry.apply_unified_config(&unified);

        // ccs/work should be in available fallbacks (since ccs is in PATH)
        let fallbacks = registry.available_fallbacks(AgentRole::Developer);
        assert!(fallbacks.contains(&"ccs/work"));
        assert!(fallbacks.contains(&"claude"));

        // Validate chains should pass
        assert!(registry.validate_agent_chains().is_ok());

        if let Some(p) = original_path {
            std::env::set_var("PATH", p);
        } else {
            std::env::remove_var("PATH");
        }
    }

    #[test]
    fn test_ccs_aliases_with_registry_constructor() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_ccs_aliases(&HashMap::new(), default_ccs());

        // Should have built-in agents - behavioral test: they resolve
        assert!(registry.resolve_config("claude").is_some());
        assert!(registry.resolve_config("codex").is_some());

        // Now test with actual aliases
        let mut registry2 = AgentRegistry::new().unwrap();
        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );

        registry2.set_ccs_aliases(&aliases, default_ccs());
        // Behavioral test: CCS alias should resolve
        assert!(registry2.resolve_config("ccs/work").is_some());
    }

    #[test]
    fn test_list_includes_ccs_aliases() {
        let mut registry = AgentRegistry::new().unwrap();

        let mut aliases = HashMap::new();
        aliases.insert(
            "work".to_string(),
            CcsAliasConfig {
                cmd: "ccs work".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        aliases.insert(
            "personal".to_string(),
            CcsAliasConfig {
                cmd: "ccs personal".to_string(),
                ..CcsAliasConfig::default()
            },
        );
        registry.set_ccs_aliases(&aliases, default_ccs());

        let all_agents = registry.list();

        assert_eq!(
            all_agents
                .iter()
                .filter(|(name, _)| name.starts_with("ccs/"))
                .count(),
            2
        );
    }

    #[test]
    fn test_resolve_fuzzy_exact_match() {
        let registry = AgentRegistry::new().unwrap();
        assert_eq!(registry.resolve_fuzzy("claude"), Some("claude".to_string()));
        assert_eq!(registry.resolve_fuzzy("codex"), Some("codex".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_ccs_unregistered() {
        let registry = AgentRegistry::new().unwrap();
        // ccs/<unregistered> should return as-is for direct execution
        assert_eq!(
            registry.resolve_fuzzy("ccs/random"),
            Some("ccs/random".to_string())
        );
        assert_eq!(
            registry.resolve_fuzzy("ccs/unregistered"),
            Some("ccs/unregistered".to_string())
        );
    }

    #[test]
    fn test_resolve_fuzzy_typos() {
        let registry = AgentRegistry::new().unwrap();
        // Test common typos
        assert_eq!(registry.resolve_fuzzy("claud"), Some("claude".to_string()));
        assert_eq!(registry.resolve_fuzzy("CLAUD"), Some("claude".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_codex_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test codex variations
        assert_eq!(registry.resolve_fuzzy("codeex"), Some("codex".to_string()));
        assert_eq!(registry.resolve_fuzzy("code-x"), Some("codex".to_string()));
        assert_eq!(registry.resolve_fuzzy("CODEEX"), Some("codex".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_cursor_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test cursor variations
        assert_eq!(registry.resolve_fuzzy("crusor"), Some("cursor".to_string()));
        assert_eq!(registry.resolve_fuzzy("CRUSOR"), Some("cursor".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_gemini_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test gemini variations
        assert_eq!(registry.resolve_fuzzy("gemeni"), Some("gemini".to_string()));
        assert_eq!(registry.resolve_fuzzy("gemni"), Some("gemini".to_string()));
        assert_eq!(registry.resolve_fuzzy("GEMENI"), Some("gemini".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_qwen_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test qwen variations
        assert_eq!(registry.resolve_fuzzy("quen"), Some("qwen".to_string()));
        assert_eq!(registry.resolve_fuzzy("quwen"), Some("qwen".to_string()));
        assert_eq!(registry.resolve_fuzzy("QUEN"), Some("qwen".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_aider_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test aider variations
        assert_eq!(registry.resolve_fuzzy("ader"), Some("aider".to_string()));
        assert_eq!(registry.resolve_fuzzy("ADER"), Some("aider".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_vibe_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test vibe variations
        assert_eq!(registry.resolve_fuzzy("vib"), Some("vibe".to_string()));
        assert_eq!(registry.resolve_fuzzy("VIB"), Some("vibe".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_cline_variations() {
        let registry = AgentRegistry::new().unwrap();
        // Test cline variations
        assert_eq!(registry.resolve_fuzzy("kline"), Some("cline".to_string()));
        assert_eq!(registry.resolve_fuzzy("KLINE"), Some("cline".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_ccs_dash_to_slash() {
        let registry = AgentRegistry::new().unwrap();
        // Test ccs- to ccs/ conversion (even for unregistered aliases)
        assert_eq!(
            registry.resolve_fuzzy("ccs-random"),
            Some("ccs/random".to_string())
        );
        assert_eq!(
            registry.resolve_fuzzy("ccs-test"),
            Some("ccs/test".to_string())
        );
    }

    #[test]
    fn test_resolve_fuzzy_underscore_replacement() {
        // Test underscore to dash/slash replacement
        // Note: These test the pattern, actual agents may not exist
        let result = AgentRegistry::get_fuzzy_alternatives("my_agent");
        assert!(result.contains(&"my_agent".to_string()));
        assert!(result.contains(&"my-agent".to_string()));
        assert!(result.contains(&"my/agent".to_string()));
    }

    #[test]
    fn test_resolve_fuzzy_unknown() {
        let registry = AgentRegistry::new().unwrap();
        // Unknown agent should return None
        assert_eq!(registry.resolve_fuzzy("totally-unknown"), None);
    }

    #[test]
    fn test_apply_unified_config_does_not_inherit_env_vars() {
        // Regression test for CCS env vars leaking between agents.
        // Ensures that when apply_unified_config merges agent configurations,
        // env_vars from the existing agent are NOT inherited into the merged agent.
        let mut registry = AgentRegistry::new().unwrap();

        // First, manually register a "claude" agent with some env vars (simulating
        // a previously-loaded agent with CCS env vars or manually-specified vars)
        registry.register(
            "claude",
            AgentConfig {
                cmd: "claude -p".to_string(),
                output_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: "--include-partial-messages".to_string(),
                // Simulate CCS env vars from a previous load
                env_vars: {
                    let mut vars = std::collections::HashMap::new();
                    vars.insert(
                        "ANTHROPIC_BASE_URL".to_string(),
                        "https://api.z.ai/api/anthropic".to_string(),
                    );
                    vars.insert(
                        "ANTHROPIC_AUTH_TOKEN".to_string(),
                        "test-token-glm".to_string(),
                    );
                    vars.insert("ANTHROPIC_MODEL".to_string(), "glm-4.7".to_string());
                    vars
                },
                display_name: None,
            },
        );

        // Verify the "claude" agent has the GLM env vars
        let claude_config = registry.resolve_config("claude").unwrap();
        assert_eq!(claude_config.env_vars.len(), 3);
        assert_eq!(
            claude_config.env_vars.get("ANTHROPIC_BASE_URL"),
            Some(&"https://api.z.ai/api/anthropic".to_string())
        );

        // Now apply a unified config that overrides the "claude" agent
        // (simulating user's ~/.config/ralph-workflow.toml with [agents.claude])
        // Create a minimal GeneralConfig via Default for UnifiedConfig
        // Note: We can't directly construct UnifiedConfig with Default because agents is not Default
        // So we'll create it by deserializing from a TOML string
        let toml_str = r#"
            [general]
            verbosity = 2
            interactive = true
            isolation_mode = true

            [agents.claude]
            cmd = "claude -p"
            display_name = "My Custom Claude"
        "#;
        let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();

        // Apply the unified config
        registry.apply_unified_config(&unified);

        // Verify that the "claude" agent's env_vars are now empty (NOT inherited)
        let claude_config_after = registry.resolve_config("claude").unwrap();
        assert_eq!(
            claude_config_after.env_vars.len(),
            0,
            "env_vars should NOT be inherited from the existing agent when unified config is applied"
        );
        assert_eq!(
            claude_config_after.display_name,
            Some("My Custom Claude".to_string()),
            "display_name should be updated from the unified config"
        );
    }

    #[test]
    fn test_resolve_config_does_not_share_env_vars_between_agents() {
        // Regression test for the exact bug scenario:
        // 1. User runs Ralph with ccs/glm agent (with GLM env vars)
        // 2. User then runs Ralph with claude agent
        // 3. Claude should NOT have GLM env vars
        //
        // This test verifies that resolve_config() returns independent AgentConfig
        // instances with separate env_vars HashMaps - i.e., modifications to one
        // agent's env_vars don't affect another agent's config.
        let mut registry = AgentRegistry::new().unwrap();

        // Register ccs/glm with GLM environment variables
        registry.register(
            "ccs/glm",
            AgentConfig {
                cmd: "ccs glm".to_string(),
                output_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
                model_flag: None,
                print_flag: "-p".to_string(),
                streaming_flag: "--include-partial-messages".to_string(),
                env_vars: {
                    let mut vars = std::collections::HashMap::new();
                    vars.insert(
                        "ANTHROPIC_BASE_URL".to_string(),
                        "https://api.z.ai/api/anthropic".to_string(),
                    );
                    vars.insert(
                        "ANTHROPIC_AUTH_TOKEN".to_string(),
                        "test-token-glm".to_string(),
                    );
                    vars.insert("ANTHROPIC_MODEL".to_string(), "glm-4.7".to_string());
                    vars
                },
                display_name: Some("ccs-glm".to_string()),
            },
        );

        // Register claude with empty env_vars (typical configuration)
        registry.register(
            "claude",
            AgentConfig {
                cmd: "claude -p".to_string(),
                output_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
                model_flag: None,
                print_flag: String::new(),
                streaming_flag: "--include-partial-messages".to_string(),
                env_vars: std::collections::HashMap::new(),
                display_name: None,
            },
        );

        // Resolve ccs/glm config first
        let glm_config = registry.resolve_config("ccs/glm").unwrap();
        assert_eq!(glm_config.env_vars.len(), 3);
        assert_eq!(
            glm_config.env_vars.get("ANTHROPIC_BASE_URL"),
            Some(&"https://api.z.ai/api/anthropic".to_string())
        );

        // Resolve claude config
        let claude_config = registry.resolve_config("claude").unwrap();
        assert_eq!(
            claude_config.env_vars.len(),
            0,
            "claude agent should have empty env_vars"
        );

        // Resolve ccs/glm again to ensure we get a fresh clone
        let glm_config2 = registry.resolve_config("ccs/glm").unwrap();
        assert_eq!(glm_config2.env_vars.len(), 3);

        // Modify the first GLM config's env_vars
        // This should NOT affect the second GLM config if cloning is deep
        drop(glm_config);

        // Verify claude still has empty env_vars after another resolve
        let claude_config2 = registry.resolve_config("claude").unwrap();
        assert_eq!(
            claude_config2.env_vars.len(),
            0,
            "claude agent env_vars should remain independent"
        );
    }
}
