//! Command execution helpers and fallback orchestration.

#![cfg_attr(feature = "security-mode", expect(dead_code))]
#![expect(clippy::too_many_lines)]
#![expect(clippy::needless_pass_by_value)]
#![expect(clippy::items_after_statements)]
#![expect(clippy::clone_on_copy)]
#![expect(clippy::needless_continue)]
#![expect(clippy::option_if_let_else)]
#![expect(clippy::match_same_arms)]

use crate::agents::{
    is_glm_like_agent, validate_model_flag, AgentRegistry, AgentRole, JsonParserType,
};
use crate::common::utils::{format_argv_for_log, split_command, truncate_text};
use std::io;

#[cfg(feature = "security-mode")]
use std::process::{Command, Stdio};

use super::fallback::try_agent_with_retries;
use super::model_flag::resolve_model_with_provider;
use super::PipelineRuntime;

#[cfg(feature = "security-mode")]
use crate::config::Config;
#[cfg(feature = "security-mode")]
use crate::container::config::{ContainerConfig, ExecutionOptions};
#[cfg(feature = "security-mode")]
use crate::container::{
    ContainerEngine, ContainerExecutor, EngineType, SecurityMode, UserAccountExecutor,
};
#[cfg(feature = "security-mode")]
use crate::git_helpers::get_repo_root;
#[cfg(feature = "security-mode")]
use crate::logger::format_generic_json_for_display;
#[cfg(feature = "security-mode")]
use std::fs::OpenOptions;
#[cfg(feature = "security-mode")]
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(feature = "security-mode")]
use std::path::PathBuf;

/// Container execution context
#[cfg(feature = "security-mode")]
struct ContainerContext<'a> {
    engine: ContainerEngine,
    executor: ContainerExecutor,
    options: ExecutionOptions,
    _phantom: std::marker::PhantomData<&'a ()>,
}

/// User account execution context
#[cfg(feature = "security-mode")]
struct UserAccountContext<'a> {
    executor: UserAccountExecutor,
    options: ExecutionOptions,
    _phantom: std::marker::PhantomData<&'a ()>,
}

