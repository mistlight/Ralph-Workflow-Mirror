#![deny(unsafe_code)]
//! Ralph: PROMPT-driven agent loop for git repos
//!
//! Runs:
//! - Developer agent: iterative progress against PROMPT.md
//! - Reviewer agent: review → fix → review passes
//! - Optional fast/full checks
//! - Final `git add -A` + `git commit -m <msg>`

mod agents;
mod colors;
mod config;
mod git_helpers;
mod json_parser;
mod language_detector;
mod platform;
mod prompts;
mod review_guidelines;
mod review_metrics;
mod timer;
mod utils;

use crate::agents::{
    auth_failure_advice, global_agents_config_path, strip_model_flag_prefix, validate_model_flag,
    AgentErrorKind, AgentRegistry, AgentRole, AgentsConfigFile, ConfigInitResult, JsonParserType,
    OpenCodeProviderType,
};
use crate::colors::Colors;
use crate::config::{Config, ReviewDepth, Verbosity};
use crate::git_helpers::{
    cleanup_agent_phase_silent, cleanup_orphaned_marker, disable_git_wrapper, end_agent_phase,
    get_repo_root, git_add_all, git_commit, git_snapshot, require_git_repo, start_agent_phase,
    uninstall_hooks, GitHelpers,
};
use crate::language_detector::{detect_stack, detect_stack_summary, ProjectStack};
use crate::prompts::{
    prompt_comprehensive_review, prompt_for_agent, prompt_incremental_review,
    prompt_security_focused_review, Action, ContextLevel, Role,
};
use crate::review_guidelines::{CheckSeverity, ReviewGuidelines};
use crate::review_metrics::ReviewMetrics;
use crate::timer::Timer;
use crate::utils::{
    checkpoint_exists, clean_context_for_reviewer, cleanup_generated_files, clear_checkpoint,
    delete_commit_message_file, delete_plan_file, ensure_files, load_checkpoint, print_progress,
    read_commit_message_file, reset_context_for_isolation, save_checkpoint, split_command,
    truncate_text, update_status, Logger, PipelineCheckpoint, PipelinePhase,
};
use clap::{Parser, ValueEnum};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Ralph: PROMPT-driven agent orchestrator for git repos
#[derive(Parser, Debug)]
#[command(name = "ralph")]
#[command(about = "PROMPT-driven multi-agent orchestrator for git repos")]
#[command(
    long_about = "Ralph orchestrates AI coding agents to implement changes based on PROMPT.md.\n\n\
    It runs a developer agent for code implementation, then a reviewer agent for\n\
    quality assurance, automatically staging and committing the final result."
)]
#[command(version)]
#[command(after_help = "WORKFLOW:\n\
    1. Create PROMPT.md with your requirements\n\
    2. Run: ralph \"feat: implement my feature\"\n\
    3. Ralph runs developer agent (N iterations)\n\
    4. Ralph runs reviewer agent (review -> fix -> re-review)\n\
    5. Changes are committed with the provided message\n\n\
CONFIGURATION:\n\
    Agents are configured in .agent/agents.toml (created on first run).\n\
    Run 'ralph --init' to create/view the config file.\n\
    Run 'ralph --list-agents' to see all configured agents.\n\n\
VERBOSITY LEVELS (-v LEVEL):\n\
    0 = quiet    Minimal output, hide tool inputs (--quiet or -q)\n\
    1 = normal   Balanced output, show tool inputs\n\
    2 = verbose  Default - generous limits for full context\n\
    3 = full     No truncation (--full)\n\
    4 = debug    Max verbosity with raw JSON (--debug)\n\n\
EXAMPLES:\n\
    ralph \"feat: add login button\"              Basic usage\n\
    ralph --quick \"fix: small bug\"              Quick mode (1 dev + 1 review)\n\
    ralph -Q \"feat: rapid prototype\"            Quick mode (shorthand)\n\
    ralph -q \"fix: typo\"                        Quiet mode (same as -v0)\n\
    ralph --full \"feat: complex change\"         Full output (same as -v3)\n\
    ralph --debug \"debug: investigate\"          Debug mode with raw JSON\n\
    ralph --developer-iters 3                    Custom iterations\n\
    ralph --preset opencode                      Use opencode for both\n\
    ralph --developer-agent aider                Use a different agent\n\n\
PLUMBING COMMANDS (for scripting):\n\
    ralph --generate-commit-msg                  Generate message only\n\
    ralph --show-commit-msg                      Display generated message\n\
    ralph --apply-commit                         Commit using generated message\n\n\
ENVIRONMENT VARIABLES:\n\
    RALPH_DEVELOPER_AGENT    Developer agent (from agent_chain)\n\
    RALPH_REVIEWER_AGENT     Reviewer agent (from agent_chain)\n\
    RALPH_DEVELOPER_ITERS    Developer iterations (default: 5)\n\
    RALPH_REVIEWER_REVIEWS   Re-review passes (default: 2)\n\
    RALPH_VERBOSITY          Verbosity level 0-4 (default: 2)\n\
    RALPH_ISOLATION_MODE     Isolation mode on/off (default: 1=on)\n\
    RALPH_AGENTS_CONFIG      Path to agents.toml")]
struct Args {
    /// Commit message for the final commit
    #[arg(
        default_value = "chore: apply PROMPT loop + review/fix/review",
        help = "Commit message for the final commit"
    )]
    commit_msg: String,

    /// Number of developer iterations (default: 5)
    #[arg(
        long = "developer-iters",
        env = "RALPH_DEVELOPER_ITERS",
        value_name = "N",
        help = "Number of developer agent iterations"
    )]
    developer_iters: Option<u32>,

    /// Number of review-fix iterations after initial fix (default: 2)
    #[arg(
        long = "reviewer-reviews",
        env = "RALPH_REVIEWER_REVIEWS",
        value_name = "N",
        help = "Number of review-fix iterations after initial fix"
    )]
    reviewer_reviews: Option<u32>,

    /// Preset for common agent combinations
    #[arg(
        long,
        env = "RALPH_PRESET",
        value_name = "NAME",
        help = "Use a preset agent combination (default, opencode)"
    )]
    preset: Option<Preset>,

    /// Developer/driver agent to use (from agent_chain.developer)
    #[arg(
        long,
        env = "RALPH_DEVELOPER_AGENT",
        aliases = ["driver-agent"],
        value_name = "AGENT",
        help = "Developer agent for code implementation (default: first in agent_chain.developer)"
    )]
    developer_agent: Option<String>,

    /// Reviewer agent to use (from agent_chain.reviewer)
    #[arg(
        long,
        env = "RALPH_REVIEWER_AGENT",
        value_name = "AGENT",
        help = "Reviewer agent for code review (default: first in agent_chain.reviewer)"
    )]
    reviewer_agent: Option<String>,

    /// Developer model/provider override (e.g., "-m opencode/glm-4.7-free")
    #[arg(
        long,
        env = "RALPH_DEVELOPER_MODEL",
        value_name = "MODEL_FLAG",
        help = "Model flag for developer agent (e.g., '-m opencode/glm-4.7-free')"
    )]
    developer_model: Option<String>,

    /// Reviewer model/provider override (e.g., "-m opencode/claude-sonnet-4")
    #[arg(
        long,
        env = "RALPH_REVIEWER_MODEL",
        value_name = "MODEL_FLAG",
        help = "Model flag for reviewer agent (e.g., '-m opencode/claude-sonnet-4')"
    )]
    reviewer_model: Option<String>,

    /// Developer provider override (e.g., "opencode", "zai", "anthropic", "openai")
    /// Use this to switch providers at runtime without changing agent config.
    /// Combined with the agent's model to form the full model flag.
    /// Provider types: 'opencode' (Zen gateway), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)
    #[arg(
        long,
        env = "RALPH_DEVELOPER_PROVIDER",
        value_name = "PROVIDER",
        help = "Provider for developer agent: 'opencode' (Zen), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)"
    )]
    developer_provider: Option<String>,

    /// Reviewer provider override (e.g., "opencode", "zai", "anthropic", "openai")
    /// Use this to switch providers at runtime without changing agent config.
    /// Combined with the agent's model to form the full model flag.
    /// Provider types: 'opencode' (Zen gateway), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)
    #[arg(
        long,
        env = "RALPH_REVIEWER_PROVIDER",
        value_name = "PROVIDER",
        help = "Provider for reviewer agent: 'opencode' (Zen), 'zai'/'zhipuai' (Z.AI direct), 'anthropic'/'openai' (direct API)"
    )]
    reviewer_provider: Option<String>,

    /// Verbosity level (0=quiet, 1=normal, 2=verbose, 3=full, 4=debug)
    #[arg(
        short,
        long,
        value_name = "LEVEL",
        value_parser = clap::value_parser!(u8).range(0..=4),
        help = "Output verbosity (0=quiet, 1=normal, 2=verbose [default], 3=full, 4=debug); overrides RALPH_VERBOSITY"
    )]
    verbosity: Option<u8>,

    /// Shorthand for --verbosity=0 (minimal output)
    #[arg(
        short,
        long,
        conflicts_with = "verbosity",
        help = "Quiet mode (same as -v0)"
    )]
    quiet: bool,

    /// Shorthand for --verbosity=3 (no truncation)
    #[arg(
        long,
        conflicts_with = "verbosity",
        help = "Full output mode, no truncation (same as -v3)"
    )]
    full: bool,

    /// Shorthand for --verbosity=4 (maximum verbosity with raw JSON)
    #[arg(long, conflicts_with = "verbosity", help = "Debug mode (same as -v4)")]
    debug: bool,

    /// Quick mode: 1 developer iteration, 1 review pass (fast turnaround)
    #[arg(
        long,
        short = 'Q',
        help = "Quick mode: 1 dev iteration + 1 review (for rapid prototyping)"
    )]
    quick: bool,

    /// Disable isolation mode (allow NOTES.md and ISSUES.md to persist)
    #[arg(
        long,
        help = "Disable isolation mode: keep NOTES.md and ISSUES.md between runs"
    )]
    no_isolation: bool,

    /// List all configured agents and exit
    #[arg(long, help = "Show all agents from registry and config file")]
    list_agents: bool,

    /// List only agents found in PATH and exit
    #[arg(long, help = "Show only agents that are installed and available")]
    list_available_agents: bool,

    /// List OpenCode provider types and their configuration
    #[arg(
        long,
        help = "Show OpenCode provider types with model prefixes and auth commands"
    )]
    list_providers: bool,

    /// Initialize agents.toml config file and exit
    #[arg(long, help = "Create .agent/agents.toml with default settings")]
    init: bool,

    /// Initialize global agents.toml config file and exit
    #[arg(
        long,
        help = "Create ~/.config/ralph/agents.toml with default settings"
    )]
    init_global: bool,

    // === Plumbing Commands ===
    // These are low-level operations for scripting and automation
    /// Generate commit message only (writes to .agent/commit-message.txt)
    #[arg(long, help = "Run only the commit message generation phase, then exit")]
    generate_commit_msg: bool,

    /// Apply commit using existing .agent/commit-message.txt
    #[arg(
        long,
        help = "Stage all changes and commit using .agent/commit-message.txt"
    )]
    apply_commit: bool,

    /// Show the generated commit message and exit
    #[arg(long, help = "Read and display .agent/commit-message.txt")]
    show_commit_msg: bool,

    // === Recovery Commands ===
    /// Resume from last checkpoint after an interruption
    #[arg(
        long,
        help = "Resume from last checkpoint (if one exists from a previous interrupted run)"
    )]
    resume: bool,

    /// Validate setup without running agents (dry run)
    #[arg(
        long,
        help = "Validate configuration and PROMPT.md without running agents"
    )]
    dry_run: bool,

    /// Output comprehensive diagnostic information
    #[arg(
        long,
        help = "Show system info, agent status, and config for troubleshooting"
    )]
    diagnose: bool,

    /// Review depth level (standard, comprehensive, security, incremental)
    #[arg(
        long,
        value_name = "LEVEL",
        help = "Review depth: standard (balanced), comprehensive (thorough), security (OWASP-focused), incremental (changed files only)"
    )]
    review_depth: Option<String>,
}

