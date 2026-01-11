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
mod prompts;
mod timer;
mod utils;

use crate::agents::{
    AgentErrorKind, AgentRegistry, AgentRole, AgentsConfigFile, ConfigInitResult, JsonParserType,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::git_helpers::{
    cleanup_agent_phase_silent, cleanup_orphaned_marker, disable_git_wrapper, end_agent_phase,
    get_repo_root, git_add_all, git_commit, git_snapshot, require_git_repo, start_agent_phase,
    uninstall_hooks, GitHelpers,
};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, Role};
use crate::timer::Timer;
use crate::utils::{
    clean_context_for_reviewer, cleanup_generated_files, delete_commit_message_file,
    delete_plan_file, ensure_files, print_progress, read_commit_message_file, update_status,
    Logger,
};
use clap::{Parser, ValueEnum};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
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
EXAMPLES:\n\
    ralph \"feat: add login button\"              Basic usage\n\
    ralph --developer-iters 3                    Fewer iterations\n\
    ralph --preset opencode                      Use opencode for both\n\
    ralph --developer-agent claude --reviewer-agent codex\n\
    ralph --use-fallback                         Auto-switch on errors\n\n\
ENVIRONMENT VARIABLES:\n\
    RALPH_DEVELOPER_AGENT    Developer agent name (default: claude)\n\
    RALPH_REVIEWER_AGENT     Reviewer agent name (default: codex)\n\
    RALPH_DEVELOPER_ITERS    Developer iterations (default: 5)\n\
    RALPH_REVIEWER_REVIEWS   Re-review passes (default: 2)\n\
    RALPH_AGENTS_CONFIG      Path to agents.toml\n\
    RALPH_USE_FALLBACK       Enable agent fallback (true/false)")]
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
        aliases = ["claude-iters"],
        value_name = "N",
        help = "Number of developer agent iterations"
    )]
    developer_iters: Option<u32>,

    /// Number of reviewer re-review passes after fix (default: 2)
    #[arg(
        long = "reviewer-reviews",
        env = "RALPH_REVIEWER_REVIEWS",
        aliases = ["codex-reviews"],
        value_name = "N",
        help = "Number of reviewer re-review passes after fix"
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

    /// Developer/driver agent to use (default: claude)
    #[arg(
        long,
        env = "RALPH_DEVELOPER_AGENT",
        aliases = ["driver-agent"],
        value_name = "AGENT",
        help = "Developer agent for code implementation"
    )]
    developer_agent: Option<String>,

    /// Reviewer agent to use (default: codex)
    #[arg(
        long,
        env = "RALPH_REVIEWER_AGENT",
        value_name = "AGENT",
        help = "Reviewer agent for code review"
    )]
    reviewer_agent: Option<String>,

    /// Verbosity level (0=quiet, 1=normal, 2=verbose, 3=full)
    #[arg(
        short,
        long,
        default_value = "1",
        value_name = "LEVEL",
        help = "Output verbosity (0=quiet, 1=normal, 2=verbose, 3=full)"
    )]
    verbosity: u8,

    /// Enable automatic agent fallback on errors
    #[arg(
        long,
        env = "RALPH_USE_FALLBACK",
        help = "Auto-switch agents on rate limits, auth errors, etc."
    )]
    use_fallback: bool,

    /// List all configured agents and exit
    #[arg(long, help = "Show all agents from registry and config file")]
    list_agents: bool,

    /// List only agents found in PATH and exit
    #[arg(long, help = "Show only agents that are installed and available")]
    list_available_agents: bool,

    /// Initialize agents.toml config file and exit
    #[arg(long, help = "Create .agent/agents.toml with default settings")]
    init: bool,
}

