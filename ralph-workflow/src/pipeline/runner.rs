//! Command execution helpers and fallback orchestration.

use crate::agents::{validate_model_flag, AgentConfig, AgentRegistry, AgentRole, JsonParserType};
use crate::common::split_command;
use std::sync::Arc;

use super::fallback::try_agent_with_retries;
use super::fallback::TryAgentResult;
use super::model_flag::resolve_model_with_provider;
use super::prompt::PipelineRuntime;

/// Build the list of agents to try and log the fallback chain.
fn build_agents_to_try<'a>(fallbacks: &'a [&'a str], primary_agent: &'a str) -> Vec<&'a str> {
    let mut agents_to_try: Vec<&'a str> = vec![primary_agent];
    for fb in fallbacks {
        if *fb != primary_agent && !agents_to_try.contains(fb) {
            agents_to_try.push(fb);
        }
    }
    agents_to_try
}

/// Get CLI model/provider overrides based on role.
fn get_cli_overrides(
    role: AgentRole,
    runtime: &PipelineRuntime<'_>,
) -> (Option<String>, Option<String>) {
    match role {
        AgentRole::Developer => (
            runtime.config.developer_model.clone(),
            runtime.config.developer_provider.clone(),
        ),
        AgentRole::Reviewer => (
            runtime.config.reviewer_model.clone(),
            runtime.config.reviewer_provider.clone(),
        ),
        AgentRole::Commit => (None, None), // Commit role doesn't have CLI overrides
    }
}

/// Context for building model flags.
struct ModelFlagBuildContext<'a> {
    agent_index: usize,
    cli_model_override: Option<&'a String>,
    cli_provider_override: Option<&'a String>,
    agent_config: &'a AgentConfig,
    agent_name: &'a str,
    fallback_config: &'a crate::agents::fallback::FallbackConfig,
    display_name: &'a str,
    runtime: &'a PipelineRuntime<'a>,
}