#[derive(Clone, Debug, ValueEnum)]
enum Preset {
    /// Use agent_chain defaults (no explicit agent override)
    Default,
    /// Use opencode for both developer and reviewer
    Opencode,
}

/// Statistics tracking
struct Stats {
    changes_detected: u32,
    developer_runs_completed: u32,
    reviewer_runs_completed: u32,
}

impl Stats {
    fn new() -> Self {
        Self {
            changes_detected: 0,
            developer_runs_completed: 0,
            reviewer_runs_completed: 0,
        }
    }
}

/// Result of running a command, including stderr for error classification
struct CommandResult {
    exit_code: i32,
    stderr: String,
}

fn argv_requests_json(argv: &[String]) -> bool {
    // Skip argv[0] (the executable); scan flags/args only.
    let mut iter = argv.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        if arg == "--json" || arg.starts_with("--json=") {
            return true;
        }

        if arg == "--output-format" {
            if let Some(next) = iter.peek() {
                let next = next.as_str();
                if next.contains("json") {
                    return true;
                }
            }
        }
        if let Some((flag, value)) = arg.split_once('=') {
            if flag == "--output-format" && value.contains("json") {
                return true;
            }
            if flag == "--format" && value == "json" {
                return true;
            }
        }

        if arg == "--format" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "json" {
                    return true;
                }
            }
        }

        // Some CLIs use short flags like -F json or -o stream-json
        if arg == "-F" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "json" {
                    return true;
                }
            }
        }
        if arg.starts_with("-F") && arg != "-F" && arg.trim_start_matches("-F") == "json" {
            return true;
        }

        if arg == "-o" {
            if let Some(next) = iter.peek() {
                let next = next.as_str();
                if next.contains("json") {
                    return true;
                }
            }
        }
        if arg.starts_with("-o") && arg != "-o" && arg.trim_start_matches("-o").contains("json") {
            return true;
        }
    }
    false
}

fn format_generic_json_for_display(line: &str, verbosity: Verbosity) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return truncate_text(line, verbosity.truncate_limit("agent_msg"));
    };

    let formatted = match verbosity {
        Verbosity::Full | Verbosity::Debug => {
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| line.to_string())
        }
        _ => serde_json::to_string(&value).unwrap_or_else(|_| line.to_string()),
    };
    truncate_text(&formatted, verbosity.truncate_limit("agent_msg"))
}

