//! Command execution helpers and fallback orchestration.

use crate::agents::{
    auth_failure_advice, validate_model_flag, AgentErrorKind, AgentRegistry, AgentRole,
    JsonParserType,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::output::{argv_requests_json, format_generic_json_for_display};
use crate::timer::Timer;
use crate::utils::{split_command, Logger};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use super::model_flag::resolve_model_with_provider;
use super::types::CommandResult;

/// Runtime services required for running agent commands.
pub(crate) struct PipelineRuntime<'a> {
    pub(crate) timer: &'a mut Timer,
    pub(crate) logger: &'a Logger,
    pub(crate) colors: &'a Colors,
    pub(crate) config: &'a Config,
}

/// A single prompt-based agent invocation.
pub(crate) struct PromptCommand<'a> {
    pub(crate) label: &'a str,
    pub(crate) cmd_str: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) logfile: &'a str,
    pub(crate) parser_type: JsonParserType,
}

/// Run a command with a prompt argument.
///
/// This is an internal helper for `run_with_fallback`.
pub(crate) fn run_with_prompt(
    cmd: PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> io::Result<CommandResult> {
    runtime.timer.start_phase();
    runtime.logger.step(&format!(
        "{}{}{}",
        runtime.colors.bold(),
        cmd.label,
        runtime.colors.reset()
    ));

    // Save prompt to file
    if let Some(parent) = Path::new(&runtime.config.prompt_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&runtime.config.prompt_path, cmd.prompt)?;
    runtime.logger.info(&format!(
        "Prompt saved to {}{}{}",
        runtime.colors.cyan(),
        runtime.config.prompt_path.display(),
        runtime.colors.reset()
    ));

    // Copy to clipboard if interactive and pbcopy available
    if runtime.config.interactive {
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(cmd.prompt.as_bytes());
            }
            let _ = child.wait();
            runtime.logger.info(&format!(
                "Prompt copied to clipboard {}(pbpaste to view){}",
                runtime.colors.dim(),
                runtime.colors.reset()
            ));
        }
    }

    // Build full command
    let argv = split_command(cmd.cmd_str)?;
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command is empty",
        ));
    }
    runtime.logger.info(&format!(
        "Executing: {}{}...{}",
        runtime.colors.dim(),
        &format!("{} <PROMPT>", cmd.cmd_str)
            .chars()
            .take(80)
            .collect::<String>(),
        runtime.colors.reset()
    ));

    // Determine if JSON parsing is needed (based on parser type and command flags)
    let uses_json = cmd.parser_type != JsonParserType::Generic || argv_requests_json(&argv);

    runtime
        .logger
        .info(&format!("Using {} parser...", cmd.parser_type));
    if let Some(parent) = Path::new(cmd.logfile).parent() {
        fs::create_dir_all(parent)?;
    }
    File::create(cmd.logfile)?;

    // Execute command
    let mut child = match Command::new(&argv[0])
        .args(&argv[1..])
        .arg(cmd.prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) =>
        {
            let exit_code = if e.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            };
            return Ok(CommandResult {
                exit_code,
                stderr: format!("{}: {}", argv[0], e),
            });
        }
        Err(e) => return Err(e),
    };

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;
    let reader = BufReader::new(stdout);

    // Drain stderr concurrently to avoid deadlocks when stderr output is large.
    let stderr_join_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || -> io::Result<String> {
            let mut stderr_output = String::new();
            let mut reader = BufReader::new(stderr);
            reader.read_to_string(&mut stderr_output)?;
            Ok(stderr_output)
        })
    });

    if uses_json {
        let stdout = io::stdout();
        let mut out = stdout.lock();

        match cmd.parser_type {
            JsonParserType::Claude => {
                let p = crate::json_parser::ClaudeParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_log_file(cmd.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Codex => {
                let p =
                    crate::json_parser::CodexParser::new(*runtime.colors, runtime.config.verbosity)
                        .with_log_file(cmd.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Gemini => {
                let p = crate::json_parser::GeminiParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_log_file(cmd.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Generic => {
                // This branch shouldn't happen when uses_json=true, but keep it safe.
                let mut buf = String::new();
                for line in reader.lines() {
                    buf.push_str(&line?);
                    buf.push('\n');
                }
                let formatted = format_generic_json_for_display(&buf, runtime.config.verbosity);
                out.write_all(formatted.as_bytes())?;
            }
        }
    } else {
        // Plain-text mode: stream output and log it.
        let mut logfile = OpenOptions::new()
            .create(true)
            .append(true)
            .open(cmd.logfile)?;

        let stdout = io::stdout();
        let mut out = stdout.lock();

        for line in reader.lines() {
            let line = line?;
            writeln!(out, "{}", line)?;
            writeln!(logfile, "{}", line)?;
        }
    }

    // Wait for command completion and collect stderr.
    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(1);
    let stderr_output = match stderr_join_handle {
        Some(handle) => handle.join().unwrap_or_else(|_| Ok(String::new()))?,
        None => String::new(),
    };

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    Ok(CommandResult {
        exit_code,
        stderr: stderr_output,
    })
}

/// Run a command with automatic fallback to alternative agents on failure.
pub(crate) fn run_with_fallback(
    role: AgentRole,
    base_label: &str,
    prompt: &str,
    logfile_prefix: &str,
    runtime: &mut PipelineRuntime<'_>,
    registry: &AgentRegistry,
    primary_agent: &str,
) -> io::Result<i32> {
    let fallback_config = registry.fallback_config();
    let fallbacks = registry.available_fallbacks(role);
    if !fallback_config.has_fallbacks(role) {
        runtime.logger.info(&format!(
            "No configured fallbacks for {}, using primary only",
            role
        ));
    }

    // Build the list of agents to try
    let mut agents_to_try: Vec<&str> = vec![primary_agent];
    for fb in &fallbacks {
        if *fb != primary_agent && !agents_to_try.contains(fb) {
            agents_to_try.push(fb);
        }
    }

    // Track the last error for final reporting
    let mut last_exit_code = 1;

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
            let Some(agent_config) = registry.get(agent_name) else {
                runtime.logger.warn(&format!(
                    "Agent '{}' not found in registry, skipping",
                    agent_name
                ));
                continue;
            };

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
                    agent_name,
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
                let parser_type = agent_config.json_parser;

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
                    }
                } else {
                    agent_config.build_cmd_with_model(
                        true,
                        true,
                        role == AgentRole::Developer,
                        model_ref,
                    )
                };

                let model_suffix = model_flag
                    .as_ref()
                    .map(|m| format!(" [{}]", m))
                    .unwrap_or_default();
                let label = format!("{} ({}{})", base_label, agent_name, model_suffix);
                let logfile = format!("{}_{}_{}.log", logfile_prefix, agent_name, model_index);

                // Try with retries
                for retry in 0..fallback_config.max_retries {
                    if retry > 0 {
                        runtime.logger.info(&format!(
                            "Retry {}/{} for {}{}...",
                            retry, fallback_config.max_retries, agent_name, model_suffix,
                        ));
                    }

                    let result = run_with_prompt(
                        PromptCommand {
                            label: &label,
                            cmd_str: &cmd_str,
                            prompt,
                            logfile: &logfile,
                            parser_type,
                        },
                        runtime,
                    )?;

                    if result.exit_code == 0 {
                        return Ok(0);
                    }

                    last_exit_code = result.exit_code;

                    // Classify the error
                    let error_kind = AgentErrorKind::classify(result.exit_code, &result.stderr);

                    runtime.logger.warn(&format!(
                        "Agent '{}'{} failed: {} (exit code {})",
                        agent_name,
                        model_suffix,
                        error_kind.description(),
                        result.exit_code
                    ));

                    // Provide provider-specific auth advice for auth failures
                    if matches!(error_kind, AgentErrorKind::AuthFailure) {
                        runtime.logger.info(&auth_failure_advice(model_ref));
                    } else {
                        runtime.logger.info(error_kind.recovery_advice());
                    }

                    // Provide installation guidance for command not found errors
                    if error_kind.is_command_not_found() {
                        let binary = cmd_str.split_whitespace().next().unwrap_or(agent_name);
                        let guidance = crate::platform::InstallGuidance::for_binary(binary);
                        runtime.logger.info(&guidance.format());
                    }

                    // Provide network-specific guidance
                    if error_kind.is_network_error() {
                        runtime.logger.info(
                            "Tip: Check your internet connection, firewall, or VPN settings.",
                        );
                    }

                    // Provide context reduction hint for memory-related errors
                    if error_kind.suggests_smaller_context() {
                        runtime.logger.info("Tip: Try reducing context size with RALPH_DEVELOPER_CONTEXT=0 or RALPH_REVIEWER_CONTEXT=0");
                    }

                    // Check for unrecoverable errors - abort immediately
                    if error_kind.is_unrecoverable() {
                        runtime
                            .logger
                            .error("Unrecoverable error - cannot continue pipeline");
                        return Ok(last_exit_code);
                    }

                    // Check if we should fallback to next agent
                    if error_kind.should_fallback() {
                        runtime.logger.info(&format!(
                            "Switching from '{}'{} to next configured fallback...",
                            agent_name, model_suffix
                        ));
                        break; // break retry loop and model loop
                    }

                    if !error_kind.should_retry() {
                        runtime.logger.info("Not retrying (non-retriable error)");
                        break;
                    }

                    // Otherwise, continue retrying the same model/agent
                    if retry + 1 < fallback_config.max_retries {
                        runtime.logger.info(&format!(
                            "Retrying '{}'{} (attempt {}/{})",
                            agent_name,
                            model_suffix,
                            retry + 2,
                            fallback_config.max_retries
                        ));
                        let wait_ms = error_kind
                            .suggested_wait_ms()
                            .max(fallback_config.retry_delay_ms);
                        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
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
    Ok(last_exit_code)
}