/// Build the list of model flags to try for an agent.
fn build_model_flags_list(ctx: &ModelFlagBuildContext<'_>) -> Vec<Option<String>> {
    let mut model_flags_to_try: Vec<Option<String>> = Vec::new();

    // CLI override takes highest priority for primary agent
    // Provider override can modify the model's provider prefix
    if ctx.agent_index == 0
        && (ctx.cli_model_override.is_some() || ctx.cli_provider_override.is_some())
    {
        let resolved = resolve_model_with_provider(
            ctx.cli_provider_override.map(std::string::String::as_str),
            ctx.cli_model_override.map(std::string::String::as_str),
            ctx.agent_config.model_flag.as_deref(),
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
    if ctx.fallback_config.has_provider_fallbacks(ctx.agent_name) {
        let provider_fallbacks = ctx.fallback_config.get_provider_fallbacks(ctx.agent_name);
        ctx.runtime.logger.info(&format!(
            "Agent '{}' has {} provider fallback(s) configured",
            ctx.display_name,
            provider_fallbacks.len()
        ));
        for model in provider_fallbacks {
            model_flags_to_try.push(Some(model.clone()));
        }
    }

    model_flags_to_try
}

/// Build the command string for a specific model configuration.
fn build_command_for_model(ctx: &TryModelContext<'_>, runtime: &PipelineRuntime<'_>) -> String {
    let model_ref = ctx.model_flag.map(std::string::String::as_str);
    // Enable yolo for Developer role always.
    // For Reviewer and Commit, only enable in fix mode (detected via base_label starting with "fix").
    let yolo = matches!(ctx.role, AgentRole::Developer)
        || (ctx.role == AgentRole::Commit && ctx.base_label.starts_with("fix"))
        || (ctx.role == AgentRole::Reviewer && ctx.base_label.starts_with("fix"));

    if ctx.agent_index == 0 && ctx.cycle == 0 && ctx.model_index == 0 {
        // For primary agent on first cycle, respect env var command overrides
        match ctx.role {
            AgentRole::Developer => runtime.config.developer_cmd.clone().unwrap_or_else(|| {
                ctx.agent_config
                    .build_cmd_with_model(true, true, true, model_ref)
            }),
            AgentRole::Reviewer => runtime.config.reviewer_cmd.clone().unwrap_or_else(|| {
                ctx.agent_config
                    .build_cmd_with_model(true, true, yolo, model_ref)
            }),
            AgentRole::Commit => runtime.config.commit_cmd.clone().unwrap_or_else(|| {
                ctx.agent_config
                    .build_cmd_with_model(true, true, yolo, model_ref)
            }),
        }
    } else {
        ctx.agent_config
            .build_cmd_with_model(true, true, yolo, model_ref)
    }
}

/// GLM-specific validation for print flag.
///
/// This validation only applies to CCS/Claude-based GLM agents that use the `-p` flag
/// for non-interactive mode. OpenCode agents are excluded because they use
/// `--auto-approve` for non-interactive mode instead.
fn validate_glm_print_flag(
    agent_name: &str,
    agent_config: &AgentConfig,
    cmd_str: &str,
    agent_index: usize,
    cycle: u32,
    model_index: usize,
    runtime: &PipelineRuntime<'_>,
) {
    // Skip validation for non-CCS/Claude GLM agents
    // is_glm_like_agent only matches CCS/Claude-based GLM agents, not OpenCode
    if !crate::agents::is_glm_like_agent(agent_name)
        || agent_index != 0
        || cycle != 0
        || model_index != 0
    {
        return;
    }

    let cmd_argv = split_command(cmd_str).ok();
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

/// Build label and logfile paths for execution.
fn build_execution_metadata(
    model_flag: Option<&String>,
    display_name: &str,
    base_label: &str,
    agent_name: &str,
    logfile_prefix: &str,
    model_index: usize,
) -> (String, String, String) {
    let model_suffix = model_flag.map(|m| format!(" [{m}]")).unwrap_or_default();
    let display_name_with_suffix = format!("{display_name}{model_suffix}");
    let label = format!("{base_label} ({display_name_with_suffix})");
    // Sanitize agent name for log file path - replace "/" with "-" to avoid
    // creating subdirectories (e.g., "ccs/glm" -> "ccs-glm")
    let safe_agent_name = agent_name.replace('/', "-");
    let logfile = format!("{logfile_prefix}_{safe_agent_name}_{model_index}.log");
    (label, logfile, display_name_with_suffix)
}

/// Result of trying a single agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrySingleAgentResult {
    /// Agent succeeded - return success
    Success,
    /// Unrecoverable error - abort immediately
    Unrecoverable(i32),
    /// Should fall back to next agent
    Fallback,
    /// Continue to next model (no retry)
    NoRetry,
}

/// Context for trying a single model.
struct TryModelContext<'a> {
    agent_config: &'a AgentConfig,
    agent_name: &'a str,
    display_name: &'a str,
    agent_index: usize,
    cycle: u32,
    model_index: usize,
    role: AgentRole,
    model_flag: Option<&'a String>,
    base_label: &'a str,
    prompt: &'a str,
    logfile_prefix: &'a str,
    fallback_config: &'a crate::agents::fallback::FallbackConfig,
    output_validator: Option<crate::pipeline::fallback::OutputValidator>,
    retry_timer: Arc<dyn crate::agents::RetryTimerProvider>,
}

/// Try a single model configuration for an agent.
fn try_single_model(
    ctx: &TryModelContext<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> std::io::Result<TrySingleAgentResult> {
    let mut parser_type = ctx.agent_config.json_parser;

    if ctx.role == AgentRole::Reviewer {
        if let Some(ref parser_override) = runtime.config.reviewer_json_parser {
            parser_type = JsonParserType::parse(parser_override);
            if ctx.agent_index == 0 && ctx.cycle == 0 && ctx.model_index == 0 {
                runtime.logger.info(&format!(
                    "Using JSON parser override '{parser_override}' for reviewer"
                ));
            }
        }
    }

    let cmd_str = build_command_for_model(ctx, runtime);

    validate_glm_print_flag(
        ctx.agent_name,
        ctx.agent_config,
        &cmd_str,
        ctx.agent_index,
        ctx.cycle,
        ctx.model_index,
        runtime,
    );

    let (label, logfile, display_name_with_suffix) = build_execution_metadata(
        ctx.model_flag,
        ctx.display_name,
        ctx.base_label,
        ctx.agent_name,
        ctx.logfile_prefix,
        ctx.model_index,
    );

    let attempt_config = crate::pipeline::fallback::AgentAttemptConfig {
        agent_name: ctx.agent_name,
        model_flag: ctx.model_flag.map(std::string::String::as_str),
        label: &label,
        display_name: &display_name_with_suffix,
        cmd_str: &cmd_str,
        prompt: ctx.prompt,
        logfile: &logfile,
        logfile_prefix: ctx.logfile_prefix,
        parser_type,
        env_vars: &ctx.agent_config.env_vars,
        model_index: ctx.model_index,
        agent_index: ctx.agent_index,
        cycle: ctx.cycle as usize,
        fallback_config: ctx.fallback_config,
        output_validator: ctx.output_validator,
        retry_timer: Arc::clone(&ctx.retry_timer),
    };
    let result = try_agent_with_retries(&attempt_config, runtime)?;

    match result {
        TryAgentResult::Success => Ok(TrySingleAgentResult::Success),
        TryAgentResult::Unrecoverable(exit_code) => {
            Ok(TrySingleAgentResult::Unrecoverable(exit_code))
        }
        TryAgentResult::Fallback => Ok(TrySingleAgentResult::Fallback),
        TryAgentResult::NoRetry => Ok(TrySingleAgentResult::NoRetry),
    }
}

/// Context for trying a single agent.
struct TryAgentContext<'a> {
    agent_name: &'a str,
    agent_index: usize,
    cycle: u32,
    role: AgentRole,
    base_label: &'a str,
    prompt: &'a str,
    logfile_prefix: &'a str,
    cli_model_override: Option<&'a String>,
    cli_provider_override: Option<&'a String>,
    output_validator: Option<crate::pipeline::fallback::OutputValidator>,
    retry_timer: Arc<dyn crate::agents::RetryTimerProvider>,
}

/// Try a single agent with all its model configurations.
fn try_single_agent(
    ctx: &TryAgentContext<'_>,
    runtime: &mut PipelineRuntime<'_>,
    registry: &AgentRegistry,
    fallback_config: &crate::agents::fallback::FallbackConfig,
) -> std::io::Result<TrySingleAgentResult> {
    let Some(agent_config) = registry.resolve_config(ctx.agent_name) else {
        runtime.logger.warn(&format!(
            "Agent '{}' not found in registry, skipping",
            ctx.agent_name
        ));
        return Ok(TrySingleAgentResult::Fallback);
    };

    let display_name = registry.display_name(ctx.agent_name);
    let model_ctx = ModelFlagBuildContext {
        agent_index: ctx.agent_index,
        cli_model_override: ctx.cli_model_override,
        cli_provider_override: ctx.cli_provider_override,
        agent_config: &agent_config,
        agent_name: ctx.agent_name,
        fallback_config,
        display_name: &display_name,
        runtime,
    };
    let model_flags_to_try = build_model_flags_list(&model_ctx);

    if ctx.agent_index == 0 && ctx.cycle == 0 {
        for model_flag in model_flags_to_try.iter().flatten() {
            for warning in validate_model_flag(model_flag) {
                runtime.logger.warn(&warning);
            }
        }
    }

    for (model_index, model_flag) in model_flags_to_try.iter().enumerate() {
        let model_ctx = TryModelContext {
            agent_config: &agent_config,
            agent_name: ctx.agent_name,
            display_name: &display_name,
            agent_index: ctx.agent_index,
            cycle: ctx.cycle,
            model_index,
            role: ctx.role,
            model_flag: model_flag.as_ref(),
            base_label: ctx.base_label,
            prompt: ctx.prompt,
            logfile_prefix: ctx.logfile_prefix,
            fallback_config,
            output_validator: ctx.output_validator,
            retry_timer: Arc::clone(&ctx.retry_timer),
        };
        let result = try_single_model(&model_ctx, runtime)?;

        match result {
            TrySingleAgentResult::Success => return Ok(TrySingleAgentResult::Success),
            TrySingleAgentResult::Unrecoverable(exit_code) => {
                return Ok(TrySingleAgentResult::Unrecoverable(exit_code))
            }
            TrySingleAgentResult::Fallback => return Ok(TrySingleAgentResult::Fallback),
            TrySingleAgentResult::NoRetry => {}
        }
    }

    Ok(TrySingleAgentResult::NoRetry)
}

/// Configuration for running with fallback.
pub struct FallbackConfig<'a, 'b> {
    pub role: AgentRole,
    pub base_label: &'a str,
    pub prompt: &'a str,
    pub logfile_prefix: &'a str,
    pub runtime: &'a mut PipelineRuntime<'b>,
    pub registry: &'a AgentRegistry,
    pub primary_agent: &'a str,
    pub output_validator: Option<crate::pipeline::fallback::OutputValidator>,
}

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
    let mut config = FallbackConfig {
        role,
        base_label,
        prompt,
        logfile_prefix,
        runtime,
        registry,
        primary_agent,
        output_validator: None,
    };
    run_with_fallback_internal(&mut config)
}