/// Run a command with a prompt argument (internal helper for run_with_fallback)
#[allow(clippy::too_many_arguments)]
fn run_with_prompt(
    label: &str,
    cmd_str: &str,
    prompt: &str,
    logfile: &str,
    parser_type: JsonParserType,
    timer: &mut Timer,
    logger: &Logger,
    colors: &Colors,
    config: &Config,
) -> io::Result<CommandResult> {
    timer.start_phase();
    logger.step(&format!("{}{}{}", colors.bold(), label, colors.reset()));

    // Save prompt to file
    if let Some(parent) = Path::new(&config.prompt_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&config.prompt_path, prompt)?;
    logger.info(&format!(
        "Prompt saved to {}{}{}",
        colors.cyan(),
        config.prompt_path.display(),
        colors.reset()
    ));

    // Copy to clipboard if interactive and pbcopy available
    if config.interactive {
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(prompt.as_bytes());
            }
            let _ = child.wait();
            logger.info(&format!(
                "Prompt copied to clipboard {}(pbpaste to view){}",
                colors.dim(),
                colors.reset()
            ));
        }
    }

    // Build full command
    let argv = split_command(cmd_str)?;
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command is empty",
        ));
    }
    logger.info(&format!(
        "Executing: {}{}...{}",
        colors.dim(),
        &format!("{} <PROMPT>", cmd_str)
            .chars()
            .take(80)
            .collect::<String>(),
        colors.reset()
    ));

    // Determine if JSON parsing is needed (based on parser type and command flags)
    let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);

    logger.info(&format!("Using {} parser...", parser_type));
    if let Some(parent) = Path::new(logfile).parent() {
        fs::create_dir_all(parent)?;
    }
    File::create(logfile)?;

    // Execute command
    let mut child = match Command::new(&argv[0])
        .args(&argv[1..])
        .arg(prompt)
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

        match parser_type {
            JsonParserType::Claude => {
                let p = crate::json_parser::ClaudeParser::new(*colors, config.verbosity)
                    .with_log_file(logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Codex => {
                let p = crate::json_parser::CodexParser::new(*colors, config.verbosity)
                    .with_log_file(logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Gemini => {
                let p = crate::json_parser::GeminiParser::new(*colors, config.verbosity)
                    .with_log_file(logfile);
                p.parse_stream(reader, &mut out)?;
            }
            JsonParserType::Generic => {
                let log_file = OpenOptions::new().create(true).append(true).open(logfile)?;
                let mut log_writer = io::BufWriter::new(log_file);

                for line in reader.lines() {
                    let line = line?;
                    if line.is_empty() {
                        continue;
                    }

                    let display = if config.verbosity.is_debug() {
                        line.clone()
                    } else {
                        format_generic_json_for_display(&line, config.verbosity)
                    };

                    let output = format!("{}[Agent]{} {}\n", colors.dim(), colors.reset(), display);
                    write!(out, "{}", output)?;

                    writeln!(log_writer, "{}", line)?;
                }
            }
        }
    } else {
        // Non-JSON mode: just pipe through
        let log_file = OpenOptions::new().create(true).append(true).open(logfile)?;
        let mut log_writer = io::BufWriter::new(log_file);
        for line in reader.lines() {
            let line = line?;
            println!("{}", line);
            writeln!(log_writer, "{}", line)?;
        }
    }

    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(1);

    // Collect stderr (drained on a background thread).
    let stderr = if let Some(handle) = stderr_join_handle {
        match handle.join() {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => {
                logger.warn(&format!("Failed to read stderr: {}", err));
                String::new()
            }
            Err(_) => {
                logger.warn("Failed to join stderr reader thread");
                String::new()
            }
        }
    } else {
        String::new()
    };

    if exit_code != 0 {
        logger.error(&format!("Command exited with code {}", exit_code));
        if !stderr.is_empty() {
            logger.error(&format!("stderr: {}", stderr.lines().next().unwrap_or("")));
        }
    } else {
        logger.success(&format!("Completed in {}", timer.phase_elapsed_formatted()));
    }

    Ok(CommandResult { exit_code, stderr })
}

/// Extract model name from a model flag or full model string
///
/// Examples:
/// - "-m opencode/glm-4.7-free" -> "glm-4.7-free"
/// - "anthropic/claude-sonnet-4" -> "claude-sonnet-4"
/// - "claude-sonnet-4" -> "claude-sonnet-4"
fn extract_model_name(model_flag: &str) -> &str {
    let model = strip_model_flag_prefix(model_flag);
    // Extract model name after provider prefix (provider/model)
    model.rsplit('/').next().unwrap_or(model)
}

fn normalize_provider_override(provider: &str) -> Option<String> {
    let trimmed = provider.trim().trim_matches('/');
    if trimmed.is_empty() || trimmed.contains('/') {
        return None;
    }
    Some(trimmed.to_string())
}

#[derive(Clone, Copy)]
enum ModelFlagStyle {
    DashMSpace,
    DashMEquals,
    DoubleDashModelSpace,
    DoubleDashModelEquals,
}

fn detect_model_flag_style(model_flag: &str) -> Option<ModelFlagStyle> {
    let s = model_flag.trim_start();
    if s.starts_with("-m=") {
        return Some(ModelFlagStyle::DashMEquals);
    }
    if s.starts_with("--model=") {
        return Some(ModelFlagStyle::DoubleDashModelEquals);
    }
    if s == "-m" || s.starts_with("-m ") || s.starts_with("-m\t") {
        return Some(ModelFlagStyle::DashMSpace);
    }
    if s == "--model" || s.starts_with("--model ") || s.starts_with("--model\t") {
        return Some(ModelFlagStyle::DoubleDashModelSpace);
    }
    None
}

fn format_model_flag(style: ModelFlagStyle, model: &str) -> String {
    match style {
        ModelFlagStyle::DashMSpace => format!("-m {}", model),
        ModelFlagStyle::DashMEquals => format!("-m={}", model),
        ModelFlagStyle::DoubleDashModelSpace => format!("--model {}", model),
        ModelFlagStyle::DoubleDashModelEquals => format!("--model={}", model),
    }
}

/// Resolve the effective model flag considering provider override
///
/// Priority:
/// 1. If provider is specified, construct "{provider}/{model_name}"
/// 2. If model is specified, use it directly
/// 3. Otherwise, use agent's configured model_flag
fn resolve_model_with_provider(
    cli_provider: Option<&str>,
    cli_model: Option<&str>,
    agent_model_flag: Option<&str>,
) -> Option<String> {
    let style = detect_model_flag_style(cli_model.unwrap_or(""))
        .or_else(|| detect_model_flag_style(agent_model_flag.unwrap_or("")))
        .unwrap_or(ModelFlagStyle::DashMSpace);

    let base_model = cli_model
        .map(|m| strip_model_flag_prefix(m).trim())
        .filter(|m| !m.is_empty())
        .or_else(|| {
            agent_model_flag
                .map(|m| strip_model_flag_prefix(m).trim())
                .filter(|m| !m.is_empty())
        })?;

    let provider_override = cli_provider.and_then(normalize_provider_override);
    match (provider_override.as_deref(), cli_model) {
        // Provider + model: construct full model flag
        (Some(provider), Some(model)) => {
            let model_name = extract_model_name(model);
            if model_name.is_empty() {
                return Some(format_model_flag(style, base_model));
            }
            Some(format_model_flag(style, &format!("{}/{}", provider, model_name)))
        }
        // Provider only: use provider with agent's default model
        (Some(provider), None) => {
            let model_name = extract_model_name(base_model);
            if model_name.is_empty() {
                return Some(format_model_flag(style, base_model));
            }
            Some(format_model_flag(style, &format!("{}/{}", provider, model_name)))
        }
        // Model only: normalize to a full model flag (preserve -m/--model style if present)
        (None, Some(_model)) => Some(format_model_flag(style, base_model)),
        // Neither: use agent's configured model (normalized)
        (None, None) => Some(format_model_flag(style, base_model)),
    }
}

/// Run a command with automatic fallback to alternative agents on failure
///
/// This function attempts to run the command with the primary agent first,
/// then falls back to alternative agents based on the fallback configuration
/// if the primary agent fails with specific error types (rate limiting,
/// token exhaustion, auth failures, command not found).
///
/// ## Fault Tolerance Strategy
///
/// 1. Try each agent with up to `max_retries` attempts
/// 2. On retriable errors (rate limit, transient), retry the same agent
/// 3. On fallback errors (auth, command not found), switch to next provider or agent
/// 4. Provider-level fallback: Try different models within the same agent before
///    switching to another agent (configured via provider_fallback in agent_chain)
/// 5. When all agents are exhausted, use exponential backoff and cycle back
///    to the first agent (up to `max_cycles` times)
/// 6. Exponential backoff: base_delay * multiplier^cycle, capped at max_backoff
#[allow(clippy::too_many_arguments)]
fn run_with_fallback(
    role: AgentRole,
    base_label: &str,
    prompt: &str,
    logfile_prefix: &str,
    timer: &mut Timer,
    logger: &Logger,
    colors: &Colors,
    config: &Config,
    registry: &AgentRegistry,
    primary_agent: &str,
) -> io::Result<i32> {
    let fallback_config = registry.fallback_config();
    let fallbacks = registry.available_fallbacks(role);
    if !fallback_config.has_fallbacks(role) {
        logger.info(&format!(
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
            config.developer_model.as_deref(),
            config.developer_provider.as_deref(),
        ),
        AgentRole::Reviewer => (
            config.reviewer_model.as_deref(),
            config.reviewer_provider.as_deref(),
        ),
    };

    // Cycle through all agents with exponential backoff
    for cycle in 0..fallback_config.max_cycles {
        if cycle > 0 {
            let backoff_ms = fallback_config.calculate_backoff(cycle - 1);
            logger.info(&format!(
                "Cycle {}/{}: All agents exhausted, waiting {}ms before retry (exponential backoff)...",
                cycle + 1,
                fallback_config.max_cycles,
                backoff_ms
            ));
            std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
        }

        for (agent_index, agent_name) in agents_to_try.iter().enumerate() {
            let Some(agent_config) = registry.get(agent_name) else {
                logger.warn(&format!(
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
            let provider_fallbacks = fallback_config.get_provider_fallbacks(agent_name);
            if !provider_fallbacks.is_empty() {
                logger.info(&format!(
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
                        logger.warn(&warning);
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
                        AgentRole::Developer => config.developer_cmd.clone().unwrap_or_else(|| {
                            agent_config.build_cmd_with_model(true, true, true, model_ref)
                        }),
                        AgentRole::Reviewer => config.reviewer_cmd.clone().unwrap_or_else(|| {
                            agent_config.build_cmd_with_model(true, true, false, model_ref)
                        }),
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
                        logger.info(&format!(
                            "Retry {}/{} for {}{}...",
                            retry, fallback_config.max_retries, agent_name, model_suffix,
                        ));
                    }

                    let result = run_with_prompt(
                        &label,
                        &cmd_str,
                        prompt,
                        &logfile,
                        parser_type,
                        timer,
                        logger,
                        colors,
                        config,
                    )?;

                    if result.exit_code == 0 {
                        return Ok(0);
                    }

                    last_exit_code = result.exit_code;

                    // Classify the error
                    let error_kind = AgentErrorKind::classify(result.exit_code, &result.stderr);

                    logger.warn(&format!(
                        "Agent '{}'{} failed: {} (exit code {})",
                        agent_name,
                        model_suffix,
                        error_kind.description(),
                        result.exit_code
                    ));

                    // Provide provider-specific auth advice for auth failures
                    if matches!(error_kind, AgentErrorKind::AuthFailure) {
                        logger.info(&auth_failure_advice(model_ref));
                    } else {
                        logger.info(error_kind.recovery_advice());
                    }

                    // Provide installation guidance for command not found errors
                    if error_kind.is_command_not_found() {
                        let binary = cmd_str.split_whitespace().next().unwrap_or(agent_name);
                        let guidance = crate::platform::InstallGuidance::for_binary(binary);
                        logger.info(&guidance.format());
                    }

                    // Provide network-specific guidance
                    if error_kind.is_network_error() {
                        logger.info(
                            "Tip: Check your internet connection, firewall, or VPN settings.",
                        );
                    }

                    // Provide context reduction hint for memory-related errors
                    if error_kind.suggests_smaller_context() {
                        logger.info("Tip: Try reducing context size with RALPH_DEVELOPER_CONTEXT=0 or RALPH_REVIEWER_CONTEXT=0");
                    }

                    // Check for unrecoverable errors - abort immediately
                    if error_kind.is_unrecoverable() {
                        logger.error("Unrecoverable error - cannot continue pipeline");
                        return Ok(last_exit_code);
                    }

                    // Use error-specific wait time for retries
                    let suggested_wait = error_kind.suggested_wait_ms();
                    let actual_wait = if suggested_wait > 0 {
                        suggested_wait
                    } else {
                        fallback_config.retry_delay_ms
                    };

                    // Decide whether to retry, try next provider, or try next agent
                    if error_kind.should_retry() && retry + 1 < fallback_config.max_retries {
                        logger.info(&format!("Waiting {}ms before retry...", actual_wait));
                        std::thread::sleep(std::time::Duration::from_millis(actual_wait));
                        continue; // Retry same agent/model
                    }

                    // For rate limits or token exhaustion, try next provider first
                    if (matches!(
                        error_kind,
                        AgentErrorKind::RateLimited | AgentErrorKind::TokenExhausted
                    )) && model_index + 1 < model_flags_to_try.len()
                    {
                        logger.info(&format!(
                            "Trying next provider/model for {}: {}",
                            agent_name,
                            model_flags_to_try[model_index + 1]
                                .as_deref()
                                .unwrap_or("(default)")
                        ));
                        break; // Try next model flag
                    }

                    // Otherwise, move to next agent
                    if error_kind.should_fallback() && agent_index + 1 < agents_to_try.len() {
                        logger.info(&format!(
                            "Switching to fallback agent: {}",
                            agents_to_try[agent_index + 1]
                        ));
                        break; // Try next agent
                    }

                    // For permanent errors, we still continue to next agent in the chain
                    if agent_index + 1 < agents_to_try.len() {
                        logger.info(&format!(
                            "Trying next agent in chain: {}",
                            agents_to_try[agent_index + 1]
                        ));
                    }
                    break;
                }
            }
        }
        // End of this cycle - if we reach here, all agents failed
        // The outer loop will apply exponential backoff and try again
    }

    // All cycles exhausted
    logger.error(&format!(
        "All agents exhausted after {} cycles with exponential backoff",
        fallback_config.max_cycles
    ));
    Ok(last_exit_code)
}

struct AgentPhaseGuard<'a> {
    git_helpers: &'a mut GitHelpers,
    logger: &'a Logger,
    active: bool,
}

impl<'a> AgentPhaseGuard<'a> {
    fn new(git_helpers: &'a mut GitHelpers, logger: &'a Logger) -> Self {
        Self {
            git_helpers,
            logger,
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for AgentPhaseGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        let _ = end_agent_phase();
        disable_git_wrapper(self.git_helpers);
        let _ = uninstall_hooks(self.logger);
        cleanup_generated_files();
    }
}

/// Helper function to print provider information for --list-providers
fn print_provider_info(colors: &Colors, provider: OpenCodeProviderType, agent_alias: &str) {
    let examples = provider.example_models();
    let example_str = if examples.is_empty() {
        String::new()
    } else {
        format!(" (e.g., {})", examples[0])
    };

    println!("{}{}{}", colors.bold(), provider.name(), colors.reset());
    println!("  Prefix: {}{}", provider.prefix(), example_str);
    println!("  Auth: {}", provider.auth_command());
    println!("  Agent: {}", agent_alias);
}

fn main() -> anyhow::Result<()> {
    // Set up Ctrl+C handler for cleanup on unexpected exit
    ctrlc::set_handler(move || {
        eprintln!("\n✋ Interrupted! Cleaning up generated files...");
        cleanup_agent_phase_silent();
        std::process::exit(130); // Standard exit code for SIGINT
    })
    .ok(); // Ignore errors if handler can't be set (e.g., nested handlers)

    let args = Args::parse();
    let colors = Colors::new();
    let mut logger = Logger::new(colors);

    // Load configuration
    let mut config = Config::from_env().with_commit_msg(args.commit_msg);

    // Handle verbosity shorthand flags (--quiet, --full, --debug take precedence)
    let base_verbosity = config.verbosity;
    config.verbosity = if args.quiet {
        crate::config::Verbosity::Quiet
    } else if args.debug {
        crate::config::Verbosity::Debug
    } else if args.full {
        crate::config::Verbosity::Full
    } else if let Some(v) = args.verbosity {
        v.into()
    } else {
        base_verbosity
    };

    // Apply preset (CLI/env preset overrides env-selected agents, but can be overridden by
    // explicit --developer-agent/--reviewer-agent flags below).
    if let Some(preset) = args.preset {
        match preset {
            Preset::Default => {
                // No override; use agent_chain defaults from agents.toml
            }
            Preset::Opencode => {
                config.developer_agent = Some("opencode".to_string());
                config.reviewer_agent = Some("opencode".to_string());
            }
        }
    }

    // Quick mode: 1 developer iteration, 1 review pass (explicit flags override)
    if args.quick {
        if args.developer_iters.is_none() {
            config.developer_iters = 1;
        }
        if args.reviewer_reviews.is_none() {
            config.reviewer_reviews = 1;
        }
    }

    if let Some(iters) = args.developer_iters {
        config.developer_iters = iters;
    }
    if let Some(reviews) = args.reviewer_reviews {
        config.reviewer_reviews = reviews;
    }
    if let Some(agent) = args.developer_agent {
        config.developer_agent = Some(agent);
    }
    if let Some(agent) = args.reviewer_agent {
        config.reviewer_agent = Some(agent);
    }
    if let Some(model) = args.developer_model {
        config.developer_model = Some(model);
    }
    if let Some(model) = args.reviewer_model {
        config.reviewer_model = Some(model);
    }
    if let Some(provider) = args.developer_provider {
        config.developer_provider = Some(provider);
    }
    if let Some(provider) = args.reviewer_provider {
        config.reviewer_provider = Some(provider);
    }
    if let Some(depth) = args.review_depth {
        if let Some(parsed) = ReviewDepth::from_str(&depth) {
            config.review_depth = parsed;
        } else {
            eprintln!(
                "{}{}Warning:{} Unknown review depth '{}'. Using default (standard).",
                colors.bold(),
                colors.yellow(),
                colors.reset(),
                depth
            );
            eprintln!("Valid options: standard, comprehensive, security, incremental");
        }
    }

    // Handle --no-isolation flag (CLI overrides env var)
    if args.no_isolation {
        config.isolation_mode = false;
    }

    // Handle --init-global flag: create global agents.toml if it doesn't exist and exit
    if args.init_global {
        let global_path = global_agents_config_path().ok_or_else(|| {
            anyhow::anyhow!("Cannot determine global config directory (no home directory)")
        })?;

        match AgentsConfigFile::ensure_config_exists(&global_path) {
            Ok(ConfigInitResult::Created) => {
                println!(
                    "{}Created global config: {}{}{}\n",
                    colors.green(),
                    colors.bold(),
                    global_path.display(),
                    colors.reset()
                );
                println!("This config will be loaded for all repositories.");
                println!(
                    "Per-repository configs in .agent/agents.toml will override these settings."
                );
                return Ok(());
            }
            Ok(ConfigInitResult::AlreadyExists) => {
                println!(
                    "{}Global config already exists:{} {}",
                    colors.yellow(),
                    colors.reset(),
                    global_path.display()
                );
                println!("Edit the file to customize, or delete it to regenerate from defaults.");
                return Ok(());
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to create global config file {}: {}",
                    global_path.display(),
                    e
                );
            }
        }
    }

    // Resolve config path relative to repo root (for git worktree support)
    // This ensures .agent/agents.toml is always in the repo/worktree root
    let repo_root_for_config = get_repo_root().ok();
    let agents_config_path = if config.agents_config_path.is_relative() {
        repo_root_for_config
            .as_ref()
            .map(|root| root.join(&config.agents_config_path))
            .unwrap_or_else(|| config.agents_config_path.clone())
    } else {
        config.agents_config_path.clone()
    };

    // Handle --init flag: create agents.toml if it doesn't exist and exit
    if args.init {
        match AgentsConfigFile::ensure_config_exists(&agents_config_path) {
            Ok(ConfigInitResult::Created) => {
                println!(
                    "{}Created {}{}{}\n",
                    colors.green(),
                    colors.bold(),
                    agents_config_path.display(),
                    colors.reset()
                );
                println!("Edit the file to customize agent configurations, then run ralph again.");
                println!("Or run ralph now to use the default settings.");
                return Ok(());
            }
            Ok(ConfigInitResult::AlreadyExists) => {
                println!(
                    "{}Config file already exists:{} {}",
                    colors.yellow(),
                    colors.reset(),
                    agents_config_path.display()
                );
                println!("Edit the file to customize, or delete it to regenerate from defaults.");
                return Ok(());
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to create config file {}: {}",
                    agents_config_path.display(),
                    e
                );
            }
        }
    }

    // Check if agents.toml exists; if not, create it and prompt user
    match AgentsConfigFile::ensure_config_exists(&agents_config_path) {
        Ok(ConfigInitResult::Created) => {
            println!();
            println!(
                "{}{}No agents.toml found - created default configuration:{}",
                colors.bold(),
                colors.yellow(),
                colors.reset()
            );
            println!(
                "  {}{}{}",
                colors.cyan(),
                agents_config_path.display(),
                colors.reset()
            );
            println!();
            println!("{}Options:{}", colors.bold(), colors.reset());
            println!("  1. Edit the file to customize agent settings, then run ralph again");
            println!("  2. Run ralph again now to use the default settings");
            println!();
            return Ok(());
        }
        Ok(ConfigInitResult::AlreadyExists) => {
            // Config exists, continue normally
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to create agents config at {}: {}",
                agents_config_path.display(),
                e
            ));
            // Continue with built-in defaults
        }
    }

    // Initialize agent registry with merged configs (global + local)
    // Priority: built-in defaults < global config < local config
    let (registry, config_sources) = match AgentRegistry::with_merged_configs(&agents_config_path) {
        Ok((r, sources, warnings)) => {
            for warning in warnings {
                logger.warn(&warning);
            }
            // Log which configs were loaded
            if !sources.is_empty() {
                for source in &sources {
                    logger.info(&format!(
                        "Loaded {} agents from {}{}{}",
                        source.agents_loaded,
                        colors.cyan(),
                        source.path.display(),
                        colors.reset()
                    ));
                }
            }
            (r, sources)
        }
        Err(e) => {
            logger.warn(&format!(
                "Failed to load agents config from {}: {}, using defaults",
                agents_config_path.display(),
                e
            ));
            let registry = AgentRegistry::new().map_err(|defaults_err| {
                anyhow::anyhow!(
                    "Failed to load built-in default agents config (examples/agents.toml): {}",
                    defaults_err
                )
            })?;
            (registry, Vec::new())
        }
    };

    // Log if no config files were found (but we still have built-in defaults)
    if config_sources.is_empty() {
        logger.info(&format!(
            "Using built-in agent defaults {}(no agents.toml found){}",
            colors.dim(),
            colors.reset()
        ));
    }

    // agent_chain is the SINGLE SOURCE OF TRUTH for default agent selection.
    // If no agent was explicitly selected via CLI/env/preset, use agent_chain first entry.
    if config.developer_agent.is_none() {
        config.developer_agent = registry
            .fallback_config()
            .get_fallbacks(AgentRole::Developer)
            .first()
            .cloned();
    }
    if config.reviewer_agent.is_none() {
        config.reviewer_agent = registry
            .fallback_config()
            .get_fallbacks(AgentRole::Reviewer)
            .first()
            .cloned();
    }

    // Resolve final agent names - these are required at this point
    let developer_agent = config.developer_agent.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "No developer agent configured.\n\
            Set via --developer-agent, RALPH_DEVELOPER_AGENT env, or agent_chain in agents.toml."
        )
    })?;
    let reviewer_agent = config.reviewer_agent.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "No reviewer agent configured.\n\
            Set via --reviewer-agent, RALPH_REVIEWER_AGENT env, or agent_chain in agents.toml."
        )
    })?;

    if args.list_agents {
        let mut items = registry.list();
        items.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (name, cfg) in items {
            println!(
                "{}\tcmd={}\tparser={}\tcan_commit={}",
                name, cfg.cmd, cfg.json_parser, cfg.can_commit
            );
        }
        return Ok(());
    }

    if args.list_available_agents {
        let mut items = registry.list_available();
        items.sort();
        for name in items {
            println!("{}", name);
        }
        return Ok(());
    }

    // --list-providers: Show OpenCode provider types and configuration
    if args.list_providers {
        println!("{}OpenCode Provider Types{}", colors.bold(), colors.reset());
        println!();
        println!("Ralph includes built-in guidance for major OpenCode provider prefixes (plus a custom fallback).");
        println!("OpenCode may support additional providers; consult OpenCode docs for the full set.");
        println!();

        // Category: OpenCode Gateway
        println!(
            "{}═══ OPENCODE GATEWAY ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::OpenCodeZen,
            "opencode-zen-glm",
        );
        println!();

        // Category: Chinese AI Providers
        println!(
            "{}═══ CHINESE AI PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(&colors, OpenCodeProviderType::ZaiDirect, "opencode-zai-glm");
        print_provider_info(
            &colors,
            OpenCodeProviderType::ZaiCodingPlan,
            "opencode-zai-glm-codingplan",
        );
        print_provider_info(&colors, OpenCodeProviderType::Moonshot, "opencode-moonshot");
        print_provider_info(&colors, OpenCodeProviderType::MiniMax, "opencode-minimax");
        println!();

        // Category: Major Cloud Providers
        println!(
            "{}═══ MAJOR CLOUD PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::Anthropic,
            "opencode-direct-claude",
        );
        print_provider_info(&colors, OpenCodeProviderType::OpenAI, "opencode-openai");
        print_provider_info(&colors, OpenCodeProviderType::Google, "opencode-google");
        print_provider_info(
            &colors,
            OpenCodeProviderType::GoogleVertex,
            "opencode-vertex",
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::AmazonBedrock,
            "opencode-bedrock",
        );
        print_provider_info(&colors, OpenCodeProviderType::AzureOpenAI, "opencode-azure");
        print_provider_info(
            &colors,
            OpenCodeProviderType::GithubCopilot,
            "opencode-copilot",
        );
        println!();

        // Category: Fast Inference Providers
        println!(
            "{}═══ FAST INFERENCE PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(&colors, OpenCodeProviderType::Groq, "opencode-groq");
        print_provider_info(&colors, OpenCodeProviderType::Together, "opencode-together");
        print_provider_info(
            &colors,
            OpenCodeProviderType::Fireworks,
            "opencode-fireworks",
        );
        print_provider_info(&colors, OpenCodeProviderType::Cerebras, "opencode-cerebras");
        print_provider_info(
            &colors,
            OpenCodeProviderType::SambaNova,
            "opencode-sambanova",
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::DeepInfra,
            "opencode-deepinfra",
        );
        println!();

        // Category: Gateway/Aggregator Providers
        println!(
            "{}═══ GATEWAY PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::OpenRouter,
            "opencode-openrouter",
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::Cloudflare,
            "opencode-cloudflare",
        );
        println!();

        // Category: Specialized Providers
        println!(
            "{}═══ SPECIALIZED PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(&colors, OpenCodeProviderType::DeepSeek, "opencode-deepseek");
        print_provider_info(&colors, OpenCodeProviderType::Xai, "opencode-xai");
        print_provider_info(&colors, OpenCodeProviderType::Mistral, "opencode-mistral");
        print_provider_info(&colors, OpenCodeProviderType::Cohere, "opencode-cohere");
        print_provider_info(
            &colors,
            OpenCodeProviderType::Perplexity,
            "opencode-perplexity",
        );
        print_provider_info(&colors, OpenCodeProviderType::AI21, "opencode-ai21");
        print_provider_info(&colors, OpenCodeProviderType::VeniceAI, "opencode-venice");
        println!();

        // Category: Open-Source Model Providers
        println!(
            "{}═══ OPEN-SOURCE MODEL PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::HuggingFace,
            "opencode-huggingface",
        );
        print_provider_info(
            &colors,
            OpenCodeProviderType::Replicate,
            "opencode-replicate",
        );
        println!();

        // Category: Cloud Platform Providers
        println!(
            "{}═══ CLOUD PLATFORM PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(&colors, OpenCodeProviderType::Baseten, "opencode-baseten");
        print_provider_info(&colors, OpenCodeProviderType::Cortecs, "opencode-cortecs");
        print_provider_info(&colors, OpenCodeProviderType::Scaleway, "opencode-scaleway");
        print_provider_info(&colors, OpenCodeProviderType::OVHcloud, "opencode-ovhcloud");
        print_provider_info(&colors, OpenCodeProviderType::IONet, "opencode-ionet");
        print_provider_info(&colors, OpenCodeProviderType::Nebius, "opencode-nebius");
        println!();

        // Category: AI Gateway Providers
        println!(
            "{}═══ AI GATEWAY PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(&colors, OpenCodeProviderType::Vercel, "opencode-vercel");
        print_provider_info(&colors, OpenCodeProviderType::Helicone, "opencode-helicone");
        print_provider_info(&colors, OpenCodeProviderType::ZenMux, "opencode-zenmux");
        println!();

        // Category: Enterprise/Industry Providers
        println!(
            "{}═══ ENTERPRISE PROVIDERS ═══{}",
            colors.bold(),
            colors.reset()
        );
        print_provider_info(&colors, OpenCodeProviderType::SapAICore, "opencode-sap");
        print_provider_info(
            &colors,
            OpenCodeProviderType::AzureCognitiveServices,
            "opencode-azure-cognitive",
        );
        println!();

        // Category: Local Providers
        println!("{}═══ LOCAL PROVIDERS ═══{}", colors.bold(), colors.reset());
        print_provider_info(&colors, OpenCodeProviderType::Ollama, "opencode-ollama");
        print_provider_info(&colors, OpenCodeProviderType::LMStudio, "opencode-lmstudio");
        print_provider_info(
            &colors,
            OpenCodeProviderType::OllamaCloud,
            "opencode-ollama-cloud",
        );
        print_provider_info(&colors, OpenCodeProviderType::LlamaCpp, "opencode-llamacpp");
        println!();

        // Category: Custom
        println!("{}═══ CUSTOM ═══{}", colors.bold(), colors.reset());
        print_provider_info(&colors, OpenCodeProviderType::Custom, "(custom)");
        println!();

        // Important notes
        println!("{}═══ IMPORTANT NOTES ═══{}", colors.bold(), colors.reset());
        println!("• OpenCode Zen (opencode/*) and Z.AI Direct (zai/* or zhipuai/*) are SEPARATE endpoints!");
        println!("  - opencode/* routes through OpenCode's Zen gateway at opencode.ai");
        println!("  - zai/* or zhipuai/* connects directly to Z.AI's API at api.z.ai");
        println!("  - Z.AI Coding Plan is an auth tier; model prefix remains zai/* or zhipuai/*");
        println!("• Cloud providers (Vertex, Bedrock, Azure, SAP) require additional configuration");
        println!("• Local providers (Ollama, LM Studio, llama.cpp) run on your hardware - no API key needed");
        println!("• Use clear naming: opencode-zen-*, opencode-zai-*, opencode-direct-* aliases");
        println!();

        return Ok(());
    }

    // --diagnose: Output comprehensive diagnostic information
    if args.diagnose {
        println!(
            "{}=== Ralph Diagnostic Report ==={}",
            colors.bold(),
            colors.reset()
        );
        println!();

        // System Information
        println!("{}System:{}", colors.bold(), colors.reset());
        println!("  OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
        if let Ok(cwd) = std::env::current_dir() {
            println!("  Working directory: {}", cwd.display());
        }
        if let Ok(shell) = std::env::var("SHELL") {
            println!("  Shell: {}", shell);
        }
        println!();

        // Git Information
        println!("{}Git:{}", colors.bold(), colors.reset());
        if let Ok(output) = Command::new("git").args(["--version"]).output() {
            if let Ok(version) = std::str::from_utf8(&output.stdout) {
                println!("  Version: {}", version.trim());
            }
        }
        let is_repo = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        println!("  In git repo: {}", if is_repo { "yes" } else { "no" });
        if is_repo {
            if let Ok(output) = Command::new("git")
                .args(["branch", "--show-current"])
                .output()
            {
                if let Ok(branch) = std::str::from_utf8(&output.stdout) {
                    println!("  Current branch: {}", branch.trim());
                }
            }
            // Check for uncommitted changes
            if let Ok(output) = Command::new("git").args(["status", "--porcelain"]).output() {
                if let Ok(status) = std::str::from_utf8(&output.stdout) {
                    let changes = status.lines().count();
                    println!("  Uncommitted changes: {}", changes);
                }
            }
        }
        println!();

        // Configuration
        println!("{}Configuration:{}", colors.bold(), colors.reset());
        println!("  Config file: {}", agents_config_path.display());
        println!("  Config exists: {}", agents_config_path.exists());
        println!(
            "  Review depth: {:?} ({})",
            config.review_depth,
            config.review_depth.description()
        );
        if let Some(global_path) = agents::global_agents_config_path() {
            println!("  Global config: {}", global_path.display());
            println!("  Global exists: {}", global_path.exists());
        }
        if !config_sources.is_empty() {
            println!("  Loaded sources:");
            for src in &config_sources {
                println!(
                    "    - {} ({} agents)",
                    src.path.display(),
                    src.agents_loaded
                );
            }
        }
        println!();

        // Agent Chain Configuration
        println!("{}Agent Chain:{}", colors.bold(), colors.reset());
        let fallback = registry.fallback_config();
        let dev_chain = fallback.get_fallbacks(agents::AgentRole::Developer);
        let rev_chain = fallback.get_fallbacks(agents::AgentRole::Reviewer);
        println!("  Developer chain: {:?}", dev_chain);
        println!("  Reviewer chain: {:?}", rev_chain);
        println!("  Max retries: {}", fallback.max_retries);
        println!("  Retry delay: {}ms", fallback.retry_delay_ms);
        println!();

        // Agent Availability
        println!("{}Agent Availability:{}", colors.bold(), colors.reset());
        let all_agents = registry.list();
        let mut sorted_agents: Vec<_> = all_agents.into_iter().collect();
        sorted_agents.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (name, cfg) in sorted_agents {
            let available = registry.is_agent_available(name);
            let status_color = if available {
                colors.green()
            } else {
                colors.red()
            };
            let status_icon = if available { "✓" } else { "✗" };
            println!(
                "  {}{}{} {} (parser: {}, cmd: {})",
                status_color,
                status_icon,
                colors.reset(),
                name,
                cfg.json_parser,
                cfg.cmd.split_whitespace().next().unwrap_or(&cfg.cmd)
            );
        }
        println!();

        // PROMPT.md Status
        println!("{}PROMPT.md:{}", colors.bold(), colors.reset());
        let prompt_path = Path::new("PROMPT.md");
        if prompt_path.exists() {
            if let Ok(content) = fs::read_to_string(prompt_path) {
                println!("  Exists: yes");
                println!("  Size: {} bytes", content.len());
                println!("  Lines: {}", content.lines().count());
                let has_goal = content.contains("## Goal") || content.contains("# Goal");
                let has_acceptance =
                    content.contains("## Acceptance") || content.contains("Acceptance Criteria");
                println!(
                    "  Has Goal section: {}",
                    if has_goal { "yes" } else { "no" }
                );
                println!(
                    "  Has Acceptance section: {}",
                    if has_acceptance { "yes" } else { "no" }
                );
            }
        } else {
            println!("  Exists: no");
        }
        println!();

        // Checkpoint Status
        println!("{}Checkpoint:{}", colors.bold(), colors.reset());
        let checkpoint_path = Path::new(".agent/checkpoint.json");
        if checkpoint_path.exists() {
            println!("  Exists: yes");
            if let Ok(Some(cp)) = utils::load_checkpoint() {
                println!("  Phase: {:?}", cp.phase);
                println!("  Developer agent: {}", cp.developer_agent);
                println!("  Reviewer agent: {}", cp.reviewer_agent);
                println!(
                    "  Iterations: {}/{} dev, {}/{} review",
                    cp.iteration, cp.total_iterations, cp.reviewer_pass, cp.total_reviewer_passes
                );
            }
        } else {
            println!("  Exists: no (no interrupted run to resume)");
        }
        println!();

        // Language Detection
        println!("{}Project Stack:{}", colors.bold(), colors.reset());
        if let Ok(cwd) = std::env::current_dir() {
            match language_detector::detect_stack(&cwd) {
                Ok(stack) => {
                    println!("  Primary language: {}", stack.primary_language);
                    if !stack.secondary_languages.is_empty() {
                        println!("  Secondary languages: {:?}", stack.secondary_languages);
                    }
                    if !stack.frameworks.is_empty() {
                        println!("  Frameworks: {:?}", stack.frameworks);
                    }
                    if let Some(pm) = &stack.package_manager {
                        println!("  Package manager: {}", pm);
                    }
                    if let Some(tf) = &stack.test_framework {
                        println!("  Test framework: {}", tf);
                    }

                    // Show language type indicators (useful for debugging language detection)
                    let language_types: Vec<&str> = [
                        if stack.is_rust() { Some("Rust") } else { None },
                        if stack.is_python() {
                            Some("Python")
                        } else {
                            None
                        },
                        if stack.is_javascript_or_typescript() {
                            Some("JS/TS")
                        } else {
                            None
                        },
                        if stack.is_go() { Some("Go") } else { None },
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    if !language_types.is_empty() {
                        println!("  Language flags: {}", language_types.join(", "));
                    }

                    // Show review guidelines summary
                    let guidelines = review_guidelines::ReviewGuidelines::for_stack(&stack);
                    println!("  Review checks: {} total", guidelines.total_checks());

                    // Show severity breakdown from get_all_checks
                    let all_checks = guidelines.get_all_checks();
                    let critical_count = all_checks
                        .iter()
                        .filter(|c| matches!(c.severity, CheckSeverity::Critical))
                        .count();
                    let high_count = all_checks
                        .iter()
                        .filter(|c| matches!(c.severity, CheckSeverity::High))
                        .count();
                    if critical_count > 0 || high_count > 0 {
                        println!(
                            "  Check severities: {} critical, {} high",
                            critical_count, high_count
                        );
                    }
                }
                Err(e) => {
                    println!("  Detection failed: {}", e);
                }
            }
        }
        println!();

        // Recent errors (if log exists)
        let log_path = Path::new(".agent/logs/pipeline.log");
        if log_path.exists() {
            println!(
                "{}Recent Log Entries (last 10):{}",
                colors.bold(),
                colors.reset()
            );
            if let Ok(content) = fs::read_to_string(log_path) {
                let lines: Vec<&str> = content.lines().collect();
                let start = lines.len().saturating_sub(10);
                for line in &lines[start..] {
                    println!("  {}", line);
                }
            }
        }

        println!();
        println!(
            "{}Copy this output for bug reports: https://github.com/anthropics/ralph/issues{}",
            colors.dim(),
            colors.reset()
        );

        return Ok(());
    }

    // Validate that agent chains are configured
    if let Err(msg) = registry.validate_agent_chains() {
        eprintln!();
        eprintln!(
            "{}{}Error:{} {}",
            colors.bold(),
            colors.red(),
            colors.reset(),
            msg
        );
        eprintln!();
        eprintln!(
            "{}Hint:{} Run 'ralph --init' to create a default agents.toml configuration.",
            colors.yellow(),
            colors.reset()
        );
        eprintln!();
        std::process::exit(1);
    }

    // === Plumbing Commands ===
    // These are low-level operations that don't need the full pipeline

    // --show-commit-msg: Just read and display the commit message file
    if args.show_commit_msg {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        match read_commit_message_file() {
            Ok(msg) => {
                println!("{}", msg);
                return Ok(());
            }
            Err(e) => {
                anyhow::bail!("Failed to read commit message: {}", e);
            }
        }
    }

    // --apply-commit: Stage all changes and commit using existing commit message
    if args.apply_commit {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        let commit_msg = read_commit_message_file()?;

        logger.info("Staging all changes...");
        git_add_all()?;

        // Show what we're committing
        let status_output = Command::new("git").args(["status", "--short"]).output()?;
        let status_str = std::str::from_utf8(&status_output.stdout).unwrap_or("");
        if !status_str.is_empty() {
            println!("{}Changes to commit:{}", colors.bold(), colors.reset());
            for line in status_str.lines().take(20) {
                println!("  {}{}{}", colors.dim(), line, colors.reset());
            }
            println!();
        }

        logger.info(&format!(
            "Commit message: {}{}{}",
            colors.cyan(),
            commit_msg,
            colors.reset()
        ));

        logger.info("Creating commit...");
        if git_commit(&commit_msg)? {
            logger.success("Commit created successfully");
            // Clean up the commit message file
            if let Err(err) = delete_commit_message_file() {
                logger.warn(&format!("Failed to delete commit-message.txt: {}", err));
            }
        } else {
            logger.warn("Nothing to commit (working tree clean)");
        }

        return Ok(());
    }

    // Validate agent commands exist (early validation with good error messages)
    let _developer_cmd = if let Some(cmd) = config.developer_cmd.clone() {
        cmd
    } else {
        registry.developer_cmd(&developer_agent).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown developer agent '{}'. Use --list-agents or define it in {}.",
                developer_agent,
                agents_config_path.display()
            )
        })?
    };
    let _reviewer_cmd = if let Some(cmd) = config.reviewer_cmd.clone() {
        cmd
    } else {
        registry.reviewer_cmd(&reviewer_agent).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown reviewer agent '{}'. Use --list-agents or define it in {}.",
                reviewer_agent,
                agents_config_path.display()
            )
        })?
    };

    // Enforce workflow-capable agents unless the user provided a custom command override.
    // Agents with can_commit=false are chat-only / non-tool agents and will stall Ralph.
    if config.developer_cmd.is_none() {
        if let Some(cfg) = registry.get(&developer_agent) {
            if !cfg.can_commit {
                anyhow::bail!(
                    "Developer agent '{}' has can_commit=false and cannot run Ralph's workflow.\n\
                    Fix: choose a different agent (see --list-agents) or set can_commit=true in {}.",
                    developer_agent,
                    agents_config_path.display()
                );
            }
        }
    }
    if config.reviewer_cmd.is_none() {
        if let Some(cfg) = registry.get(&reviewer_agent) {
            if !cfg.can_commit {
                anyhow::bail!(
                    "Reviewer agent '{}' has can_commit=false and cannot run Ralph's workflow.\n\
                    Fix: choose a different agent (see --list-agents) or set can_commit=true in {}.",
                    reviewer_agent,
                    agents_config_path.display()
                );
            }
        }
    }

    // Require git repo
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;
    ensure_files(config.isolation_mode)?;

    // Reset context for isolation mode (default) - delete NOTES.md and ISSUES.md
    // to prevent context contamination from previous runs
    if config.isolation_mode {
        reset_context_for_isolation(&logger)?;
    }

    logger = logger.with_log_file(".agent/logs/pipeline.log");

    // --dry-run: Validate setup without running agents
    if args.dry_run {
        logger.header("DRY RUN: Validation", |c| c.cyan());

        // Validate PROMPT.md using the utility function
        let validation = utils::validate_prompt_md(config.strict_validation);

        // Report errors first
        for err in &validation.errors {
            logger.error(err);
        }

        // Report warnings
        for warn in &validation.warnings {
            logger.warn(&format!("{} (recommended)", warn));
        }

        // Bail if validation failed
        if !validation.is_valid() {
            anyhow::bail!("Dry run failed: PROMPT.md validation errors");
        }

        // Report successes
        if validation.has_goal {
            logger.success("PROMPT.md has Goal section");
        }
        if validation.has_acceptance {
            logger.success("PROMPT.md has acceptance checks section");
        }

        logger.success(&format!("Developer agent: {}", developer_agent));
        logger.success(&format!("Reviewer agent: {}", reviewer_agent));
        logger.success(&format!("Developer iterations: {}", config.developer_iters));
        logger.success(&format!("Reviewer passes: {}", config.reviewer_reviews));

        // Check for checkpoint
        if checkpoint_exists() {
            logger.info("Checkpoint found - can resume with --resume");
            if let Ok(Some(cp)) = load_checkpoint() {
                logger.info(&format!("  Phase: {}", cp.phase));
                logger.info(&format!("  Progress: {}", cp.description()));
                logger.info(&format!("  Saved at: {}", cp.timestamp));
            }
        }

        // Detect stack - use the convenience function for simple display
        logger.success(&format!(
            "Detected stack: {}",
            detect_stack_summary(&repo_root)
        ));

        logger.success("Dry run validation complete");
        return Ok(());
    }

    // --resume: Resume from last checkpoint
    let resume_checkpoint = if args.resume {
        match load_checkpoint() {
            Ok(Some(checkpoint)) => {
                logger.header("RESUME: Loading Checkpoint", |c| c.yellow());
                logger.info(&format!("Resuming from: {}", checkpoint.description()));
                logger.info(&format!("Checkpoint saved at: {}", checkpoint.timestamp));

                // Verify agents match
                if checkpoint.developer_agent != developer_agent {
                    logger.warn(&format!(
                        "Developer agent changed: {} -> {}",
                        checkpoint.developer_agent, developer_agent
                    ));
                }
                if checkpoint.reviewer_agent != reviewer_agent {
                    logger.warn(&format!(
                        "Reviewer agent changed: {} -> {}",
                        checkpoint.reviewer_agent, reviewer_agent
                    ));
                }

                Some(checkpoint)
            }
            Ok(None) => {
                logger.warn("No checkpoint found. Starting fresh pipeline...");
                None
            }
            Err(e) => {
                logger.warn(&format!(
                    "Failed to load checkpoint (starting fresh): {}",
                    e
                ));
                None
            }
        }
    } else {
        None
    };

    // --generate-commit-msg: Run only the commit message generation phase
    if args.generate_commit_msg {
        let reviewer_context = ContextLevel::from(config.reviewer_context);
        let mut timer = Timer::new();

        logger.info("Generating commit message...");

        let commit_msg_prompt = prompt_for_agent(
            Role::Reviewer,
            Action::GenerateCommitMessage,
            reviewer_context,
            None,
            None,
            None, // No guidelines needed for commit message generation
        );

        let _ = run_with_fallback(
            AgentRole::Reviewer,
            "generate commit msg",
            &commit_msg_prompt,
            ".agent/logs/commit_message",
            &mut timer,
            &logger,
            &colors,
            &config,
            &registry,
            &reviewer_agent,
        );

        // Verify and display the generated message
        match read_commit_message_file() {
            Ok(msg) => {
                logger.success("Commit message generated:");
                println!();
                println!("{}{}{}", colors.cyan(), msg, colors.reset());
                println!();
                logger.info("Message saved to .agent/commit-message.txt");
                logger.info("Run 'ralph --apply-commit' to create the commit");
            }
            Err(e) => {
                logger.error(&format!("Failed to generate commit message: {}", e));
                anyhow::bail!("Commit message generation failed");
            }
        }

        return Ok(());
    }

    // Set up git helpers
    let mut git_helpers = GitHelpers::new();

    cleanup_orphaned_marker(&logger)?;

    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &logger);

    let mut timer = Timer::new();
    let mut stats = Stats::new();

    // Welcome banner
    println!();
    println!(
        "{}{}╭────────────────────────────────────────────────────────────╮{}",
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!(
        "{}{}│{}  {}{}🤖 Ralph{} {}─ PROMPT-driven agent orchestrator{}              {}{}│{}",
        colors.bold(),
        colors.cyan(),
        colors.reset(),
        colors.bold(),
        colors.white(),
        colors.reset(),
        colors.dim(),
        colors.reset(),
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!(
        "{}{}│{}  {}{} × {} pipeline for autonomous development{}                 {}{}│{}",
        colors.bold(),
        colors.cyan(),
        colors.reset(),
        colors.dim(),
        developer_agent,
        reviewer_agent,
        colors.reset(),
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!(
        "{}{}╰────────────────────────────────────────────────────────────╯{}",
        colors.bold(),
        colors.cyan(),
        colors.reset()
    );
    println!();
    logger.info(&format!(
        "Working directory: {}{}{}",
        colors.cyan(),
        repo_root.display(),
        colors.reset()
    ));
    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        config.commit_msg,
        colors.reset()
    ));

    // Detect project stack for language-specific review guidance (if enabled)
    let project_stack: Option<ProjectStack> = if config.auto_detect_stack {
        match detect_stack(&repo_root) {
            Ok(stack) => {
                logger.info(&format!(
                    "Detected stack: {}{}{}",
                    colors.cyan(),
                    stack.summary(),
                    colors.reset()
                ));
                Some(stack)
            }
            Err(e) => {
                logger.warn(&format!("Could not detect project stack: {}", e));
                None
            }
        }
    } else {
        None
    };

    // Generate language-specific review guidelines if stack was detected
    let review_guidelines: Option<ReviewGuidelines> =
        project_stack.as_ref().map(ReviewGuidelines::for_stack);

    if let Some(ref guidelines) = review_guidelines {
        logger.info(&format!(
            "Review guidelines: {}{}{}",
            colors.dim(),
            guidelines.summary(),
            colors.reset()
        ));
    }

    println!();

    let phase_rank = |p: PipelinePhase| -> u8 {
        match p {
            PipelinePhase::Planning => 0,
            PipelinePhase::Development => 1,
            PipelinePhase::Review => 2,
            PipelinePhase::Fix => 3,
            PipelinePhase::ReviewAgain => 4,
            PipelinePhase::CommitMessage => 5,
            PipelinePhase::FinalValidation => 6,
            PipelinePhase::Complete => 7,
        }
    };

    let resume_phase = resume_checkpoint.as_ref().map(|c| c.phase);
    let resume_rank = resume_phase.map(phase_rank);
    let should_run_from = |phase: PipelinePhase| -> bool {
        match resume_rank {
            None => true,
            Some(rank) => phase_rank(phase) >= rank,
        }
    };

    // Phase 1: Development (PROMPT → PLAN → Execute → Delete PLAN, repeated X times)
    logger.header("PHASE 1: Development", |c| c.blue());
    if resume_rank.is_some_and(|rank| rank >= phase_rank(PipelinePhase::Review)) {
        logger.info("Skipping development phase (checkpoint indicates it already completed)");
    } else {
        logger.info(&format!(
            "Running {}{}{} developer iterations ({})",
            colors.bold(),
            config.developer_iters,
            colors.reset(),
            developer_agent
        ));

        let mut prev_snap = git_snapshot()?;
        let developer_context = ContextLevel::from(config.developer_context);

        let start_iter = match resume_phase {
            Some(PipelinePhase::Planning | PipelinePhase::Development) => resume_checkpoint
                .as_ref()
                .map(|c| c.iteration)
                .unwrap_or(1)
                .clamp(1, config.developer_iters),
            _ => 1,
        };

        for i in start_iter..=config.developer_iters {
            logger.subheader(&format!("Iteration {} of {}", i, config.developer_iters));
            print_progress(i, config.developer_iters, "Overall");

            let resuming_into_development =
                args.resume && resume_phase == Some(PipelinePhase::Development) && i == start_iter;

            // Step 1: Create PLAN from PROMPT (skip if resuming into development)
            if !resuming_into_development {
                // Save checkpoint at start of planning phase (if enabled)
                if config.checkpoint_enabled {
                    let _ = save_checkpoint(&PipelineCheckpoint::new(
                        PipelinePhase::Planning,
                        i,
                        config.developer_iters,
                        0,
                        config.reviewer_reviews,
                        &developer_agent,
                        &reviewer_agent,
                    ));
                }

                logger.info("Creating plan from PROMPT.md...");
                update_status("Starting planning phase", config.isolation_mode)?;

                let plan_prompt = prompt_for_agent(
                    Role::Developer,
                    Action::Plan,
                    ContextLevel::Normal,
                    None,
                    None,
                    None, // No guidelines needed for planning
                );

                let _ = run_with_fallback(
                    AgentRole::Developer,
                    &format!("planning #{}", i),
                    &plan_prompt,
                    &format!(".agent/logs/planning_{}", i),
                    &mut timer,
                    &logger,
                    &colors,
                    &config,
                    &registry,
                    &developer_agent,
                );
            } else {
                logger.info("Resuming at development step; skipping plan generation");
            }

            // Verify PLAN.md was created (required)
            let plan_path = std::path::Path::new(".agent/PLAN.md");
            let mut plan_ok = plan_path
                .exists()
                .then(|| fs::read_to_string(plan_path).ok())
                .flatten()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);

            if !plan_ok && resuming_into_development {
                logger.warn("Missing .agent/PLAN.md; rerunning plan generation to recover");

                if config.checkpoint_enabled {
                    let _ = save_checkpoint(&PipelineCheckpoint::new(
                        PipelinePhase::Planning,
                        i,
                        config.developer_iters,
                        0,
                        config.reviewer_reviews,
                        &developer_agent,
                        &reviewer_agent,
                    ));
                }

                let plan_prompt = prompt_for_agent(
                    Role::Developer,
                    Action::Plan,
                    ContextLevel::Normal,
                    None,
                    None,
                    None,
                );

                let _ = run_with_fallback(
                    AgentRole::Developer,
                    &format!("planning #{}", i),
                    &plan_prompt,
                    &format!(".agent/logs/planning_{}", i),
                    &mut timer,
                    &logger,
                    &colors,
                    &config,
                    &registry,
                    &developer_agent,
                );

                plan_ok = plan_path
                    .exists()
                    .then(|| fs::read_to_string(plan_path).ok())
                    .flatten()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
            }

            if !plan_ok {
                anyhow::bail!("Planning phase did not create a non-empty .agent/PLAN.md");
            }
            logger.success("PLAN.md created");

            // Save checkpoint at start of development phase (if enabled)
            if config.checkpoint_enabled {
                let _ = save_checkpoint(&PipelineCheckpoint::new(
                    PipelinePhase::Development,
                    i,
                    config.developer_iters,
                    0,
                    config.reviewer_reviews,
                    &developer_agent,
                    &reviewer_agent,
                ));
            }

            // Step 2: Execute the PLAN
            logger.info("Executing plan...");
            update_status("Starting development iteration", config.isolation_mode)?;

            let prompt = prompt_for_agent(
                Role::Developer,
                Action::Iterate,
                developer_context,
                Some(i),
                Some(config.developer_iters),
                None, // No guidelines needed for development iteration
            );

            let exit_code = run_with_fallback(
                AgentRole::Developer,
                &format!("run #{}", i),
                &prompt,
                &format!(".agent/logs/developer_{}", i),
                &mut timer,
                &logger,
                &colors,
                &config,
                &registry,
                &developer_agent,
            )?;

            if exit_code != 0 {
                logger.error(&format!(
                    "Iteration {} encountered an error but continuing",
                    i
                ));
            }

            stats.developer_runs_completed += 1;
            update_status("Completed progress step", config.isolation_mode)?;

            let snap = git_snapshot()?;
            if snap == prev_snap {
                logger.warn("No git-status change detected");
            } else {
                logger.success("Repository modified");
                stats.changes_detected += 1;
            }
            prev_snap = snap;

            // Run fast check if configured
            if let Some(ref fast_cmd) = config.fast_check_cmd {
                logger.info(&format!(
                    "Running fast check: {}{}{}",
                    colors.dim(),
                    fast_cmd,
                    colors.reset()
                ));

                let _fast_logfile = format!(".agent/logs/fast_check_{}.log", i);
                let status = Command::new("sh").args(["-c", fast_cmd]).status()?;

                if status.success() {
                    logger.success("Fast check passed");
                } else {
                    logger.warn("Fast check had issues (non-blocking)");
                }
            }

            // Step 3: Delete the PLAN
            logger.info("Deleting PLAN.md...");
            if let Err(err) = delete_plan_file() {
                logger.warn(&format!("Failed to delete PLAN.md: {}", err));
            }
            logger.success("PLAN.md deleted");
        }
    }

    update_status("In progress.", config.isolation_mode)?;

    // Phase 2: Reviewer review/fix cycle
    logger.header("PHASE 2: Review & Fix", |c| c.magenta());

    // Clean context for reviewer if using minimal context
    let reviewer_context = ContextLevel::from(config.reviewer_context);
    let run_any_reviewer_phase = should_run_from(PipelinePhase::Review)
        || should_run_from(PipelinePhase::Fix)
        || should_run_from(PipelinePhase::ReviewAgain)
        || should_run_from(PipelinePhase::CommitMessage);
    if reviewer_context == ContextLevel::Minimal && run_any_reviewer_phase {
        clean_context_for_reviewer(&logger, config.isolation_mode)?;
    }

    if should_run_from(PipelinePhase::Review) {
        logger.info(&format!(
            "Running review → fix → review×{}{}{} cycle ({})",
            colors.bold(),
            config.reviewer_reviews,
            colors.reset(),
            reviewer_agent
        ));

        // Save checkpoint at start of review phase (if enabled)
        if config.checkpoint_enabled {
            let _ = save_checkpoint(&PipelineCheckpoint::new(
                PipelinePhase::Review,
                config.developer_iters,
                config.developer_iters,
                0,
                config.reviewer_reviews,
                &developer_agent,
                &reviewer_agent,
            ));
        }

        // Initial review - select prompt based on review_depth configuration
        logger.subheader("Initial Review");
        update_status("Starting code review", config.isolation_mode)?;

        let prompt = match config.review_depth {
            ReviewDepth::Security => {
                if let Some(ref guidelines) = review_guidelines {
                    logger.info("Using security-focused review with language-specific checks");
                    prompt_security_focused_review(reviewer_context, guidelines)
                } else {
                    logger.info("Using security-focused review");
                    prompt_security_focused_review(reviewer_context, &ReviewGuidelines::default())
                }
            }
            ReviewDepth::Incremental => {
                logger.info("Using incremental review (changed files only)");
                prompt_incremental_review(reviewer_context)
            }
            ReviewDepth::Comprehensive => {
                if let Some(ref guidelines) = review_guidelines {
                    logger.info("Using comprehensive review with language-specific checks");
                    prompt_comprehensive_review(reviewer_context, guidelines)
                } else {
                    logger.info("Using comprehensive review");
                    prompt_comprehensive_review(reviewer_context, &ReviewGuidelines::default())
                }
            }
            ReviewDepth::Standard => {
                // Standard review: use comprehensive if guidelines available, else basic
                if let Some(ref guidelines) = review_guidelines {
                    logger.info("Using comprehensive review with language-specific checks");
                    prompt_comprehensive_review(reviewer_context, guidelines)
                } else {
                    // Fall back to standard review prompt
                    prompt_for_agent(
                        Role::Reviewer,
                        Action::Review,
                        reviewer_context,
                        None,
                        None,
                        None,
                    )
                }
            }
        };
        let _ = run_with_fallback(
            AgentRole::Reviewer,
            "review (comprehensive)",
            &prompt,
            ".agent/logs/reviewer_review_1",
            &mut timer,
            &logger,
            &colors,
            &config,
            &registry,
            &reviewer_agent,
        );
        stats.reviewer_runs_completed += 1;
    } else if run_any_reviewer_phase {
        logger.info("Skipping initial review (resuming from a later checkpoint phase)");
    }

    if should_run_from(PipelinePhase::Fix) {
        // Save checkpoint at start of fix phase (if enabled)
        if config.checkpoint_enabled {
            let _ = save_checkpoint(&PipelineCheckpoint::new(
                PipelinePhase::Fix,
                config.developer_iters,
                config.developer_iters,
                0,
                config.reviewer_reviews,
                &developer_agent,
                &reviewer_agent,
            ));
        }

        // Applying fixes
        logger.subheader("Applying Fixes");
        update_status("Applying fixes", config.isolation_mode)?;

        let prompt = prompt_for_agent(
            Role::Reviewer,
            Action::Fix,
            reviewer_context,
            None,
            None,
            None, // No guidelines needed for fix phase
        );
        let _ = run_with_fallback(
            AgentRole::Reviewer,
            "fix",
            &prompt,
            ".agent/logs/reviewer_fix",
            &mut timer,
            &logger,
            &colors,
            &config,
            &registry,
            &reviewer_agent,
        );
        stats.reviewer_runs_completed += 1;
    } else if run_any_reviewer_phase {
        logger.info("Skipping fix phase (resuming from a later checkpoint phase)");
    }

    if should_run_from(PipelinePhase::ReviewAgain) {
        let start_pass = if resume_phase == Some(PipelinePhase::ReviewAgain) {
            resume_checkpoint
                .as_ref()
                .map(|c| c.reviewer_pass)
                .unwrap_or(1)
                .clamp(1, config.reviewer_reviews.max(1))
        } else {
            1
        };

        // Review-Fix iterations (replaces verification loop)
        for j in start_pass..=config.reviewer_reviews {
            // Save checkpoint at start of each iteration
            if config.checkpoint_enabled {
                let _ = save_checkpoint(&PipelineCheckpoint::new(
                    PipelinePhase::ReviewAgain,
                    config.developer_iters,
                    config.developer_iters,
                    j,
                    config.reviewer_reviews,
                    &developer_agent,
                    &reviewer_agent,
                ));
            }

            logger.subheader(&format!(
                "Review-Fix Iteration {} of {}",
                j, config.reviewer_reviews
            ));
            print_progress(j, config.reviewer_reviews, "Review-Fix passes");

            // REVIEW PASS (full review, creates detailed ISSUES.md)
            update_status("Re-reviewing", config.isolation_mode)?;
            let review_prompt = prompt_for_agent(
                Role::Reviewer,
                Action::Review,
                reviewer_context,
                None,
                None,
                review_guidelines.as_ref(),
            );
            let _ = run_with_fallback(
                AgentRole::Reviewer,
                &format!("review #{}", j + 1),
                &review_prompt,
                &format!(".agent/logs/reviewer_review_{}", j + 1),
                &mut timer,
                &logger,
                &colors,
                &config,
                &registry,
                &reviewer_agent,
            );
            stats.reviewer_runs_completed += 1;

            // EARLY EXIT CHECK: If review found no issues, stop
            if let Ok(metrics) = ReviewMetrics::from_issues_file() {
                if metrics.no_issues_declared && metrics.total_issues == 0 {
                    logger.success(&format!(
                        "No issues found after iteration {} - stopping early",
                        j
                    ));
                    break;
                }
            }

            // FIX PASS (addresses issues found in review)
            update_status("Applying fixes", config.isolation_mode)?;
            let fix_prompt = prompt_for_agent(
                Role::Reviewer,
                Action::Fix,
                reviewer_context,
                None,
                None,
                None,
            );
            let _ = run_with_fallback(
                AgentRole::Reviewer,
                &format!("fix #{}", j + 1),
                &fix_prompt,
                &format!(".agent/logs/reviewer_fix_{}", j + 1),
                &mut timer,
                &logger,
                &colors,
                &config,
                &registry,
                &reviewer_agent,
            );
            stats.reviewer_runs_completed += 1;
        }
    } else if run_any_reviewer_phase {
        logger.info("Skipping review-fix iterations (resuming from a later checkpoint phase)");
    }

    if should_run_from(PipelinePhase::CommitMessage) {
        // Save checkpoint at start of commit message phase (if enabled)
        if config.checkpoint_enabled {
            let _ = save_checkpoint(&PipelineCheckpoint::new(
                PipelinePhase::CommitMessage,
                config.developer_iters,
                config.developer_iters,
                config.reviewer_reviews,
                config.reviewer_reviews,
                &developer_agent,
                &reviewer_agent,
            ));
        }

        // Commit message generation phase
        logger.subheader("Generating Commit Message");
        update_status("Generating commit message", config.isolation_mode)?;

        let commit_msg_prompt = prompt_for_agent(
            Role::Reviewer,
            Action::GenerateCommitMessage,
            reviewer_context,
            None,
            None,
            None, // No guidelines needed for commit message generation
        );

        let _ = run_with_fallback(
            AgentRole::Reviewer,
            "generate commit msg",
            &commit_msg_prompt,
            ".agent/logs/commit_message",
            &mut timer,
            &logger,
            &colors,
            &config,
            &registry,
            &reviewer_agent,
        );
        stats.reviewer_runs_completed += 1;
    } else if run_any_reviewer_phase {
        logger.info("Skipping commit message generation (resuming from a later checkpoint phase)");
    }

    update_status("In progress.", config.isolation_mode)?;

    // Phase 3: Final checks (if configured)
    if let Some(ref full_cmd) = config.full_check_cmd {
        if should_run_from(PipelinePhase::FinalValidation) {
            if config.checkpoint_enabled {
                let _ = save_checkpoint(&PipelineCheckpoint::new(
                    PipelinePhase::FinalValidation,
                    config.developer_iters,
                    config.developer_iters,
                    config.reviewer_reviews,
                    config.reviewer_reviews,
                    &developer_agent,
                    &reviewer_agent,
                ));
            }

            logger.header("PHASE 3: Final Validation", |c| c.yellow());
            logger.info(&format!(
                "Running full check: {}{}{}",
                colors.dim(),
                full_cmd,
                colors.reset()
            ));

            let status = Command::new("sh").args(["-c", full_cmd]).status()?;

            if status.success() {
                logger.success("Full check passed");
            } else {
                logger.error("Full check failed");
                anyhow::bail!("Full check failed");
            }
        } else {
            logger.header("PHASE 3: Final Validation", |c| c.yellow());
            logger.info("Skipping final validation (resuming from a later checkpoint phase)");
        }
    }

    // Phase 4: Commit (always done programmatically by Ralph)
    end_agent_phase()?;
    disable_git_wrapper(agent_phase_guard.git_helpers);
    if let Err(err) = uninstall_hooks(&logger) {
        logger.warn(&format!("Failed to uninstall Ralph hooks: {}", err));
    }

    logger.header("PHASE 4: Commit Changes", |c| c.green());

    logger.info("Staging all changes...");
    git_add_all()?;

    // Show what we're committing
    println!();
    println!("{}Changes to commit:{}", colors.bold(), colors.reset());
    let status_output = Command::new("git").args(["status", "--short"]).output()?;
    let lines: Vec<&str> = std::str::from_utf8(&status_output.stdout)
        .unwrap_or("")
        .lines()
        .take(20)
        .collect();
    for line in lines {
        println!("  {}{}{}", colors.dim(), line, colors.reset());
    }
    println!();

    // Read commit message from file (required)
    let final_commit_msg = read_commit_message_file()?;
    logger.info(&format!(
        "Commit message: {}{}{}",
        colors.cyan(),
        final_commit_msg,
        colors.reset()
    ));

    logger.info("Creating commit...");
    if git_commit(&final_commit_msg)? {
        logger.success("Commit created successfully");
    } else {
        logger.warn("Nothing to commit (working tree clean)");
    }

    // Delete commit-message.txt after committing
    if let Err(err) = delete_commit_message_file() {
        logger.warn(&format!("Failed to delete commit-message.txt: {}", err));
    }

    // Clear checkpoint on successful completion
    if let Err(e) = clear_checkpoint() {
        logger.warn(&format!("Failed to clear checkpoint: {}", e));
    }

    // Final summary
    logger.header("Pipeline Complete", |c| c.green());

    println!();
    println!(
        "{}{}📊 Summary{}",
        colors.bold(),
        colors.white(),
        colors.reset()
    );
    println!(
        "{}──────────────────────────────────{}",
        colors.dim(),
        colors.reset()
    );
    println!(
        "  {}⏱{}  Total time:      {}{}{}",
        colors.cyan(),
        colors.reset(),
        colors.bold(),
        timer.elapsed_formatted(),
        colors.reset()
    );
    println!(
        "  {}🔄{}  Dev runs:        {}{}{}/{}",
        colors.blue(),
        colors.reset(),
        colors.bold(),
        stats.developer_runs_completed,
        colors.reset(),
        config.developer_iters
    );
    println!(
        "  {}🔍{}  Review runs:     {}{}{}",
        colors.magenta(),
        colors.reset(),
        colors.bold(),
        stats.reviewer_runs_completed,
        colors.reset()
    );
    println!(
        "  {}📝{}  Changes detected: {}{}{}",
        colors.green(),
        colors.reset(),
        colors.bold(),
        stats.changes_detected,
        colors.reset()
    );

    // Review metrics from ISSUES.md
    if let Ok(metrics) = ReviewMetrics::from_issues_file() {
        if metrics.issues_file_found {
            if metrics.no_issues_declared && metrics.total_issues == 0 {
                println!(
                    "  {}✓{}   Review result:   {}{}{}",
                    colors.green(),
                    colors.reset(),
                    colors.bold(),
                    metrics.summary(),
                    colors.reset()
                );
            } else if metrics.total_issues > 0 {
                // Use summary() for a concise one-line display
                println!(
                    "  {}🔎{}  Review summary:  {}{}{}",
                    colors.yellow(),
                    colors.reset(),
                    colors.bold(),
                    metrics.summary(),
                    colors.reset()
                );
                // Show unresolved count
                let unresolved = metrics.unresolved_issues();
                if unresolved > 0 {
                    println!(
                        "  {}⚠{}   Unresolved:      {}{}{} issues remaining",
                        colors.red(),
                        colors.reset(),
                        colors.bold(),
                        unresolved,
                        colors.reset()
                    );
                }
                // Show detailed breakdown in verbose mode
                if config.verbosity.is_verbose() && metrics.total_issues > 1 {
                    println!("  {}📊{}  Breakdown:", colors.dim(), colors.reset());
                    for line in metrics.detailed_summary().lines() {
                        println!("      {}{}{}", colors.dim(), line.trim(), colors.reset());
                    }
                }
                // Highlight blocking issues
                if metrics.has_blocking_issues() {
                    println!(
                        "  {}🚨{}  BLOCKING:        {}{}{} critical/high issues unresolved",
                        colors.red(),
                        colors.reset(),
                        colors.bold(),
                        metrics.unresolved_blocking_issues(),
                        colors.reset()
                    );
                }
            }
        }
    }
    println!();

    println!(
        "{}{}📁 Output Files{}",
        colors.bold(),
        colors.white(),
        colors.reset()
    );
    println!(
        "{}──────────────────────────────────{}",
        colors.dim(),
        colors.reset()
    );
    println!(
        "  → {}PROMPT.md{}           Goal definition",
        colors.cyan(),
        colors.reset()
    );
    println!(
        "  → {}.agent/STATUS.md{}    Current status",
        colors.cyan(),
        colors.reset()
    );
    // Only show ISSUES.md and NOTES.md when NOT in isolation mode
    if !config.isolation_mode {
        println!(
            "  → {}.agent/ISSUES.md{}    Review findings",
            colors.cyan(),
            colors.reset()
        );
        println!(
            "  → {}.agent/NOTES.md{}     Progress notes",
            colors.cyan(),
            colors.reset()
        );
    }
    println!(
        "  → {}.agent/logs/{}        Detailed logs",
        colors.cyan(),
        colors.reset()
    );
    println!();

    logger.success("Ralph pipeline completed successfully!");

    agent_phase_guard.disarm();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_model_with_provider_emits_full_model_flag() {
        // Provider override should preserve a full -m/--model flag rather than returning provider/model.
        assert_eq!(
            resolve_model_with_provider(
                Some("opencode"),
                Some("-m zai/glm-4.7"),
                Some("-m anthropic/claude-sonnet-4")
            )
            .as_deref(),
            Some("-m opencode/glm-4.7")
        );

        // Provider-only override should use the agent's configured model name.
        assert_eq!(
            resolve_model_with_provider(Some("opencode"), None, Some("-m anthropic/claude-sonnet-4"))
                .as_deref(),
            Some("-m opencode/claude-sonnet-4")
        );

        // Model-only overrides normalize bare provider/model to a full flag.
        assert_eq!(
            resolve_model_with_provider(None, Some("opencode/glm-4.7-free"), None).as_deref(),
            Some("-m opencode/glm-4.7-free")
        );

        // Preserve the user's style when provided.
        assert_eq!(
            resolve_model_with_provider(None, Some("--model=opencode/glm-4.7-free"), None).as_deref(),
            Some("--model=opencode/glm-4.7-free")
        );
    }

    #[test]
    fn argv_requests_json_detects_common_flags() {
        assert!(argv_requests_json(&split_command("tool --json").unwrap()));
        assert!(argv_requests_json(
            &split_command("tool --output-format=stream-json").unwrap()
        ));
        assert!(argv_requests_json(
            &split_command("tool --output-format stream-json").unwrap()
        ));
        assert!(argv_requests_json(
            &split_command("tool --format json").unwrap()
        ));
        assert!(argv_requests_json(&split_command("tool -F json").unwrap()));
        assert!(argv_requests_json(
            &split_command("tool -o stream-json").unwrap()
        ));
    }

    #[test]
    fn format_generic_json_for_display_pretty_prints_when_full() {
        let line = r#"{"type":"message","content":{"text":"hello"}}"#;
        let formatted = format_generic_json_for_display(line, Verbosity::Full);
        assert!(formatted.contains('\n'));
        assert!(formatted.contains("\"type\""));
        assert!(formatted.contains("\"message\""));
    }

    #[test]
    fn contract_qwen_stream_json_parses_with_claude_parser() {
        let registry = AgentRegistry::new().unwrap();
        let qwen = registry.get("qwen").unwrap();

        let cmd = qwen.build_cmd(true, true, true);
        let argv = split_command(&cmd).unwrap();

        let parser_type = qwen.json_parser;
        let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
        assert!(uses_json, "Qwen should run in JSON-parsing mode");
        assert_eq!(parser_type, JsonParserType::Claude);

        // Claude stream-json compatibility (used by qwen-code)
        let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello from qwen"}]}}"#;
        let input = std::io::Cursor::new(format!("{}\n", json));
        let reader = std::io::BufReader::new(input);

        let mut out = Vec::new();
        let colors = Colors { enabled: false };
        let parser = crate::json_parser::ClaudeParser::new(colors, Verbosity::Normal);
        parser.parse_stream(reader, &mut out).unwrap();

        let rendered = String::from_utf8(out).unwrap();
        assert!(rendered.contains("Hello from qwen"));
    }

    #[test]
    fn contract_vibe_runs_in_plain_text_mode() {
        let registry = AgentRegistry::new().unwrap();
        let vibe = registry.get("vibe").unwrap();

        let cmd = vibe.build_cmd(true, true, true);
        let argv = split_command(&cmd).unwrap();

        let parser_type = vibe.json_parser;
        let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
        assert!(!uses_json, "vibe should not enable JSON parsing by default");
        assert_eq!(parser_type, JsonParserType::Generic);
    }

    #[test]
    fn contract_llama_cli_runs_in_plain_text_mode_with_local_model_flag() {
        let registry = AgentRegistry::new().unwrap();
        let llama = registry.get("llama-cli").unwrap();

        let cmd = llama.build_cmd(true, true, true);
        assert!(
            cmd.contains(" -m "),
            "llama-cli should default to a local model path"
        );

        let argv = split_command(&cmd).unwrap();

        let parser_type = llama.json_parser;
        let uses_json = parser_type != JsonParserType::Generic || argv_requests_json(&argv);
        assert!(
            !uses_json,
            "llama-cli should not enable JSON parsing by default"
        );
        assert_eq!(parser_type, JsonParserType::Generic);
    }

    #[test]
    fn run_with_prompt_returns_command_result_for_missing_binary() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let config = Config {
            interactive: false,
            prompt_path: dir.path().join("prompt.txt"),
            ..Config::default()
        };

        let result = run_with_prompt(
            "test",
            "definitely-not-a-real-binary-ralph",
            "hello",
            &dir.path().join("log.txt").display().to_string(),
            JsonParserType::Generic,
            &mut timer,
            &logger,
            &colors,
            &config,
        )
        .unwrap();

        assert_eq!(result.exit_code, 127);
        assert!(!result.stderr.is_empty());
    }
}
