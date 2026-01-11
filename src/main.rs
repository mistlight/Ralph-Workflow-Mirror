//! Ralph: PROMPT-driven agent loop for git repos
//!
//! Runs:
//! - Claude: iterative progress against PROMPT.md
//! - Codex: review → fix → review passes
//! - Optional fast/full checks
//! - Final `git add -A` + `git commit -m <msg>`

use clap::Parser;
use ralph::agents::{AgentRegistry, AgentType};
use ralph::colors::Colors;
use ralph::config::Config;
use ralph::git_helpers::{
    allow_reviewer_commit, disable_git_wrapper, end_agent_phase, get_head_commit,
    get_last_commit_message, get_repo_root, git_add_all, git_commit, git_snapshot,
    require_git_repo, start_agent_phase, GitHelpers,
};
use ralph::json_parser::detect_agent_type;
use ralph::prompts::{
    prompt_claude_iteration, prompt_codex_fix, prompt_codex_review, prompt_codex_review_again,
    prompt_commit, ContextLevel,
};
use ralph::timer::Timer;
use ralph::utils::{
    clean_context_for_reviewer, ensure_files, print_progress, update_status, Logger,
};
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Ralph: PROMPT-driven agent orchestrator for git repos
#[derive(Parser, Debug)]
#[command(name = "ralph")]
#[command(about = "PROMPT-driven multi-agent orchestrator for git repos")]
#[command(version)]
struct Args {
    /// Commit message for the final commit
    #[arg(default_value = "chore: apply PROMPT loop + Codex review/fix/review/review")]
    commit_msg: String,

    /// Number of Claude iterations
    #[arg(long, env = "CLAUDE_ITERS")]
    claude_iters: Option<u32>,

    /// Number of Codex review passes after fix
    #[arg(long, env = "CODEX_REVIEWS")]
    codex_reviews: Option<u32>,

    /// Developer agent to use
    #[arg(long, env = "RALPH_DEVELOPER_AGENT")]
    developer_agent: Option<String>,

    /// Reviewer agent to use
    #[arg(long, env = "RALPH_REVIEWER_AGENT")]
    reviewer_agent: Option<String>,

    /// Verbosity level (0=quiet, 1=normal, 2=verbose, 3=full)
    #[arg(short, long, default_value = "1")]
    verbosity: u8,
}

/// Statistics tracking
struct Stats {
    changes_detected: u32,
    claude_runs_completed: u32,
    codex_runs_completed: u32,
    reviewer_committed: bool,
}

impl Stats {
    fn new() -> Self {
        Self {
            changes_detected: 0,
            claude_runs_completed: 0,
            codex_runs_completed: 0,
            reviewer_committed: false,
        }
    }
}