/// Try to initialize security mode if enabled
///
/// Returns None if security mode is disabled or initialization fails,
/// allowing graceful fallback to direct execution.
#[cfg(feature = "security-mode")]
fn try_init_security_mode(config: &Config) -> Option<SecurityModeContext<'static>> {
    // Determine security mode
    let security_mode_str = config.security_mode.as_deref().unwrap_or("auto");
    let security_mode: SecurityMode = match security_mode_str.parse() {
        Ok(mode) => mode,
        Err(e) => {
            eprintln!("Warning: Invalid security mode '{security_mode_str}': {e}. Using auto.");
            SecurityMode::Auto
        }
    };

    // Resolve Auto mode based on platform and availability
    let resolved_mode = match security_mode {
        SecurityMode::Auto => SecurityMode::default_for_platform(),
        other => other,
    };

    match resolved_mode {
        SecurityMode::Container => {
            // Check if container mode is explicitly disabled
            if !config.container_mode {
                return None;
            }

            // Get repository root
            let repo_root = match get_repo_root() {
                Ok(root) => root,
                Err(e) => {
                    eprintln!(
                        "Warning: Container mode enabled but couldn't detect repository root: {e}. \
                         Falling back to direct execution."
                    );
                    return None;
                }
            };

            // Determine engine type from config
            let engine_type = match config.container_engine.as_deref() {
                Some("docker") => EngineType::Docker,
                Some("podman") => EngineType::Podman,
                Some("auto") | None => EngineType::Auto,
                Some(other) => {
                    eprintln!("Warning: Unknown container engine '{other}'. Using auto-detection.");
                    EngineType::Auto
                }
            };

            // Detect container engine
            let engine = match ContainerEngine::detect(engine_type) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to detect container runtime: {e}. \
                         Falling back to direct execution. Use --no-container-mode to suppress this warning."
                    );
                    return None;
                }
            };

            // Determine container image
            let image = config
                .container_image
                .clone()
                .unwrap_or_else(|| "ralph-agent:latest".to_string());

            // Auto-pull image if not present locally
            if config.container_auto_pull.unwrap_or(true) {
                match engine.image_exists(&image) {
                    Ok(exists) => {
                        if !exists {
                            eprintln!("Container image '{image}' not found locally. Pulling...");
                            if let Err(e) = engine.pull_image(&image) {
                                eprintln!(
                                    "Warning: Failed to pull container image '{image}': {e}. \
                                     Falling back to direct execution."
                                );
                                return None;
                            }
                            eprintln!("Successfully pulled image '{image}'");
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to check if image '{image}' exists locally: {e}. \
                             Proceeding with container execution."
                        );
                    }
                }
            }

            // Create container config
            let container_config = ContainerConfig::new(repo_root, PathBuf::from(".agent"), image)
                .with_enabled(true)
                .with_engine(engine_type)
                .with_network(config.container_network);

            // Create container executor
            let executor = ContainerExecutor::new(container_config);

            // Create execution options
            let options = ExecutionOptions::default();

            Some(SecurityModeContext::Container(Box::new(ContainerContext {
                engine,
                executor,
                options,
                _phantom: std::marker::PhantomData,
            })))
        }
        SecurityMode::UserAccount => {
            // Get repository root
            let workspace_path = match get_repo_root() {
                Ok(root) => root,
                Err(e) => {
                    eprintln!(
                        "Warning: User account mode enabled but couldn't detect repository root: {e}. \
                         Falling back to direct execution."
                    );
                    return None;
                }
            };

            // Create user account executor
            let executor = match UserAccountExecutor::new(
                workspace_path,
                PathBuf::from(".agent"),
                None, // Use default user name
            ) {
                Ok(exec) => exec,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to initialize user account mode: {e}. \
                         Falling back to direct execution.\n\
                         To set up user account mode, run: sudo useradd -m -s /bin/bash ralph-agent"
                    );
                    return None;
                }
            };

            // Create execution options
            let options = ExecutionOptions::default();

            Some(SecurityModeContext::UserAccount(Box::new(
                UserAccountContext {
                    executor,
                    options,
                    _phantom: std::marker::PhantomData,
                },
            )))
        }
        SecurityMode::Auto | SecurityMode::None => None,
    }
}

/// Attempt to initialize user account mode as a fallback from container mode
///
/// This is called when container execution fails, providing a graceful fallback
/// to user account mode before finally falling back to direct execution.
#[cfg(feature = "security-mode")]
fn fallback_to_user_account_mode(_config: &Config) -> Option<UserAccountContext<'static>> {
    // Get repository root
    let workspace_path = match get_repo_root() {
        Ok(root) => root,
        Err(_e) => {
            return None;
        }
    };

    // Create user account executor
    let executor = match UserAccountExecutor::new(
        workspace_path,
        PathBuf::from(".agent"),
        None, // Use default user name
    ) {
        Ok(exec) => exec,
        Err(_e) => {
            return None;
        }
    };

    // Create execution options
    let options = ExecutionOptions::default();

    Some(UserAccountContext {
        executor,
        options,
        _phantom: std::marker::PhantomData,
    })
}

/// Security mode execution context (enum for different modes)
#[cfg(feature = "security-mode")]
enum SecurityModeContext<'a> {
    Container(Box<ContainerContext<'a>>),
    UserAccount(Box<UserAccountContext<'a>>),
}

/// Configuration for direct command execution
#[cfg(feature = "security-mode")]
struct DirectExecutionConfig<'a> {
    argv: &'a [String],
    prompt: &'a str,
    env_vars: &'a std::collections::HashMap<String, String>,
    logfile: &'a str,
    parser_type: JsonParserType,
    display_name: &'a str,
    uses_json: bool,
}

