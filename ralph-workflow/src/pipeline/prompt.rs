//! Prompt-based command execution.

use crate::agents::{is_glm_like_agent, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use crate::config::Config;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::logger::{argv_requests_json, format_generic_json_for_display};
use crate::pipeline::Timer;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use super::clipboard::get_platform_clipboard_command;
use super::types::CommandResult;

/// A single prompt-based agent invocation.
pub struct PromptCommand<'a> {
    pub label: &'a str,
    pub display_name: &'a str,
    pub cmd_str: &'a str,
    pub prompt: &'a str,
    pub logfile: &'a str,
    pub parser_type: JsonParserType,
    pub env_vars: &'a std::collections::HashMap<String, String>,
}

/// Runtime services required for running agent commands.
pub struct PipelineRuntime<'a> {
    pub timer: &'a mut Timer,
    pub logger: &'a Logger,
    pub colors: &'a Colors,
    pub config: &'a Config,
}

/// Run a command with a prompt argument.
///
/// This is an internal helper for `run_with_fallback`.
#[expect(clippy::too_many_lines)]
pub fn run_with_prompt(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> io::Result<CommandResult> {
    // Anthropic environment variables to sanitize before spawning agent subprocesses.
    // This prevents GLM/CCS env vars from the parent shell from leaking into agents
    // that rely on default credentials (like ANTHROPIC_API_KEY for the standard Claude agent).
    const ANTHROPIC_ENV_VARS_TO_SANITIZE: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
    ];

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

    // Copy to clipboard if interactive
    if runtime.config.interactive {
        if let Some(clipboard_cmd) = get_platform_clipboard_command() {
            if let Ok(mut child) = Command::new(clipboard_cmd.binary)
                .args(clipboard_cmd.args)
                .stdin(Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(cmd.prompt.as_bytes());
                }
                let _ = child.wait();
                runtime.logger.info(&format!(
                    "Prompt copied to clipboard {}({}){}",
                    runtime.colors.dim(),
                    clipboard_cmd.paste_hint,
                    runtime.colors.reset()
                ));
            }
        }
    }

    // Build full command
    let argv = split_command(cmd.cmd_str)?;
    if argv.is_empty() || cmd.cmd_str.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command is empty or contains only whitespace",
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

    // Clear problematic Anthropic env vars that weren't explicitly set by the agent.
    for &var in ANTHROPIC_ENV_VARS_TO_SANITIZE {
        if !cmd.env_vars.contains_key(var) {
            command.env_remove(var);
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
        Some(handle) => match handle.join() {
            Ok(result) => result?,
            Err(panic_payload) => {
                // Thread panicked - try to extract panic message for diagnostics
                let panic_msg = panic_payload.downcast_ref::<String>().map_or_else(
                    || {
                        panic_payload.downcast_ref::<&str>().map_or_else(
                            || "<unknown panic>".to_string(),
                            std::string::ToString::to_string,
                        )
                    },
                    std::clone::Clone::clone,
                );
                runtime.logger.warn(&format!(
                    "Stderr collection thread panicked: {panic_msg}. This may indicate a bug."
                ));
                String::new()
            }
        },
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