/// Run a command with a prompt argument
#[allow(clippy::too_many_arguments)]
fn run_with_prompt(
    label: &str,
    cmd_str: &str,
    prompt: &str,
    logfile: &str,
    timer: &mut Timer,
    logger: &Logger,
    colors: &Colors,
    config: &Config,
) -> io::Result<i32> {
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

    // Detect agent type for parsing
    let agent_type_str = detect_agent_type(cmd_str);
    let agent_type = AgentType::from_cmd(cmd_str);
    let uses_json = cmd_str.contains("--output-format=stream-json") || cmd_str.contains("--json");

    logger.info(&format!("Parsing {} JSON stream...", agent_type_str));

    // Create log file
    let log_file = File::create(logfile)?;
    let mut log_writer = io::BufWriter::new(log_file);

    // Execute command
    let mut child = Command::new("sh")
        .args(["-c", &full_cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let reader = BufReader::new(stdout);

    if uses_json {
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }

            // Parse and display
            if let Some(output) = match agent_type {
                AgentType::Claude => {
                    let p = ralph::json_parser::ClaudeParser::new(*colors, config.verbosity);
                    p.parse_event(&line)
                }
                AgentType::Codex => {
                    let p = ralph::json_parser::CodexParser::new(*colors, config.verbosity);
                    p.parse_event(&line)
                }
                _ => Some(format!("{}[Agent]{} {}\n", colors.dim(), colors.reset(), &line.chars().take(100).collect::<String>())),
            } {
                print!("{}", output);
            }

            // Log raw JSON
            writeln!(log_writer, "{}", line)?;
        }
    } else {
        // Non-JSON mode: just pipe through
        for line in reader.lines() {
            let line = line?;
            println!("{}", line);
            writeln!(log_writer, "{}", line)?;
        }
    }

    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(1);

    if exit_code != 0 {
        logger.error(&format!("Command exited with code {}", exit_code));
    } else {
        logger.success(&format!("Completed in {}", timer.phase_elapsed_formatted()));
    }

    Ok(exit_code)
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let colors = Colors::new();
    let logger = Logger::new(colors).with_log_file(".agent/logs/pipeline.log");

    // Load configuration
    let mut config = Config::from_env();
    config.commit_msg = args.commit_msg;
    config.verbosity = args.verbosity.into();

    if let Some(iters) = args.claude_iters {
        config.claude_iters = iters;
    }
    if let Some(reviews) = args.codex_reviews {
        config.codex_reviews = reviews;
    }
    if let Some(agent) = args.developer_agent {
        config.developer_agent = agent;
    }
    if let Some(agent) = args.reviewer_agent {
        config.reviewer_agent = agent;
    }

    // Initialize agent registry
    let registry = AgentRegistry::new();

    // Get agent commands
    let claude_cmd = config.claude_cmd.clone().unwrap_or_else(|| {
        registry
            .developer_cmd(&config.developer_agent)
            .unwrap_or_else(|| "claude -p --output-format=stream-json --dangerously-skip-permissions --verbose".to_string())
    });
    let codex_cmd = config.codex_cmd.clone().unwrap_or_else(|| {
        registry
            .reviewer_cmd(&config.reviewer_agent)
            .unwrap_or_else(|| "codex exec --json --yolo".to_string())
    });

    // Require git repo
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;
    ensure_files()?;

    // Set up git helpers
    let mut git_helpers = GitHelpers::new();

    // Cleanup handler (simplified - Rust doesn't have trap)
    let cleanup = || {
        let _ = end_agent_phase();
        disable_git_wrapper(&mut GitHelpers::new());
    };

    start_agent_phase(&mut git_helpers)?;

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
        "{}{}│{}  {}Claude × Codex pipeline for autonomous development{}       {}{}│{}",
        colors.bold(),
        colors.cyan(),
        colors.reset(),
        colors.dim(),
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

    // Phase 1: Claude iterations
    logger.header("PHASE 1: Claude Development", |c| c.blue());
    logger.info(&format!(
        "Running {}{}{} Claude iterations",
        colors.bold(),
        config.claude_iters,
        colors.reset()
    ));

    let mut prev_snap = git_snapshot()?;
    let developer_context = ContextLevel::from(config.developer_context);

    for i in 1..=config.claude_iters {
        logger.subheader(&format!("Claude Iteration {} of {}", i, config.claude_iters));
        print_progress(i, config.claude_iters, "Overall");

        update_status("Starting Claude iteration", "none", "Make progress on PROMPT.md goals")?;

        let prompt = prompt_claude_iteration(i, config.claude_iters, developer_context);
        let logfile = format!(".agent/logs/claude_{}.log", i);

        let exit_code = run_with_prompt(
            &format!("Claude run #{}", i),
            &claude_cmd,
            &prompt,
            &logfile,
            &mut timer,
            &logger,
            &colors,
            &config,
        )?;

        if exit_code != 0 {
            logger.error(&format!("Iteration {} encountered an error but continuing", i));
        }

        stats.claude_runs_completed += 1;
        update_status("Completed progress step", "none", "Continue work on PROMPT.md goals")?;

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
            let status = Command::new("sh")
                .args(["-c", fast_cmd])
                .status()?;

            if status.success() {
                logger.success("Fast check passed");
            } else {
                logger.warn("Fast check had issues (non-blocking)");
            }
        }
    }

    update_status("Code changes made", "none", "Evaluate codebase")?;

    // Phase 2: Codex review/fix cycle
    logger.header("PHASE 2: Codex Review & Fix", |c| c.magenta());

    // Clean context for reviewer if using minimal context
    let reviewer_context = ContextLevel::from(config.reviewer_context);
    if reviewer_context == ContextLevel::Minimal {
        clean_context_for_reviewer(&logger)?;
    }

    logger.info(&format!(
        "Running review → fix → review×{}{}{} cycle",
        colors.bold(),
        config.codex_reviews,
        colors.reset()
    ));

    // Initial review
    logger.subheader("Initial Review");
    update_status("Starting code review", "none", "Evaluate codebase")?;

    let prompt = prompt_codex_review(reviewer_context);
    let _ = run_with_prompt(
        "Codex review (initial)",
        &codex_cmd,
        &prompt,
        ".agent/logs/codex_review_1.log",
        &mut timer,
        &logger,
        &colors,
        &config,
    );
    stats.codex_runs_completed += 1;

    // Applying fixes
    logger.subheader("Applying Fixes");
    update_status("Applying fixes", "none", "Address issues found")?;

    let prompt = prompt_codex_fix();
    let _ = run_with_prompt(
        "Codex fix",
        &codex_cmd,
        &prompt,
        ".agent/logs/codex_fix.log",
        &mut timer,
        &logger,
        &colors,
        &config,
    );
    stats.codex_runs_completed += 1;

    // Verification reviews
    for j in 1..=config.codex_reviews {
        logger.subheader(&format!("Verification Review {} of {}", j, config.codex_reviews));
        print_progress(j, config.codex_reviews, "Review passes");

        update_status("Verification review", "none", "Re-evaluate codebase")?;

        let prompt = prompt_codex_review_again(reviewer_context);
        let logfile = format!(".agent/logs/codex_review_{}.log", j + 1);

        let _ = run_with_prompt(
            &format!("Codex re-review #{}", j),
            &codex_cmd,
            &prompt,
            &logfile,
            &mut timer,
            &logger,
            &colors,
            &config,
        );
        stats.codex_runs_completed += 1;
    }

    // Reviewer commit phase
    if config.reviewer_commits {
        logger.subheader("Reviewer Commit");
        update_status("Reviewer creating commit", "none", "Commit all changes")?;

        let head_before = get_head_commit()?;

        // Allow reviewer to commit
        allow_reviewer_commit(&mut git_helpers);

        let prompt = prompt_commit(&config.commit_msg);
        let _ = run_with_prompt(
            "Codex commit",
            &codex_cmd,
            &prompt,
            ".agent/logs/codex_commit.log",
            &mut timer,
            &logger,
            &colors,
            &config,
        );
        stats.codex_runs_completed += 1;

        // Verify reviewer created a new commit
        let head_after = get_head_commit()?;
        if head_before != head_after && !head_after.is_empty() {
            let msg = get_last_commit_message()?;
            logger.success(&format!(
                "Reviewer created commit: {}{}{}",
                colors.cyan(),
                msg,
                colors.reset()
            ));
            stats.reviewer_committed = true;
        } else {
            logger.warn("Reviewer did not create a new commit");
        }
    }

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

        let status = Command::new("sh")
            .args(["-c", full_cmd])
            .status()?;

        if status.success() {
            logger.success("Full check passed");
        } else {
            logger.error("Full check failed");
            cleanup();
            std::process::exit(1);
        }
    }

    // Phase 4: Commit (only if Ralph commits, not reviewer)
    end_agent_phase()?;
    disable_git_wrapper(&mut git_helpers);

    if !config.reviewer_commits {
        logger.header("PHASE 4: Commit Changes", |c| c.green());

        logger.info("Staging all changes...");
        git_add_all()?;

        // Show what we're committing
        println!();
        println!("{}Changes to commit:{}", colors.bold(), colors.reset());
        let status_output = Command::new("git")
            .args(["status", "--short"])
            .output()?;
        let lines: Vec<&str> = std::str::from_utf8(&status_output.stdout)
            .unwrap_or("")
            .lines()
            .take(20)
            .collect();
        for line in lines {
            println!("  {}{}{}", colors.dim(), line, colors.reset());
        }
        println!();

        logger.info("Creating commit...");
        if git_commit(&config.commit_msg)? {
            logger.success("Commit created successfully");
        } else {
            logger.warn("Nothing to commit (working tree clean)");
        }
    } else {
        logger.header("PHASE 4: Verify Commit", |c| c.green());

        if stats.reviewer_committed {
            let msg = get_last_commit_message()?;
            logger.success(&format!(
                "Verified reviewer commit: {}{}{}",
                colors.cyan(),
                msg,
                colors.reset()
            ));
        } else {
            logger.warn("Reviewer did not create a commit - using fallback");
            logger.info("Fallback: staging and committing changes...");
            git_add_all()?;
            if git_commit(&config.commit_msg)? {
                logger.success("Fallback commit created");
            } else {
                logger.warn("Nothing to commit (working tree clean)");
            }
        }
    }

    // Final summary
    logger.header("Pipeline Complete", |c| c.green());

    println!();
    println!("{}{}📊 Summary{}", colors.bold(), colors.white(), colors.reset());
    println!("{}──────────────────────────────────{}", colors.dim(), colors.reset());
    println!(
        "  {}⏱{}  Total time:      {}{}{}",
        colors.cyan(),
        colors.reset(),
        colors.bold(),
        timer.elapsed_formatted(),
        colors.reset()
    );
    println!(
        "  {}🔄{}  Claude runs:     {}{}{}/{}",
        colors.blue(),
        colors.reset(),
        colors.bold(),
        stats.claude_runs_completed,
        colors.reset(),
        config.claude_iters
    );
    println!(
        "  {}🔍{}  Codex runs:      {}{}{}",
        colors.magenta(),
        colors.reset(),
        colors.bold(),
        stats.codex_runs_completed,
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

    println!("{}{}📁 Output Files{}", colors.bold(), colors.white(), colors.reset());
    println!("{}──────────────────────────────────{}", colors.dim(), colors.reset());
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

    Ok(())
}
