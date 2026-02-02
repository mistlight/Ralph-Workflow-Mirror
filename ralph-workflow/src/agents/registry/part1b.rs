// AgentRegistry implementation - Part 1b: Configuration loading and validation.
// Contains: apply_unified_config, apply_ccs_aliases, apply_agent_overrides,
// create_new_agent_config, merge_agent_config, fallback_config, retry_timer,
// set_retry_timer, available_fallbacks, validate_agent_chains, is_agent_available,
// list_available.

impl AgentRegistry {
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
            session_flag: overrides.session_flag.clone().unwrap_or_else(|| {
                // Default session continuation flags for known agents
                // These flags are verified from CLI --help output:
                // - Claude: --resume <session_id> (from `claude --help`)
                // - OpenCode: -s <session_id> (from `opencode run --help`)
                // - Codex: Uses `codex exec resume <id>` subcommand, not a flag - not supported
                if cmd.starts_with("claude") || cmd.starts_with("ccs") {
                    "--resume {}".to_string()
                } else if cmd.starts_with("opencode") {
                    "-s {}".to_string()
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
            session_flag: overrides
                .session_flag
                .clone()
                .unwrap_or(existing.session_flag),
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
    #[cfg(any(test, feature = "test-utils"))]
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
