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
use crate::config::{CcsAliasConfig, CcsConfig};
use std::collections::HashMap;
use std::path::Path;

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
        aliases: HashMap<String, CcsAliasConfig>,
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
        let alternatives = self.get_fuzzy_alternatives(&normalized);

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
    pub(crate) fn get_fuzzy_alternatives(&self, name: &str) -> Vec<String> {
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

    /// Check if agent exists (registered only, not CCS aliases).
    #[cfg(test)]
    pub fn is_known(&self, name: &str) -> bool {
        self.agents.contains_key(name)
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
        let mut loaded = 0usize;

        if !unified.ccs_aliases.is_empty() {
            loaded += unified.ccs_aliases.len();
            let aliases = unified
                .ccs_aliases
                .iter()
                .map(|(name, v)| (name.clone(), v.as_config()))
                .collect::<HashMap<_, _>>();
            self.set_ccs_aliases(aliases, unified.ccs.clone());
        }

        if !unified.agents.is_empty() {
            for (name, overrides) in &unified.agents {
                let Some(existing) = self.agents.get(name).cloned() else {
                    // New agent definition: require a non-empty command.
                    let Some(cmd) = overrides
                        .cmd
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    else {
                        continue;
                    };

                    let json_parser = overrides
                        .json_parser
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("generic");

                    self.register(
                        name,
                        AgentConfig {
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
                        },
                    );
                    loaded += 1;
                    continue;
                };

                let merged = AgentConfig {
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
                    model_flag: if overrides.model_flag.is_some() {
                        overrides.model_flag.clone()
                    } else {
                        existing.model_flag
                    },
                    print_flag: overrides.print_flag.clone().unwrap_or(existing.print_flag),
                    streaming_flag: overrides
                        .streaming_flag
                        .clone()
                        .unwrap_or(existing.streaming_flag),
                    env_vars: existing.env_vars.clone(),
                    // Preserve existing display name unless explicitly overridden
                    // Empty string explicitly clears the display name
                    display_name: match &overrides.display_name {
                        Some(s) if s.is_empty() => None,
                        Some(s) => Some(s.clone()),
                        None => existing.display_name,
                    },
                };

                self.register(name, merged);
                loaded += 1;
            }
        }

        if let Some(chain) = &unified.agent_chain {
            self.fallback = chain.clone();
        }

        loaded
    }

    /// Get the fallback configuration.
    pub const fn fallback_config(&self) -> &FallbackConfig {
        &self.fallback
    }

    /// Set the fallback configuration.
    #[cfg(test)]
    pub fn set_fallback(&mut self, fallback: FallbackConfig) {
        self.fallback = fallback;
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
            let Ok(parts) = crate::utils::split_command(&config.cmd) else {
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
        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));
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
        assert!(registry.is_known("testbot"));
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
        registry.set_fallback(FallbackConfig {
            developer: vec![
                "claude".to_string(),
                "nonexistent".to_string(),
                "codex".to_string(),
            ],
            reviewer: vec![],
            ..Default::default()
        });

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

        // Empty chains should fail
        registry.set_fallback(FallbackConfig::default());
        assert!(registry.validate_agent_chains().is_err());

        // Both chains configured should pass
        registry.set_fallback(FallbackConfig {
            developer: vec!["claude".to_string()],
            reviewer: vec!["codex".to_string()],
            ..Default::default()
        });
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

        registry.set_ccs_aliases(aliases, default_ccs());

        // CCS aliases should be registered as agents
        assert!(registry.is_known("ccs/work"));
        assert!(registry.is_known("ccs/personal"));
        assert!(registry.is_known("ccs/gemini"));

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
        registry.set_ccs_aliases(aliases, default_ccs());

        // Set fallback chain with CCS alias
        registry.set_fallback(FallbackConfig {
            developer: vec!["ccs/work".to_string(), "claude".to_string()],
            reviewer: vec!["claude".to_string()],
            ..Default::default()
        });

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
        registry.set_ccs_aliases(HashMap::new(), default_ccs());

        // Should have built-in agents
        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));

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

        registry2.set_ccs_aliases(aliases, default_ccs());
        assert!(registry2.is_known("ccs/work"));
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
        registry.set_ccs_aliases(aliases, default_ccs());

        let all_agents = registry.list();
        let ccs_agents: Vec<_> = all_agents
            .iter()
            .filter(|(name, _)| name.starts_with("ccs/"))
            .collect();

        assert_eq!(ccs_agents.len(), 2);
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
        let registry = AgentRegistry::new().unwrap();
        // Test underscore to dash/slash replacement
        // Note: These test the pattern, actual agents may not exist
        let result = registry.get_fuzzy_alternatives("my_agent");
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
}
