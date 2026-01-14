//! Application entrypoint and pipeline orchestration.
//!
//! This module exists to keep `src/main.rs` small and focused while preserving
//! the CLI surface and overall runtime behavior. It wires together:
//! - CLI/config parsing and plumbing commands
//! - Agent registry loading
//! - Repo setup and resume support
//! - Phase execution via `crate::phases`
//!
//! # Module Structure
//!
//! - [`config_init`]: Configuration loading and agent registry initialization
//! - [`plumbing`]: Low-level git operations (show/apply commit messages)
//! - [`validation`]: Agent validation and chain validation
//! - [`resume`]: Checkpoint resume functionality
//! - [`detection`]: Project stack detection
//! - [`finalization`]: Pipeline cleanup and finalization

#![expect(clippy::too_many_lines)]

pub mod config_init;
pub mod context;
pub mod detection;
pub mod finalization;
pub mod orchestrator;
pub mod phase_runners;
pub mod plumbing;
pub mod resume;
pub mod validation;

use crate::agents::AgentRegistry;
use crate::cli::{
    create_prompt_from_template, handle_diagnose, handle_dry_run, handle_list_agents,
    handle_list_available_agents, handle_list_providers, prompt_template_selection, Args,
};
use crate::colors::Colors;
use crate::files::{ensure_files, reset_context_for_isolation};
use crate::git_helpers::{get_repo_root, require_git_repo, reset_start_commit};
use crate::logger::Logger;
use std::env;

use config_init::initialize_config;
use context::PipelineContext;
use orchestrator::run_pipeline;
use plumbing::{handle_apply_commit, handle_generate_commit_msg, handle_show_commit_msg};
use validation::{
    resolve_required_agents, validate_agent_chains, validate_agent_commands, validate_can_commit,
};

/// Main application entry point.
///
/// Orchestrates the entire Ralph pipeline:
/// 1. Configuration initialization
/// 2. Agent validation
/// 3. Plumbing commands (if requested)
/// 4. Development phase
/// 5. Review & fix phase
/// 6. Final validation
/// 7. Commit phase
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if any phase fails.
pub fn run(args: Args) -> anyhow::Result<()> {
    let colors = Colors::new();
    let mut logger = Logger::new(colors);

    // Initialize configuration and agent registry
    let Some(init_result) = initialize_config(&args, colors, &logger)? else {
        return Ok(()); // Early exit (--init/--init-global/--init-legacy)
    };

    let config_init::ConfigInitResult {
        config,
        registry,
        config_path,
        config_sources,
    } = init_result;

    // Resolve required agent names
    let validated = resolve_required_agents(&config)?;
    let developer_agent = validated.developer_agent;
    let reviewer_agent = validated.reviewer_agent;

    // Get display names for UI/logging
    let developer_display = registry.display_name(&developer_agent);
    let reviewer_display = registry.display_name(&reviewer_agent);

    // Handle listing commands (these can run without git repo)
    if handle_listing_commands(&args, &registry, colors) {
        return Ok(());
    }

    // Handle --diagnose
    if args.diagnose {
        handle_diagnose(colors, &config, &registry, &config_path, &config_sources);
        return Ok(());
    }

    // Validate agent chains
    validate_agent_chains(&registry, colors);

    // Handle plumbing commands (these need git repo but not full validation)
    if args.show_commit_msg {
        return handle_show_commit_msg();
    }
    if args.apply_commit {
        return handle_apply_commit(&logger, colors);
    }
    if args.reset_start_commit {
        require_git_repo()?;
        let repo_root = get_repo_root()?;
        env::set_current_dir(&repo_root)?;

        match reset_start_commit() {
            Ok(()) => {
                logger.success("Starting commit reference reset to current HEAD");
                logger.info(".agent/start_commit has been updated");
                return Ok(());
            }
            Err(e) => {
                logger.error(&format!("Failed to reset starting commit: {e}"));
                anyhow::bail!("Failed to reset starting commit");
            }
        }
    }

    // Validate agent commands exist
    validate_agent_commands(
        &config,
        &registry,
        &developer_agent,
        &reviewer_agent,
        &config_path,
    )?;

    // Validate agents are workflow-capable
    validate_can_commit(
        &config,
        &registry,
        &developer_agent,
        &reviewer_agent,
        &config_path,
    )?;

    // Set up git repo and working directory
    require_git_repo()?;
    let repo_root = get_repo_root()?;
    env::set_current_dir(&repo_root)?;

    // In interactive mode, prompt to create PROMPT.md from a template BEFORE ensure_files().
    // If the user declines (or we can't prompt), exit without creating a placeholder PROMPT.md.
    if args.interactive && !std::path::Path::new("PROMPT.md").exists() {
        if let Some(template_name) = prompt_template_selection(colors) {
            create_prompt_from_template(&template_name, colors)?;
            println!();
            logger.info(
                "PROMPT.md created. Please edit it with your task details, then run ralph again.",
            );
            logger.info(&format!(
                "Tip: Edit PROMPT.md, then run: ralph \"{}\"",
                config.commit_msg
            ));
            return Ok(());
        }
        println!();
        logger.info("PROMPT.md is required to run the pipeline.");
        logger.info(
            "Create one with 'ralph --init-prompt <template>' (see: 'ralph --list-templates'), then rerun.",
        );
        return Ok(());
    }

    ensure_files(config.isolation_mode)?;

    // Reset context for isolation mode
    if config.isolation_mode {
        reset_context_for_isolation(&logger)?;
    }

    logger = logger.with_log_file(".agent/logs/pipeline.log");

    // Handle --dry-run
    if args.dry_run {
        return handle_dry_run(
            &logger,
            colors,
            &config,
            &developer_display,
            &reviewer_display,
            &repo_root,
        );
    }

    // Handle --generate-commit-msg
    if args.generate_commit_msg {
        return handle_generate_commit_msg(
            &config,
            &registry,
            &logger,
            colors,
            &developer_agent,
            &reviewer_agent,
        );
    }

    // Run the full pipeline
    run_pipeline(PipelineContext {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        developer_display,
        reviewer_display,
        repo_root,
        logger,
        colors,
    })
}

/// Handles listing commands that don't require the full pipeline.
///
/// Returns `true` if a listing command was handled and we should exit.
fn handle_listing_commands(args: &Args, registry: &AgentRegistry, colors: Colors) -> bool {
    if args.list_agents {
        handle_list_agents(registry);
        return true;
    }
    if args.list_available_agents {
        handle_list_available_agents(registry);
        return true;
    }
    if args.list_providers {
        handle_list_providers(colors);
        return true;
    }
    false
}
