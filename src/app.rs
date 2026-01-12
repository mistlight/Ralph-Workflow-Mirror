//! Application entrypoint and pipeline orchestration.
//!
//! This module exists to keep `src/main.rs` small and focused while preserving
//! the CLI surface and overall runtime behavior. It wires together:
//! - CLI/config parsing and plumbing commands
//! - Agent registry loading
//! - Repo setup and resume support
//! - Phase execution via `crate::phases`

use crate::agents::{AgentRegistry, AgentRole};
use crate::banner::{print_final_summary, print_welcome_banner};
use crate::cli::{
    apply_args_to_config, ensure_config_or_create, handle_diagnose, handle_dry_run, handle_init,
    handle_init_global, handle_list_agents, handle_list_available_agents, handle_list_providers,
    Args,
};
use crate::colors::Colors;
use crate::config::Config;
use crate::git_helpers::{
    cleanup_orphaned_marker, get_repo_root, git_add_all, git_commit, require_git_repo,
    start_agent_phase,
};
use crate::guidelines::ReviewGuidelines;
use crate::language_detector::{detect_stack, ProjectStack};
use crate::phases::{
    generate_commit_message, run_commit_phase, run_development_phase, run_review_phase,
    PhaseContext,
};
use crate::pipeline::{AgentPhaseGuard, Stats};
use crate::timer::Timer;
use crate::utils::{
    delete_commit_message_file, ensure_files, load_checkpoint, read_commit_message_file,
    reset_context_for_isolation, save_checkpoint, update_status, Logger, PipelineCheckpoint,
    PipelinePhase,
};
use std::env;
use std::process::Command;

pub fn run(args: Args) -> anyhow::Result<()> {
    let colors = Colors::new();
    let mut logger = Logger::new(colors);

    // Load configuration
    let mut config = Config::from_env().with_commit_msg(args.commit_msg.clone());

    // Apply CLI arguments to config
    apply_args_to_config(&args, &mut config, &colors);

    // Handle --init-global flag: create global agents.toml if it doesn't exist and exit
    if args.init_global && handle_init_global(&colors)? {
        return Ok(());
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
    if args.init && handle_init(&colors, &agents_config_path)? {
        return Ok(());
    }

    // Check if agents.toml exists; if not, create it and prompt user
    if ensure_config_or_create(&colors, &agents_config_path, &logger)? {
        return Ok(());
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
        handle_list_agents(&registry);
        return Ok(());
    }

    if args.list_available_agents {
        handle_list_available_agents(&registry);
        return Ok(());
    }

    // --list-providers: Show OpenCode provider types and configuration
    if args.list_providers {
        handle_list_providers(&colors);
        return Ok(());
    }

    // --diagnose: Output comprehensive diagnostic information
    if args.diagnose {
        handle_diagnose(
            &colors,
            &config,
            &registry,
            &agents_config_path,
            &config_sources,
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
        return handle_dry_run(
            &logger,
            &colors,
            &config,
            &developer_agent,
            &reviewer_agent,
            &repo_root,
        );
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
        let mut timer = Timer::new();
        let mut stats = Stats::new();

        logger.info("Generating commit message...");

        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: &developer_agent,
            reviewer_agent: &reviewer_agent,
            review_guidelines: None,
        };
        generate_commit_message(&mut ctx)?;

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
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    cleanup_orphaned_marker(&logger)?;

    start_agent_phase(&mut git_helpers)?;
    let mut agent_phase_guard = AgentPhaseGuard::new(&mut git_helpers, &logger);

    let mut timer = Timer::new();
    let mut stats = Stats::new();

    // Welcome banner
    print_welcome_banner(&colors, &developer_agent, &reviewer_agent);
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

    let mut ctx = PhaseContext {
        config: &config,
        registry: &registry,
        logger: &logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent: &developer_agent,
        reviewer_agent: &reviewer_agent,
        review_guidelines: review_guidelines.as_ref(),
    };

    // Phase 1: Development (PROMPT → PLAN → Execute → Delete PLAN, repeated X times)
    logger.header("PHASE 1: Development", |c| c.blue());
    if resume_rank.is_some_and(|rank| rank >= phase_rank(PipelinePhase::Review)) {
        logger.info("Skipping development phase (checkpoint indicates it already completed)");
    } else if should_run_from(PipelinePhase::Planning) {
        let start_iter = match resume_phase {
            Some(PipelinePhase::Planning | PipelinePhase::Development) => resume_checkpoint
                .as_ref()
                .map(|c| c.iteration)
                .unwrap_or(1)
                .clamp(1, config.developer_iters),
            _ => 1,
        };
        let resuming_from_development =
            args.resume && resume_phase == Some(PipelinePhase::Development);
        let development_result =
            run_development_phase(&mut ctx, start_iter, resuming_from_development)?;
        if development_result.had_errors {
            logger.warn("Development phase completed with non-fatal errors");
        }
    } else {
        logger.info("Skipping development phase (resuming from a later checkpoint phase)");
    }

    update_status("In progress.", config.isolation_mode)?;

    // Phase 2: Reviewer review/fix cycle
    logger.header("PHASE 2: Review & Fix", |c| c.magenta());

    // Review-Fix cycles: N cycles means exactly N (review + fix) pairs
    // N=0 skips review entirely, N=1 is one review-fix cycle, N=2 is two cycles, etc.
    // For backward compatibility, also accept checkpoints at old Fix/ReviewAgain phases
    let run_any_reviewer_phase = should_run_from(PipelinePhase::Review)
        || should_run_from(PipelinePhase::Fix)
        || should_run_from(PipelinePhase::ReviewAgain)
        || should_run_from(PipelinePhase::CommitMessage);

    let should_run_review_phase = should_run_from(PipelinePhase::Review)
        || resume_phase == Some(PipelinePhase::Fix)
        || resume_phase == Some(PipelinePhase::ReviewAgain);
    if should_run_review_phase && config.reviewer_reviews > 0 {
        let start_pass = match resume_phase {
            Some(PipelinePhase::Review | PipelinePhase::Fix | PipelinePhase::ReviewAgain) => {
                resume_checkpoint
                    .as_ref()
                    .map(|c| c.reviewer_pass)
                    .unwrap_or(1)
                    .clamp(1, config.reviewer_reviews.max(1))
            }
            _ => 1,
        };

        let review_result = run_review_phase(&mut ctx, start_pass)?;
        if review_result.completed_early {
            logger.success("Review phase completed early (no issues found)");
        }
    } else if run_any_reviewer_phase && config.reviewer_reviews == 0 {
        logger.info("Skipping review phase (reviewer_reviews=0)");
    } else if run_any_reviewer_phase {
        logger.info("Skipping review-fix cycles (resuming from a later checkpoint phase)");
    }

    if should_run_from(PipelinePhase::CommitMessage) {
        generate_commit_message(&mut ctx)?;
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
    crate::git_helpers::end_agent_phase()?;
    crate::git_helpers::disable_git_wrapper(agent_phase_guard.git_helpers);
    if let Err(err) = crate::git_helpers::uninstall_hooks(&logger) {
        logger.warn(&format!("Failed to uninstall Ralph hooks: {}", err));
    }

    logger.header("PHASE 4: Commit Changes", |c| c.green());
    let commit_result = run_commit_phase(&logger, &colors)?;
    if commit_result.commit_created {
        logger.success(&format!(
            "Committed with message: {}{}{}",
            colors.cyan(),
            commit_result.commit_message,
            colors.reset()
        ));
    }

    // Final summary
    print_final_summary(&colors, &config, &timer, &stats, &logger);

    agent_phase_guard.disarm();
    Ok(())
}