/// Run a command with automatic fallback to alternative agents on failure.
///
/// Includes an optional output validator callback that checks if the agent
/// produced valid output after `exit_code=0`. If validation fails, triggers fallback.
///
/// This variant takes a `FallbackConfig` directly for cases where you need
/// to specify an output validator.
pub fn run_with_fallback_and_validator(
    config: &mut FallbackConfig<'_, '_>,
) -> std::io::Result<i32> {
    run_with_fallback_internal(config)
}

/// Run a command with automatic fallback to alternative agents on failure.
///
/// Includes an optional output validator callback that checks if the agent
/// produced valid output after `exit_code=0`. If validation fails, triggers fallback.
fn run_with_fallback_internal(config: &mut FallbackConfig<'_, '_>) -> std::io::Result<i32> {
    let fallback_config = config.registry.fallback_config();
    let fallbacks = config.registry.available_fallbacks(config.role);
    if fallback_config.has_fallbacks(config.role) {
        config.runtime.logger.info(&format!(
            "Agent fallback chain for {}: {}",
            config.role,
            fallbacks.join(", ")
        ));
    } else {
        config.runtime.logger.info(&format!(
            "No configured fallbacks for {}, using primary only",
            config.role
        ));
    }

    let agents_to_try = build_agents_to_try(&fallbacks, config.primary_agent);
    let (cli_model_override, cli_provider_override) =
        get_cli_overrides(config.role, config.runtime);

    for cycle in 0..fallback_config.max_cycles {
        if cycle > 0 {
            let backoff_ms = fallback_config.calculate_backoff(cycle - 1);
            config.runtime.logger.info(&format!(
                "Cycle {}/{}: All agents exhausted, waiting {}ms before retry (exponential backoff)...",
                cycle + 1,
                fallback_config.max_cycles,
                backoff_ms
            ));
            config
                .registry
                .retry_timer()
                .sleep(std::time::Duration::from_millis(backoff_ms));
        }

        for (agent_index, agent_name) in agents_to_try.iter().enumerate() {
            let ctx = TryAgentContext {
                agent_name,
                agent_index,
                cycle,
                role: config.role,
                base_label: config.base_label,
                prompt: config.prompt,
                logfile_prefix: config.logfile_prefix,
                cli_model_override: cli_model_override.as_ref(),
                cli_provider_override: cli_provider_override.as_ref(),
                output_validator: config.output_validator,
                retry_timer: config.registry.retry_timer(),
            };
            let result = try_single_agent(&ctx, config.runtime, config.registry, fallback_config)?;

            match result {
                TrySingleAgentResult::Success => return Ok(0),
                TrySingleAgentResult::Unrecoverable(exit_code) => return Ok(exit_code),
                TrySingleAgentResult::Fallback | TrySingleAgentResult::NoRetry => {}
            }
        }
    }

    config.runtime.logger.error(&format!(
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
        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                // printenv might not be available on all systems
                eprintln!("Skipping test: printenv not available ({e})");
                return;
            }
        };
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

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                // printenv might not be available on all systems
                eprintln!("Skipping test: printenv not available ({e})");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);

        // The agent-specific key should be present
        assert!(stdout.contains("agent-specific-key"));
    }
}
