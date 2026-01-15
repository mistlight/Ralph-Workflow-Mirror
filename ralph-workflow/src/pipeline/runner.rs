//! Command execution helpers and fallback orchestration.

use crate::agents::{validate_model_flag, AgentRegistry, AgentRole, JsonParserType};
use crate::common::split_command;

use super::fallback::try_agent_with_retries;
use super::fallback::TryAgentResult;
use super::model_flag::resolve_model_with_provider;
use super::prompt::PipelineRuntime;

/// Run a command with automatic fallback to alternative agents on failure.
pub fn run_with_fallback(
    role: AgentRole,
    base_label: &str,
    prompt: &str,
    logfile_prefix: &str,
    runtime: &mut PipelineRuntime<'_>,
    registry: &AgentRegistry,
    primary_agent: &str,
) -> std::io::Result<i32> {
    let fallback_config = registry.fallback_config();
    let fallbacks = registry.available_fallbacks(role);
    if !fallback_config.has_fallbacks(role) {
        runtime.logger.info(&format!(
            "No configured fallbacks for {role}, using primary only"
        ));
    }

    // Build the list of agents to try
    let mut agents_to_try: Vec<&str> = vec![primary_agent];
    for fb in &fallbacks {
        if *fb != primary_agent && !agents_to_try.contains(fb) {
            agents_to_try.push(fb);
        }
    }

    // Get the CLI model and provider overrides based on role (if any)
    let (cli_model_override, cli_provider_override) = match role {
        AgentRole::Developer => (
            runtime.config.developer_model.as_deref(),
            runtime.config.developer_provider.as_deref(),
        ),
        AgentRole::Reviewer => (
            runtime.config.reviewer_model.as_deref(),
            runtime.config.reviewer_provider.as_deref(),
        ),
        AgentRole::Commit => (None, None), // Commit role doesn't have CLI overrides
    };

    // Cycle through all agents with exponential backoff
    for cycle in 0..fallback_config.max_cycles {
        if cycle > 0 {
            let backoff_ms = fallback_config.calculate_backoff(cycle - 1);
            runtime.logger.info(&format!(
                "Cycle {}/{}: All agents exhausted, waiting {}ms before retry (exponential backoff)...",
                cycle + 1,
                fallback_config.max_cycles,
                backoff_ms
            ));
            std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
        }

        for (agent_index, agent_name) in agents_to_try.iter().enumerate() {
            let Some(agent_config) = registry.resolve_config(agent_name) else {
                runtime.logger.warn(&format!(
                    "Agent '{agent_name}' not found in registry, skipping"
                ));
                continue;
            };

            // Get display name for this agent (used throughout user-facing output)
            let display_name = registry.display_name(agent_name);

            // Build the list of model flags to try for this agent:
            // 1. CLI model/provider override (if provided and this is the primary agent)
            // 2. Agent's configured model_flag (from agents.toml)
            // 3. Provider fallback models (from agent_chain.provider_fallback)
            let mut model_flags_to_try: Vec<Option<String>> = Vec::new();

            // CLI override takes highest priority for primary agent
            // Provider override can modify the model's provider prefix
            if agent_index == 0 && (cli_model_override.is_some() || cli_provider_override.is_some())
            {
                let resolved = resolve_model_with_provider(
                    cli_provider_override,
                    cli_model_override,
                    agent_config.model_flag.as_deref(),
                );
                if resolved.is_some() {
                    model_flags_to_try.push(resolved);
                }
            }

            // Add the agent's default model (None means use agent's configured model_flag or no model)
            if model_flags_to_try.is_empty() {
                model_flags_to_try.push(None);
            }

            // Add provider fallback models for this agent
            if fallback_config.has_provider_fallbacks(agent_name) {
                let provider_fallbacks = fallback_config.get_provider_fallbacks(agent_name);
                runtime.logger.info(&format!(
                    "Agent '{}' has {} provider fallback(s) configured",
                    display_name,
                    provider_fallbacks.len()
                ));
                for model in provider_fallbacks {
                    model_flags_to_try.push(Some(model.clone()));
                }
            }

            // Validate model flags and emit warnings (only on first try to avoid spam)
            if agent_index == 0 && cycle == 0 {
                for model_flag in model_flags_to_try.iter().flatten() {
                    for warning in validate_model_flag(model_flag) {
                        runtime.logger.warn(&warning);
                    }
                }
            }

            // Try each model flag
            for (model_index, model_flag) in model_flags_to_try.iter().enumerate() {
                let mut parser_type = agent_config.json_parser;

                // Apply parser override for reviewer if configured
                // CLI/env var override takes precedence over agent config
                if role == AgentRole::Reviewer {
                    if let Some(ref parser_override) = runtime.config.reviewer_json_parser {
                        parser_type = JsonParserType::parse(parser_override);
                        // Only log on first try to avoid spam
                        if agent_index == 0 && cycle == 0 && model_index == 0 {
                            runtime.logger.info(&format!(
                                "Using JSON parser override '{parser_override}' for reviewer"
                            ));
                        }
                    }
                }

                // Build command with model override
                let model_ref = model_flag.as_deref();
                let cmd_str = if agent_index == 0 && cycle == 0 && model_index == 0 {
                    // For primary agent on first cycle, respect env var command overrides
                    match role {
                        AgentRole::Developer => {
                            runtime.config.developer_cmd.clone().unwrap_or_else(|| {
                                agent_config.build_cmd_with_model(true, true, true, model_ref)
                            })
                        }
                        AgentRole::Reviewer => {
                            runtime.config.reviewer_cmd.clone().unwrap_or_else(|| {
                                agent_config.build_cmd_with_model(true, true, false, model_ref)
                            })
                        }
                        AgentRole::Commit => {
                            // Commit role doesn't have cmd override, use default
                            agent_config.build_cmd_with_model(true, true, false, model_ref)
                        }
                    }
                } else {
                    agent_config.build_cmd_with_model(
                        true,
                        true,
                        role == AgentRole::Developer,
                        model_ref,
                    )
                };

                // GLM-specific diagnostic output for print flag validation
                if crate::agents::is_glm_like_agent(agent_name)
                    && agent_index == 0
                    && cycle == 0
                    && model_index == 0
                {
                    let cmd_argv = split_command(&cmd_str).ok();
                    let has_print_flag = cmd_argv
                        .as_ref()
                        .is_some_and(|argv| argv.iter().any(|arg| arg == "-p"));
                    if !has_print_flag {
                        if agent_config.print_flag.is_empty() {
                            runtime.logger.warn(&format!(
                                "GLM agent '{agent_name}' is missing '-p' flag: print_flag is empty in configuration. \
                                 Add 'print_flag = \"-p\"' to [ccs] section in ~/.config/ralph-workflow.toml"
                            ));
                        } else {
                            runtime.logger.warn(&format!(
                                "GLM agent '{agent_name}' may be missing '-p' flag in command. Check configuration."
                            ));
                        }
                    }
                }

                let model_suffix = model_flag
                    .as_ref()
                    .map(|m| format!(" [{m}]"))
                    .unwrap_or_default();
                let display_name = registry.display_name(agent_name);
                let label = format!("{base_label} ({display_name}{model_suffix})");
                // Sanitize agent name for log file path - replace "/" with "-" to avoid
                // creating subdirectories (e.g., "ccs/glm" -> "ccs-glm")
                let safe_agent_name = agent_name.replace('/', "-");
                let logfile = format!("{logfile_prefix}_{safe_agent_name}_{model_index}.log");

                // Try this agent/model configuration with retries
                let attempt_config = crate::pipeline::fallback::AgentAttemptConfig {
                    agent_name,
                    model_flag: model_flag.as_deref(),
                    label: &label,
                    display_name: &display_name,
                    cmd_str: &cmd_str,
                    prompt,
                    logfile: &logfile,
                    parser_type,
                    env_vars: &agent_config.env_vars,
                    model_index,
                    agent_index,
                    cycle: cycle as usize,
                    fallback_config,
                };
                let result = try_agent_with_retries(&attempt_config, runtime)?;

                match result {
                    TryAgentResult::Success => return Ok(0),
                    TryAgentResult::Unrecoverable(exit_code) => return Ok(exit_code),
                    TryAgentResult::Fallback => {
                        // Break to next model/agent
                        break;
                    }
                    TryAgentResult::NoRetry => {
                        // Non-retriable error - continue to next model/agent
                    }
                }
            }
        }
    }

    // All cycles exhausted
    runtime.logger.error(&format!(
        "All agents exhausted after {} cycles with exponential backoff",
        fallback_config.max_cycles
    ));
    Ok(1)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set_multiple(vars: &[(&'static str, &str)]) -> Vec<Self> {
            let _lock = ENV_MUTEX
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            vars.iter()
                .map(|&(key, value)| {
                    let prev = std::env::var_os(key);
                    std::env::set_var(key, value);
                    Self { key, prev }
                })
                .collect()
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    /// Test that environment variable sanitization works correctly.
    ///
    /// This regression test ensures that when running agents with empty `env_vars`,
    /// GLM/CCS environment variables from the parent shell are NOT passed to
    /// the subprocess. This is critical for preventing "Invalid API key" errors
    /// when switching between GLM (CCS) and standard Claude agents.
    ///
    /// The test:
    /// 1. Sets GLM-like environment variables in the test process
    /// 2. Creates a Command that would be used for an agent with empty `env_vars`
    /// 3. Verifies that the problematic Anthropic env vars are cleared
    #[test]
    fn test_runner_sanitizes_anthropic_env_vars() {
        // Anthropic environment variables to sanitize
        const ANTHROPIC_ENV_VARS_TO_SANITIZE: &[&str] = &[
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
        ];

        let _guard = EnvGuard::set_multiple(&[
            ("ANTHROPIC_API_KEY", "test-token-glm"),
            ("ANTHROPIC_BASE_URL", "https://glm.example.com"),
        ]);

        // Simulate running an agent with empty env_vars (like codex)
        // The ANTHROPIC_* vars should be sanitized from the parent environment
        let mut cmd = std::process::Command::new("printenv");
        for &var in ANTHROPIC_ENV_VARS_TO_SANITIZE {
            cmd.env_remove(var);
        }

        // Execute the command and check that GLM variables are NOT present
        let output = cmd.output().expect("Failed to execute printenv");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // The GLM-set variables should NOT be in the subprocess environment
        // (they were sanitized by env_remove)
        assert!(!stdout.contains("test-token-glm"));
        assert!(!stdout.contains("https://glm.example.com"));
    }

    #[test]
    fn test_runner_does_not_sanitize_explicit_env_vars() {
        // If an agent explicitly sets ANTHROPIC_API_KEY in its env_vars,
        // that should NOT be sanitized

        let mut cmd = std::process::Command::new("printenv");

        // Simulate agent setting its own ANTHROPIC_API_KEY
        let agent_env_vars =
            std::collections::HashMap::from([("ANTHROPIC_API_KEY", "agent-specific-key")]);

        // First, sanitize all Anthropic vars
        for &var in &["ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL"] {
            cmd.env_remove(var);
        }

        // Then, apply agent's env_vars (which should NOT be sanitized)
        for (key, value) in &agent_env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().expect("Failed to execute printenv");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // The agent-specific key should be present
        assert!(stdout.contains("agent-specific-key"));
    }
}
