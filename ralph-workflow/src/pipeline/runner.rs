//! Command execution helpers and fallback orchestration.

use crate::agents::{
    auth_failure_advice, is_glm_like_agent, validate_model_flag, AgentErrorKind, AgentRegistry,
    AgentRole, JsonParserType,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::output::{argv_requests_json, format_generic_json_for_display};
use crate::timer::Timer;
use crate::utils::{format_argv_for_log, split_command, truncate_text, Logger};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use super::model_flag::resolve_model_with_provider;
use super::types::CommandResult;

/// Runtime services required for running agent commands.
pub struct PipelineRuntime<'a> {
    pub(crate) timer: &'a mut Timer,
    pub(crate) logger: &'a Logger,
    pub(crate) colors: &'a Colors,
    pub(crate) config: &'a Config,
}

/// A single prompt-based agent invocation.
pub struct PromptCommand<'a> {
    pub(crate) label: &'a str,
    pub(crate) display_name: &'a str,
    pub(crate) cmd_str: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) logfile: &'a str,
    pub(crate) parser_type: JsonParserType,
    pub(crate) env_vars: &'a std::collections::HashMap<String, String>,
}

/// Run a command with a prompt argument.
///
/// This is an internal helper for `run_with_fallback`.
pub fn run_with_prompt(
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

    let mut argv_for_log = argv.clone();
    argv_for_log.push("<PROMPT>".to_string());
    let display_cmd = truncate_text(&format_argv_for_log(&argv_for_log), 160);
    runtime.logger.info(&format!(
        "Executing: {}{}{}",
        runtime.colors.dim(),
        display_cmd,
        runtime.colors.reset()
    ));

    // GLM-specific debug logging
    // Check for GLM-like agents using the shared detection function
    let is_glm_cmd = is_glm_like_agent(cmd.cmd_str);

    if is_glm_cmd && runtime.config.verbosity.is_debug() {
        runtime
            .logger
            .info(&format!("GLM command details: {display_cmd}"));
        // Verify -p flag is present
        if argv.iter().any(|arg| arg == "-p") {
            runtime
                .logger
                .info("GLM command includes '-p' flag (correct)");
        } else {
            runtime.logger.warn("GLM command may be missing '-p' flag");
        }
    }

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
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command.arg(cmd.prompt);

    // Inject environment variables from agent config
    if !cmd.env_vars.is_empty() {
        if runtime.config.verbosity.is_debug() {
            runtime.logger.info(&format!(
                "Injecting {} environment variable(s) into subprocess",
                cmd.env_vars.len()
            ));
            // Show env var keys only (redact values for security)
            for key in cmd.env_vars.keys() {
                runtime.logger.info(&format!("  - {key}"));
            }
        }
        for (key, value) in cmd.env_vars {
            command.env(key, value);
        }
    }

    let mut child = match command
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
            const STDERR_MAX_BYTES: usize = 512 * 1024;

            let mut reader = BufReader::new(stderr);
            let mut buf = [0u8; 8192];
            let mut collected = Vec::<u8>::new();
            let mut truncated = false;

            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }

                let remaining = STDERR_MAX_BYTES.saturating_sub(collected.len());
                if remaining == 0 {
                    truncated = true;
                    break;
                }

                let to_take = remaining.min(n);
                collected.extend_from_slice(&buf[..to_take]);
                if to_take < n {
                    truncated = true;
                    break;
                }
            }

            let mut stderr_output = String::from_utf8_lossy(&collected).into_owned();
            if truncated {
                if !stderr_output.ends_with('\n') {
                    stderr_output.push('\n');
                }
                stderr_output.push_str("<stderr truncated>");
            }

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
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Codex => {
                let p =
                    crate::json_parser::CodexParser::new(*runtime.colors, runtime.config.verbosity)
                        .with_display_name(cmd.display_name)
                        .with_log_file(cmd.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Gemini => {
                let p = crate::json_parser::GeminiParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::OpenCode => {
                let p = crate::json_parser::OpenCodeParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
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
            writeln!(out, "{line}")?;
            writeln!(logfile, "{line}")?;
        }
    }

    // Wait for command completion and collect stderr.
    let status = child.wait()?;
    // If status.code() returns None (process terminated by signal on Unix),
    // we default to exit code 1 (failure). This treats signal termination
    // as a failure, which is appropriate for agent execution.
    let exit_code = status.code().unwrap_or(1);

    // Log if process was terminated by signal (no exit code available)
    if status.code().is_none() && runtime.config.verbosity.is_debug() {
        runtime
            .logger
            .warn("Process terminated by signal (no exit code), treating as failure");
    }

    let stderr_output = match stderr_join_handle {
        Some(handle) => handle.join().unwrap_or_else(|_| Ok(String::new()))?,
        None => String::new(),
    };

    // Debug logging for stderr output to help diagnose agent issues
    if !stderr_output.is_empty() && runtime.config.verbosity.is_debug() {
        runtime.logger.warn(&format!(
            "Agent stderr output detected ({} bytes):",
            stderr_output.len()
        ));
        // Show first few lines of stderr for debugging
        for (i, line) in stderr_output.lines().take(5).enumerate() {
            runtime.logger.info(&format!("  stderr[{i}]: {line}"));
        }
        if stderr_output.lines().count() > 5 {
            runtime.logger.info(&format!(
                "  ... ({} more lines, see log file for full output)",
                stderr_output.lines().count() - 5
            ));
        }
    }

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
pub fn run_with_fallback(
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

                // GLM-specific diagnostic output
                // Check for GLM-like agents using the shared detection function
                let is_glm_agent = is_glm_like_agent(agent_name);

                if is_glm_agent && agent_index == 0 && cycle == 0 && model_index == 0 {
                    let cmd_argv = split_command(&cmd_str).ok();
                    let full_cmd_log = cmd_argv
                        .as_ref()
                        .map(|argv| {
                            let mut argv_for_log = argv.clone();
                            argv_for_log.push("<PROMPT>".to_string());
                            truncate_text(&format_argv_for_log(&argv_for_log), 160)
                        })
                        .unwrap_or_else(|| "<unparseable command>".to_string());

                    if runtime.config.verbosity.is_debug() {
                        runtime
                            .logger
                            .info(&format!("GLM agent '{agent_name}' command configuration:"));
                        runtime
                            .logger
                            .info(&format!("  Base command: {}", agent_config.cmd));
                        runtime
                            .logger
                            .info(&format!("  Print flag: '{}'", agent_config.print_flag));
                        runtime
                            .logger
                            .info(&format!("  Output flag: '{}'", agent_config.output_flag));
                        runtime
                            .logger
                            .info(&format!("  YOLO flag: '{}'", agent_config.yolo_flag));
                        runtime
                            .logger
                            .info(&format!("  JSON parser: {:?}", agent_config.json_parser));
                        runtime
                            .logger
                            .info(&format!("  Full command: {full_cmd_log}"));
                    }
                    // Validate -p flag is present (warn if missing regardless of print_flag value)
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

                // Try with retries
                for retry in 0..fallback_config.max_retries {
                    if retry > 0 {
                        runtime.logger.info(&format!(
                            "Retry {}/{} for {}{}...",
                            retry, fallback_config.max_retries, display_name, model_suffix,
                        ));
                    }

                    let result = run_with_prompt(
                        PromptCommand {
                            label: &label,
                            display_name: &display_name,
                            cmd_str: &cmd_str,
                            prompt,
                            logfile: &logfile,
                            parser_type,
                            env_vars: &agent_config.env_vars,
                        },
                        runtime,
                    )?;

                    if result.exit_code == 0 {
                        return Ok(0);
                    }

                    last_exit_code = result.exit_code;

                    // Classify the error with agent context for better handling
                    let error_kind = AgentErrorKind::classify_with_agent(
                        result.exit_code,
                        &result.stderr,
                        Some(agent_name),
                        model_flag.as_deref(),
                    );

                    runtime.logger.warn(&format!(
                        "Agent '{}'{} failed: {} (exit code {})",
                        agent_name,
                        model_suffix,
                        error_kind.description(),
                        result.exit_code
                    ));

                    // GLM-specific diagnostics
                    // GLM (via CCS) has known issues that deserve special guidance
                    let is_glm_agent = is_glm_like_agent(agent_name);

                    if is_glm_agent
                        && matches!(
                            error_kind,
                            AgentErrorKind::AgentSpecificQuirk
                                | AgentErrorKind::ToolExecutionFailed
                        )
                    {
                        runtime.logger.warn(&format!(
                            "{}GLM Agent Issue Detected:{} GLM has known compatibility issues with Ralph.",
                            runtime.colors.yellow(),
                            runtime.colors.reset()
                        ));
                        runtime.logger.info("Suggested workarounds:");
                        runtime
                            .logger
                            .info("  1. Try: ralph --reviewer-agent codex");
                        runtime
                            .logger
                            .info("  2. Try: ralph --reviewer-json-parser generic");
                        runtime
                            .logger
                            .info("  3. Skip review: RALPH_REVIEWER_REVIEWS=0 ralph");
                        runtime
                            .logger
                            .info("See docs/agent-compatibility.md for details.");
                    }

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
                            "Switching from '{display_name}'{model_suffix} to next configured fallback..."
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
                            display_name,
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