#[derive(Clone, Debug, ValueEnum)]
enum Preset {
    /// Historical default: claude (dev) + codex (review)
    #[value(alias = "claude-codex")]
    Default,
    /// Use opencode for both developer and reviewer
    #[value(alias = "opencode-both", alias = "opencode-only")]
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
#[allow(dead_code)]
struct CommandResult {
    exit_code: i32,
    stderr: String,
}

/// Run a command with a prompt argument
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
    let shell_escaped_prompt = shell_escape::escape(prompt.into());
    let full_cmd = format!("{} {}", cmd_str, shell_escaped_prompt);
    logger.info(&format!(
        "Executing: {}{}...{}",
        colors.dim(),
        &full_cmd.chars().take(80).collect::<String>(),
        colors.reset()
    ));

    // Determine if JSON parsing is needed (based on parser type and command flags)
    let uses_json = parser_type != JsonParserType::Generic
        || cmd_str.contains("--output-format=stream-json")
        || cmd_str.contains("--json");

    logger.info(&format!("Using {} parser...", parser_type));
    File::create(logfile)?;

    // Execute command
    let mut child = Command::new("sh")
        .args(["-c", &full_cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;
    let reader = BufReader::new(stdout);

    // Capture stderr in a separate thread
    let stderr_handle = child.stderr.take();

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
            JsonParserType::Generic => {
                let log_file = OpenOptions::new().create(true).append(true).open(logfile)?;
                let mut log_writer = io::BufWriter::new(log_file);

                for line in reader.lines() {
                    let line = line?;
                    if line.is_empty() {
                        continue;
                    }

                    let output = format!(
                        "{}[Agent]{} {}\n",
                        colors.dim(),
                        colors.reset(),
                        &line.chars().take(100).collect::<String>()
                    );
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

    // Collect stderr
    let stderr = if let Some(mut stderr_pipe) = stderr_handle {
        let mut stderr_output = String::new();
        if let Err(err) = std::io::Read::read_to_string(&mut stderr_pipe, &mut stderr_output) {
            logger.warn(&format!("Failed to read stderr: {}", err));
        }
        stderr_output
    } else {
        String::new()
    };

    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(1);

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

/// Run a command with automatic fallback to alternative agents on failure
///
/// This function attempts to run the command with the primary agent first,
/// then falls back to alternative agents based on the fallback configuration
/// if the primary agent fails with specific error types (rate limiting,
/// token exhaustion, auth failures, command not found).
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

    // Start with primary agent
    let mut agents_to_try: Vec<&str> = vec![primary_agent];

    // Add configured fallbacks that aren't the primary
    for fb in &fallbacks {
        if *fb != primary_agent && !agents_to_try.contains(fb) {
            agents_to_try.push(fb);
        }
    }

    for (agent_index, agent_name) in agents_to_try.iter().enumerate() {
        let Some(agent_config) = registry.get(agent_name) else {
            logger.warn(&format!(
                "Agent '{}' not found in registry, skipping",
                agent_name
            ));
            continue;
        };

        let cmd_str = agent_config.build_cmd(true, true, role == AgentRole::Developer);
        let parser_type = agent_config.json_parser;
        let label = format!("{} ({})", base_label, agent_name);
        let logfile = format!("{}_{}.log", logfile_prefix, agent_name);

        // Try with retries
        for retry in 0..fallback_config.max_retries {
            if retry > 0 {
                logger.info(&format!(
                    "Retry {}/{} for {} after {}ms delay...",
                    retry, fallback_config.max_retries, agent_name, fallback_config.retry_delay_ms
                ));
                std::thread::sleep(std::time::Duration::from_millis(
                    fallback_config.retry_delay_ms,
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

            // Classify the error
            let error_kind = AgentErrorKind::classify(result.exit_code, &result.stderr);

            logger.warn(&format!(
                "Agent '{}' failed with {:?} (exit code {})",
                agent_name, error_kind, result.exit_code
            ));

            // Decide whether to retry or fallback
            if error_kind.should_retry() && retry + 1 < fallback_config.max_retries {
                continue; // Retry same agent
            }

            if error_kind.should_fallback() && agent_index + 1 < agents_to_try.len() {
                logger.info(&format!(
                    "Switching to fallback agent: {}",
                    agents_to_try[agent_index + 1]
                ));
                break; // Try next agent
            }

            // For permanent errors or no more retries, try next agent or give up
            if agent_index + 1 >= agents_to_try.len() {
                logger.error("All agents exhausted, returning last error");
                return Ok(result.exit_code);
            }
            break;
        }
    }

    // Should not reach here, but return error if we do
    Ok(1)
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
    let logger = Logger::new(colors).with_log_file(".agent/logs/pipeline.log");

    // Load configuration
    let mut config = Config::from_env().with_commit_msg(args.commit_msg);
    config.verbosity = args.verbosity.into();

    // Apply preset first (CLI/env preset overrides env-selected agents, but can be overridden by
    // explicit --developer-agent/--reviewer-agent flags below).
    if let Some(preset) = args.preset {
        match preset {
            Preset::Default => {
                config.developer_agent = "claude".to_string();
                config.reviewer_agent = "codex".to_string();
            }
            Preset::Opencode => {
                config.developer_agent = "opencode".to_string();
                config.reviewer_agent = "opencode".to_string();
            }
        }
    }

    if let Some(iters) = args.developer_iters {
        config.developer_iters = iters;
    }
    if let Some(reviews) = args.reviewer_reviews {
        config.reviewer_reviews = reviews;
    }
    if let Some(agent) = args.developer_agent {
        config.developer_agent = agent;
    }
    if let Some(agent) = args.reviewer_agent {
        config.reviewer_agent = agent;
    }
    if args.use_fallback {
        config.use_fallback = true;
    }

    // Handle --init flag: create agents.toml if it doesn't exist and exit
    if args.init {
        match AgentsConfigFile::ensure_config_exists(&config.agents_config_path) {
            Ok(ConfigInitResult::Created) => {
                println!(
                    "{}Created {}{}{}\n",
                    colors.green(),
                    colors.bold(),
                    config.agents_config_path.display(),
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
                    config.agents_config_path.display()
                );
                println!("Edit the file to customize, or delete it to regenerate from defaults.");
                return Ok(());
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to create config file {}: {}",
                    config.agents_config_path.display(),
                    e
                );
            }
        }
    }

    // Check if agents.toml exists; if not, create it and prompt user
    match AgentsConfigFile::ensure_config_exists(&config.agents_config_path) {
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
                config.agents_config_path.display(),
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
                config.agents_config_path.display(),
                e
            ));
            // Continue with built-in defaults
        }
    }

    // Initialize agent registry (load from config file if present)
    let mut registry = match AgentRegistry::with_config_file(&config.agents_config_path) {
        Ok(r) => r,
        Err(e) => {
            logger.warn(&format!(
                "Failed to load agents config from {}: {}, using defaults",
                config.agents_config_path.display(),
                e
            ));
            AgentRegistry::new().map_err(|defaults_err| {
                anyhow::anyhow!(
                    "Failed to load built-in default agents config (examples/agents.toml): {}",
                    defaults_err
                )
            })?
        }
    };

    if config.use_fallback {
        let mut fallback = registry.fallback_config().clone();
        let default_chain = [
            "claude", "codex", "opencode", "aider", "goose", "cline", "continue", "amazon-q",
            "gemini",
        ];

        if !fallback.has_fallbacks(AgentRole::Developer) {
            fallback.developer = default_chain.iter().map(|s| s.to_string()).collect();
        }
        if !fallback.has_fallbacks(AgentRole::Reviewer) {
            fallback.reviewer = default_chain.iter().map(|s| s.to_string()).collect();
        }

        registry.set_fallback(fallback);
    }

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

    // Get agent commands and parser types
    let developer_cmd = if let Some(cmd) = config.developer_cmd.clone() {
        cmd
    } else {
        registry
            .developer_cmd(&config.developer_agent)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown developer agent '{}'. Use --list-agents or define it in {}.",
                    config.developer_agent,
                    config.agents_config_path.display()
                )
            })?
    };
    let reviewer_cmd = if let Some(cmd) = config.reviewer_cmd.clone() {
        cmd
    } else {
        registry
            .reviewer_cmd(&config.reviewer_agent)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown reviewer agent '{}'. Use --list-agents or define it in {}.",
                    config.reviewer_agent,
                    config.agents_config_path.display()
                )
            })?
    };
    let developer_parser = registry.parser_type(&config.developer_agent);
    let reviewer_parser = registry.parser_type(&config.reviewer_agent);

    // Require git repo
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;
    ensure_files()?;

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
        config.developer_agent,
        config.reviewer_agent,
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
    println!();

    // Phase 1: Development (PROMPT → PLAN → Execute → Delete PLAN, repeated X times)
    logger.header("PHASE 1: Development", |c| c.blue());
    logger.info(&format!(
        "Running {}{}{} developer iterations ({})",
        colors.bold(),
        config.developer_iters,
        colors.reset(),
        config.developer_agent
    ));

    let mut prev_snap = git_snapshot()?;
    let developer_context = ContextLevel::from(config.developer_context);

    for i in 1..=config.developer_iters {
        logger.subheader(&format!("Iteration {} of {}", i, config.developer_iters));
        print_progress(i, config.developer_iters, "Overall");

        // Step 1: Create PLAN from PROMPT
        logger.info("Creating plan from PROMPT.md...");
        update_status(
            "Starting planning phase",
            "none",
            "Analyze PROMPT.md and create PLAN.md",
        )?;

        let plan_prompt = prompt_for_agent(
            Role::Developer,
            Action::Plan,
            ContextLevel::Normal,
            None,
            None,
            None,
        );

        if config.use_fallback {
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
                &config.developer_agent,
            );
        } else {
            let _ = run_with_prompt(
                &format!("{} planning #{}", config.developer_agent, i),
                &developer_cmd,
                &plan_prompt,
                &format!(".agent/logs/planning_{}.log", i),
                developer_parser,
                &mut timer,
                &logger,
                &colors,
                &config,
            );
        }

        // Verify PLAN.md was created (required)
        let plan_path = std::path::Path::new(".agent/PLAN.md");
        let plan_ok = plan_path
            .exists()
            .then(|| fs::read_to_string(plan_path).ok())
            .flatten()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        if !plan_ok {
            anyhow::bail!("Planning phase did not create a non-empty .agent/PLAN.md");
        }
        logger.success("PLAN.md created");

        // Step 2: Execute the PLAN
        logger.info("Executing plan...");
        update_status(
            "Starting development iteration",
            "none",
            "Make progress on PROMPT.md goals",
        )?;

        let prompt = prompt_for_agent(
            Role::Developer,
            Action::Iterate,
            developer_context,
            Some(i),
            Some(config.developer_iters),
            None,
        );
        let logfile = format!(".agent/logs/developer_{}.log", i);

        let exit_code = if config.use_fallback {
            run_with_fallback(
                AgentRole::Developer,
                &format!("run #{}", i),
                &prompt,
                &format!(".agent/logs/developer_{}", i),
                &mut timer,
                &logger,
                &colors,
                &config,
                &registry,
                &config.developer_agent,
            )?
        } else {
            let result = run_with_prompt(
                &format!("{} run #{}", config.developer_agent, i),
                &developer_cmd,
                &prompt,
                &logfile,
                developer_parser,
                &mut timer,
                &logger,
                &colors,
                &config,
            )?;
            result.exit_code
        };

        if exit_code != 0 {
            logger.error(&format!(
                "Iteration {} encountered an error but continuing",
                i
            ));
        }

        stats.developer_runs_completed += 1;
        update_status(
            "Completed progress step",
            "none",
            "Continue work on PROMPT.md goals",
        )?;

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

    update_status("Code changes made", "none", "Evaluate codebase")?;

    // Phase 2: Reviewer review/fix cycle
    logger.header("PHASE 2: Review & Fix", |c| c.magenta());

    // Clean context for reviewer if using minimal context
    let reviewer_context = ContextLevel::from(config.reviewer_context);
    if reviewer_context == ContextLevel::Minimal {
        clean_context_for_reviewer(&logger)?;
    }

    logger.info(&format!(
        "Running review → fix → review×{}{}{} cycle ({})",
        colors.bold(),
        config.reviewer_reviews,
        colors.reset(),
        config.reviewer_agent
    ));

    // Initial review
    logger.subheader("Initial Review");
    update_status("Starting code review", "none", "Evaluate codebase")?;

    let prompt = prompt_for_agent(
        Role::Reviewer,
        Action::Review,
        reviewer_context,
        None,
        None,
        None,
    );
    if config.use_fallback {
        let _ = run_with_fallback(
            AgentRole::Reviewer,
            "review (initial)",
            &prompt,
            ".agent/logs/reviewer_review_1",
            &mut timer,
            &logger,
            &colors,
            &config,
            &registry,
            &config.reviewer_agent,
        );
    } else {
        let _ = run_with_prompt(
            &format!("{} review (initial)", config.reviewer_agent),
            &reviewer_cmd,
            &prompt,
            ".agent/logs/codex_review_1.log",
            reviewer_parser,
            &mut timer,
            &logger,
            &colors,
            &config,
        );
    }
    stats.reviewer_runs_completed += 1;

    // Applying fixes
    logger.subheader("Applying Fixes");
    update_status("Applying fixes", "none", "Address issues found")?;

    let prompt = prompt_for_agent(
        Role::Reviewer,
        Action::Fix,
        reviewer_context,
        None,
        None,
        None,
    );
    if config.use_fallback {
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
            &config.reviewer_agent,
        );
    } else {
        let _ = run_with_prompt(
            &format!("{} fix", config.reviewer_agent),
            &reviewer_cmd,
            &prompt,
            ".agent/logs/codex_fix.log",
            reviewer_parser,
            &mut timer,
            &logger,
            &colors,
            &config,
        );
    }
    stats.reviewer_runs_completed += 1;

    // Verification reviews
    for j in 1..=config.reviewer_reviews {
        logger.subheader(&format!(
            "Verification Review {} of {}",
            j, config.reviewer_reviews
        ));
        print_progress(j, config.reviewer_reviews, "Review passes");

        update_status("Verification review", "none", "Re-evaluate codebase")?;

        let prompt = prompt_for_agent(
            Role::Reviewer,
            Action::ReviewAgain,
            reviewer_context,
            None,
            None,
            None,
        );
        let logfile = format!(".agent/logs/codex_review_{}.log", j + 1);

        if config.use_fallback {
            let _ = run_with_fallback(
                AgentRole::Reviewer,
                &format!("re-review #{}", j),
                &prompt,
                &format!(".agent/logs/reviewer_review_{}", j + 1),
                &mut timer,
                &logger,
                &colors,
                &config,
                &registry,
                &config.reviewer_agent,
            );
        } else {
            let _ = run_with_prompt(
                &format!("{} re-review #{}", config.reviewer_agent, j),
                &reviewer_cmd,
                &prompt,
                &logfile,
                reviewer_parser,
                &mut timer,
                &logger,
                &colors,
                &config,
            );
        }
        stats.reviewer_runs_completed += 1;
    }

    // Commit message generation phase
    logger.subheader("Generating Commit Message");
    update_status(
        "Generating commit message",
        "none",
        "Agent writes commit message",
    )?;

    let commit_msg_prompt = prompt_for_agent(
        Role::Reviewer,
        Action::GenerateCommitMessage,
        reviewer_context,
        None,
        None,
        None,
    );

    if config.use_fallback {
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
            &config.reviewer_agent,
        );
    } else {
        let _ = run_with_prompt(
            &format!("{} generate commit msg", config.reviewer_agent),
            &reviewer_cmd,
            &commit_msg_prompt,
            ".agent/logs/commit_message.log",
            reviewer_parser,
            &mut timer,
            &logger,
            &colors,
            &config,
        );
    }
    stats.reviewer_runs_completed += 1;

    update_status("Review phase complete", "none", "Awaiting finalization")?;

    // Phase 3: Final checks (if configured)
    if let Some(ref full_cmd) = config.full_check_cmd {
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