/// Execute a command directly (not in a container)
///
/// This is the fallback path when container mode is disabled or unavailable.
#[cfg(feature = "security-mode")]
fn execute_command_direct(
    config: DirectExecutionConfig<'_>,
    runtime: &PipelineRuntime<'_>,
) -> io::Result<(i32, String)> {
    // Validate prompt for null bytes which are universally invalid in command arguments
    if config.prompt.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Prompt contains null byte which is invalid for command execution",
        ));
    }

    // Build command
    let mut command = Command::new(&config.argv[0]);
    command.args(&config.argv[1..]);
    command.arg(config.prompt);

    // Inject environment variables
    for (key, value) in config.env_vars {
        command.env(key, value);
    }

    // Spawn process
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
            return Ok((exit_code, format!("{}: {}", config.argv[0], e)));
        }
        Err(e) => return Err(e),
    };

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;
    let reader = BufReader::new(stdout);

    // Drain stderr concurrently to avoid deadlocks when stderr output is large.
    const STDERR_MAX_BYTES: usize = 512 * 1024;
    let stderr_join_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || -> io::Result<String> {
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

    if config.uses_json {
        let stdout = io::stdout();
        let mut out = stdout.lock();

        match config.parser_type {
            JsonParserType::Claude => {
                let p = crate::json_parser::ClaudeParser::new(
                    (*runtime.colors).clone(),
                    runtime.config.verbosity,
                )
                .with_display_name(config.display_name)
                .with_log_file(config.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Codex => {
                let p = crate::json_parser::CodexParser::new(
                    (*runtime.colors).clone(),
                    runtime.config.verbosity,
                )
                .with_display_name(config.display_name)
                .with_log_file(config.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Gemini => {
                let p = crate::json_parser::GeminiParser::new(
                    (*runtime.colors).clone(),
                    runtime.config.verbosity,
                )
                .with_display_name(config.display_name)
                .with_log_file(config.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::OpenCode => {
                let p = crate::json_parser::OpenCodeParser::new(
                    (*runtime.colors).clone(),
                    runtime.config.verbosity,
                )
                .with_display_name(config.display_name)
                .with_log_file(config.logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Generic => {
                // This branch shouldn't happen when config.uses_json=true, but keep it safe.
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
        let mut logfile_handle = OpenOptions::new()
            .create(true)
            .append(true)
            .open(config.logfile)?;

        let stdout = io::stdout();
        let mut out = stdout.lock();

        for line in reader.lines() {
            let line = line?;
            writeln!(out, "{line}")?;
            writeln!(logfile_handle, "{line}")?;
        }
    }

    // Wait for command completion and collect stderr.
    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(1);
    let stderr_output = match stderr_join_handle {
        Some(handle) => handle.join().unwrap_or_else(|_| Ok(String::new()))?,
        None => String::new(),
    };

    Ok((exit_code, stderr_output))
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
    let last_exit_code = 1;

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
                    let full_cmd_log = match cmd_argv.as_ref() {
                        Some(argv) => {
                            let mut argv_for_log = argv.clone();
                            argv_for_log.push("<PROMPT>".to_string());
                            truncate_text(&format_argv_for_log(&argv_for_log), 160)
                        }
                        None => "<unparseable command>".to_string(),
                    };

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

                // Prepare environment variables for the agent
                let env_vars = std::collections::HashMap::new();

                // Try agent with retries
                let result = try_agent_with_retries(
                    agent_name,
                    model_ref,
                    &label,
                    &display_name,
                    &cmd_str,
                    prompt,
                    &logfile,
                    parser_type,
                    &env_vars,
                    model_index,
                    agent_index,
                    cycle as usize,
                    runtime,
                    fallback_config,
                )?;

                match result {
                    super::fallback::TryAgentResult::Success => return Ok(0),
                    super::fallback::TryAgentResult::Unrecoverable(exit_code) => {
                        return Ok(exit_code)
                    }
                    super::fallback::TryAgentResult::Fallback => {
                        // Continue to next model/agent
                        continue;
                    }
                    super::fallback::TryAgentResult::NoRetry => {
                        // Continue to next model/agent
                        continue;
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
