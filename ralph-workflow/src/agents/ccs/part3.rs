// CCS Alias Resolver
// This module contains the CcsAliasResolver struct and its implementation.

/// CCS alias resolver that can be used by the agent registry.
#[derive(Debug, Clone, Default)]
pub struct CcsAliasResolver {
    aliases: HashMap<String, CcsAliasConfig>,
    defaults: CcsConfig,
}

impl CcsAliasResolver {
    /// Create a new CCS alias resolver with the given aliases.
    pub const fn new(aliases: HashMap<String, CcsAliasConfig>, defaults: CcsConfig) -> Self {
        Self { aliases, defaults }
    }

    /// Create an empty resolver (no aliases configured).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Try to resolve an agent name as a CCS reference.
    ///
    /// Returns `Some(AgentConfig)` if the name is a valid CCS reference.
    /// For known aliases (or default `ccs`), uses the configured command.
    /// For unknown aliases (e.g., `ccs/random`), generates a default CCS config
    /// to allow direct CCS execution without configuration.
    /// Returns `None` if the name is not a CCS reference (doesn't start with "ccs").
    pub fn try_resolve(&self, agent_name: &str) -> Option<AgentConfig> {
        let alias = parse_ccs_ref(agent_name)?;
        // Try to resolve from configured aliases
        if let Some(config) = resolve_ccs_agent(alias, &self.aliases, &self.defaults) {
            return Some(config);
        }
        // For unknown CCS aliases, generate a default config for direct execution
        // This allows commands like `ccs random` to work without pre-configuration
        let cmd = CcsAliasConfig {
            cmd: format!("ccs {alias}"),
            ..CcsAliasConfig::default()
        };
        let display_name = format!("ccs-{alias}");
        Some(build_ccs_agent_config(
            &cmd,
            &self.defaults,
            display_name,
            alias,
        ))
    }
}
