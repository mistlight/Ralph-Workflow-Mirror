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

pub mod config_init;
pub mod context;
pub mod detection;
pub mod effect;
pub mod effect_handler;
pub mod effectful;
pub mod event_loop;
pub mod finalization;
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_effect_handler;
pub mod plumbing;
mod rebase;
pub mod resume;
pub mod validation;

use crate::agents::AgentRegistry;
use crate::app::finalization::finalize_pipeline;
use crate::banner::print_welcome_banner;
use crate::checkpoint::{
    save_checkpoint_with_workspace, CheckpointBuilder, PipelineCheckpoint, PipelinePhase,
};
use crate::cli::{
    create_prompt_from_template, handle_diagnose, handle_dry_run, handle_list_agents,
    handle_list_available_agents, handle_list_providers, handle_show_baseline,
    handle_template_commands, prompt_template_selection, Args,
};

use crate::executor::ProcessExecutor;
use crate::files::protection::monitoring::PromptMonitor;
use crate::files::{
    create_prompt_backup_with_workspace, make_prompt_read_only_with_workspace,
    update_status_with_workspace, validate_prompt_md_with_workspace,
};
use crate::git_helpers::{
    abort_rebase, continue_rebase, get_conflicted_files, is_main_or_master_branch,
    reset_start_commit, RebaseResult,
};
#[cfg(not(feature = "test-utils"))]
use crate::git_helpers::{
    cleanup_orphaned_marker, get_start_commit_summary, save_start_commit, start_agent_phase,
};
use crate::logger::Colors;
use crate::logger::Logger;
use crate::phases::PhaseContext;
use crate::pipeline::{AgentPhaseGuard, Stats, Timer};
use crate::prompts::template_context::TemplateContext;

use config_init::initialize_config;
use context::PipelineContext;
use detection::detect_project_stack;
use plumbing::handle_generate_commit_msg;
use rebase::{run_initial_rebase, run_rebase_to_default, try_resolve_conflicts_without_phase_ctx};
use resume::{handle_resume_with_validation, offer_resume_if_checkpoint_exists};
use validation::{
    resolve_required_agents, validate_agent_chains, validate_agent_commands, validate_can_commit,
};

fn discover_repo_root_for_workspace<H: effect::AppEffectHandler>(
    override_dir: Option<&std::path::Path>,
    handler: &mut H,
) -> anyhow::Result<std::path::PathBuf> {
    use effect::{AppEffect, AppEffectResult};

    if let Some(dir) = override_dir {
        match handler.execute(AppEffect::SetCurrentDir {
            path: dir.to_path_buf(),
        }) {
            AppEffectResult::Ok => {}
            AppEffectResult::Error(e) => anyhow::bail!(e),
            other => anyhow::bail!("unexpected result from SetCurrentDir: {:?}", other),
        }
    }

    match handler.execute(AppEffect::GitRequireRepo) {
        AppEffectResult::Ok => {}
        AppEffectResult::Error(e) => anyhow::bail!("Not in a git repository: {e}"),
        other => anyhow::bail!("unexpected result from GitRequireRepo: {:?}", other),
    }

    match handler.execute(AppEffect::GitGetRepoRoot) {
        AppEffectResult::Path(p) => Ok(p),
        AppEffectResult::Error(e) => anyhow::bail!("Failed to get repo root: {e}"),
        other => anyhow::bail!("unexpected result from GitGetRepoRoot: {:?}", other),
    }
}

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
/// * `executor` - Process executor for external process execution
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if any phase fails.
pub fn run(args: Args, executor: std::sync::Arc<dyn ProcessExecutor>) -> anyhow::Result<()> {
    let colors = Colors::new();
    let logger = Logger::new(colors);

    // Set working directory first if override is provided
    // This ensures all subsequent operations (including config init) use the correct directory
    if let Some(ref override_dir) = args.working_dir_override {
        std::env::set_current_dir(override_dir)?;
    }

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

    // Handle listing commands (these can run without git repo)
    if handle_listing_commands(&args, &registry, colors) {
        return Ok(());
    }

    // Handle --diagnose
    if args.recovery.diagnose {
        handle_diagnose(
            colors,
            &config,
            &registry,
            &config_path,
            &config_sources,
            &*executor,
        );
        return Ok(());
    }

    // Validate agent chains
    validate_agent_chains(&registry, colors);

    // Create effect handler for production operations
    let mut handler = effect_handler::RealAppEffectHandler::new();

    // Get repo root early for workspace creation (needed by plumbing commands)
    // This uses the same logic as setup_working_dir_via_handler but captures the repo_root.
    let early_repo_root =
        discover_repo_root_for_workspace(args.working_dir_override.as_deref(), &mut handler)?;

    // Create workspace for plumbing commands (and later for the full pipeline)
    let workspace: std::sync::Arc<dyn crate::workspace::Workspace> =
        std::sync::Arc::new(crate::workspace::WorkspaceFs::new(early_repo_root));

    // Handle plumbing commands with workspace support
    if handle_plumbing_commands(
        &args,
        &logger,
        colors,
        &mut handler,
        Some(workspace.as_ref()),
    )? {
        return Ok(());
    }

    // Validate agents and set up git repo and PROMPT.md
    // Note: repo_root is discovered again here (same as early_repo_root) but also
    // does additional setup like PROMPT.md creation that plumbing commands don't need
    let Some(repo_root) = validate_and_setup_agents(
        AgentSetupParams {
            config: &config,
            registry: &registry,
            developer_agent: &developer_agent,
            reviewer_agent: &reviewer_agent,
            config_path: &config_path,
            colors,
            logger: &logger,
            working_dir_override: args.working_dir_override.as_deref(),
        },
        &mut handler,
    )?
    else {
        return Ok(());
    };

    // Prepare pipeline context or exit early
    // Note: Reuse workspace created earlier (same repo root)
    (prepare_pipeline_or_exit(PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
        executor,
        handler: &mut handler,
        workspace,
    })?)
    .map_or_else(|| Ok(()), |ctx| run_pipeline(&ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::{AppEffect, AppEffectHandler, AppEffectResult};

    #[derive(Debug)]
    struct TestRepoRootHandler {
        captured: Vec<AppEffect>,
        repo_root: std::path::PathBuf,
    }

    impl TestRepoRootHandler {
        fn new(repo_root: std::path::PathBuf) -> Self {
            Self {
                captured: Vec::new(),
                repo_root,
            }
        }
    }

    impl AppEffectHandler for TestRepoRootHandler {
        fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
            self.captured.push(effect.clone());
            match effect {
                AppEffect::SetCurrentDir { .. } => AppEffectResult::Ok,
                AppEffect::GitRequireRepo => AppEffectResult::Ok,
                AppEffect::GitGetRepoRoot => AppEffectResult::Path(self.repo_root.clone()),
                other => panic!("unexpected effect in test handler: {other:?}"),
            }
        }
    }

    #[test]
    fn discover_repo_root_for_workspace_prefers_git_repo_root_over_override_dir() {
        let override_dir = std::path::PathBuf::from("/override/subdir");
        let repo_root = std::path::PathBuf::from("/repo");
        let mut handler = TestRepoRootHandler::new(repo_root.clone());

        let got = discover_repo_root_for_workspace(Some(&override_dir), &mut handler).unwrap();
        assert_eq!(got, repo_root);

        assert!(matches!(
            handler.captured.get(0),
            Some(AppEffect::SetCurrentDir { .. })
        ));
        assert!(handler
            .captured
            .iter()
            .any(|e| matches!(e, AppEffect::GitRequireRepo)));
        assert!(handler
            .captured
            .iter()
            .any(|e| matches!(e, AppEffect::GitGetRepoRoot)));
    }
}

/// Test-only entry point that accepts a pre-built Config.
///
/// This function is for integration testing only. It bypasses environment variable
/// loading and uses the provided Config directly, enabling deterministic tests
/// that don't rely on process-global state.
///
/// This function handles ALL commands including early-exit commands (--init, --diagnose,
/// --reset-start-commit, etc.) so that tests can use a single entry point.
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
/// * `executor` - Process executor for external process execution  
/// * `config` - Pre-built configuration (bypasses env var loading)
/// * `registry` - Pre-built agent registry
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if any phase fails.
#[cfg(feature = "test-utils")]
pub fn run_with_config(
    args: Args,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    config: crate::config::Config,
    registry: AgentRegistry,
) -> anyhow::Result<()> {
    // Use real path resolver and effect handler by default for backward compatibility
    let mut handler = effect_handler::RealAppEffectHandler::new();
    run_with_config_and_resolver(
        args,
        executor,
        config,
        registry,
        &crate::config::RealConfigEnvironment,
        &mut handler,
        None, // Use default WorkspaceFs
    )
}

/// Test-only entry point that accepts a pre-built Config and a custom path resolver.
///
/// This function is for integration testing only. It bypasses environment variable
/// loading and uses the provided Config and path resolver directly, enabling
/// deterministic tests that don't rely on process-global state or env vars.
///
/// This function handles ALL commands including early-exit commands (--init, --diagnose,
/// --reset-start-commit, etc.) so that tests can use a single entry point.
///
/// # Arguments
///
/// * `args` - The parsed CLI arguments
/// * `executor` - Process executor for external process execution
/// * `config` - Pre-built configuration (bypasses env var loading)
/// * `registry` - Pre-built agent registry
/// * `path_resolver` - Custom path resolver for init commands
/// * `handler` - Effect handler for git/filesystem operations
/// * `workspace` - Optional workspace for file operations (if `None`, uses `WorkspaceFs`)
///
/// # Returns
///
/// Returns `Ok(())` on success or an error if any phase fails.
#[cfg(feature = "test-utils")]
pub fn run_with_config_and_resolver<
    P: crate::config::ConfigEnvironment,
    H: effect::AppEffectHandler,
>(
    args: Args,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    config: crate::config::Config,
    registry: AgentRegistry,
    path_resolver: &P,
    handler: &mut H,
    workspace: Option<std::sync::Arc<dyn crate::workspace::Workspace>>,
) -> anyhow::Result<()> {
    use crate::cli::{
        handle_extended_help, handle_init_global_with, handle_init_prompt_with,
        handle_list_work_guides, handle_smart_init_with,
    };

    let colors = Colors::new();
    let logger = Logger::new(colors);

    // Set working directory first if override is provided
    if let Some(ref override_dir) = args.working_dir_override {
        std::env::set_current_dir(override_dir)?;
    }

    // Handle --extended-help / --man flag: display extended help and exit.
    if args.recovery.extended_help {
        handle_extended_help();
        if args.work_guide_list.list_work_guides {
            println!();
            handle_list_work_guides(colors);
        }
        return Ok(());
    }

    // Handle --list-work-guides / --list-templates flag
    if args.work_guide_list.list_work_guides && handle_list_work_guides(colors) {
        return Ok(());
    }

    // Handle --init-prompt flag: create PROMPT.md from template and exit
    if let Some(ref template_name) = args.init_prompt {
        if handle_init_prompt_with(
            template_name,
            args.unified_init.force_init,
            colors,
            path_resolver,
        )? {
            return Ok(());
        }
    }

    // Handle smart --init flag: intelligently determine what to initialize
    if args.unified_init.init.is_some()
        && handle_smart_init_with(
            args.unified_init.init.as_deref(),
            args.unified_init.force_init,
            colors,
            path_resolver,
        )?
    {
        return Ok(());
    }

    // Handle --init-config flag: explicit config creation and exit
    if args.unified_init.init_config && handle_init_global_with(colors, path_resolver)? {
        return Ok(());
    }

    // Handle --init-global flag: create unified config if it doesn't exist and exit
    if args.unified_init.init_global && handle_init_global_with(colors, path_resolver)? {
        return Ok(());
    }

    // Handle --init-legacy flag: legacy per-repo agents.toml creation and exit
    if args.legacy_init.init_legacy {
        let repo_root = match handler.execute(effect::AppEffect::GitGetRepoRoot) {
            effect::AppEffectResult::Path(p) => Some(p),
            _ => None,
        };
        let legacy_path = repo_root.map_or_else(
            || std::path::PathBuf::from(".agent/agents.toml"),
            |root| root.join(".agent/agents.toml"),
        );
        if crate::cli::handle_init_legacy(colors, &legacy_path)? {
            return Ok(());
        }
    }

    // Use provided config directly (no env var loading)
    let config_path = std::path::PathBuf::from("test-config");

    // Resolve required agent names
    let validated = resolve_required_agents(&config)?;
    let developer_agent = validated.developer_agent;
    let reviewer_agent = validated.reviewer_agent;

    // Handle listing commands (these can run without git repo)
    if handle_listing_commands(&args, &registry, colors) {
        return Ok(());
    }

    // Handle --diagnose
    if args.recovery.diagnose {
        handle_diagnose(colors, &config, &registry, &config_path, &[], &*executor);
        return Ok(());
    }

    // Handle plumbing commands (--reset-start-commit, --show-commit-msg, etc.)
    // Pass workspace reference for testability with MemoryWorkspace
    if handle_plumbing_commands(
        &args,
        &logger,
        colors,
        handler,
        workspace.as_ref().map(|w| w.as_ref()),
    )? {
        return Ok(());
    }

    // Validate agents and set up git repo and PROMPT.md
    let Some(repo_root) = validate_and_setup_agents(
        AgentSetupParams {
            config: &config,
            registry: &registry,
            developer_agent: &developer_agent,
            reviewer_agent: &reviewer_agent,
            config_path: &config_path,
            colors,
            logger: &logger,
            working_dir_override: args.working_dir_override.as_deref(),
        },
        handler,
    )?
    else {
        return Ok(());
    };

    // Create workspace for explicit path resolution, or use injected workspace
    let workspace = workspace.unwrap_or_else(|| {
        std::sync::Arc::new(crate::workspace::WorkspaceFs::new(repo_root.clone()))
    });

    // Prepare pipeline context or exit early
    (prepare_pipeline_or_exit(PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
        executor,
        handler,
        workspace,
    })?)
    .map_or_else(|| Ok(()), |ctx| run_pipeline(&ctx))
}

/// Parameters for `run_with_config_and_handlers`.
///
/// Groups related parameters to reduce function argument count.
#[cfg(feature = "test-utils")]
pub struct RunWithHandlersParams<'a, 'ctx, P, A, E>
where
    P: crate::config::ConfigEnvironment,
    A: effect::AppEffectHandler,
    E: crate::reducer::EffectHandler<'ctx> + crate::app::event_loop::StatefulHandler,
{
    pub args: Args,
    pub executor: std::sync::Arc<dyn ProcessExecutor>,
    pub config: crate::config::Config,
    pub registry: AgentRegistry,
    pub path_resolver: &'a P,
    pub app_handler: &'a mut A,
    pub effect_handler: &'a mut E,
    pub workspace: Option<std::sync::Arc<dyn crate::workspace::Workspace>>,
    /// Phantom data to bind the `'ctx` lifetime from `EffectHandler<'ctx>`.
    pub _marker: std::marker::PhantomData<&'ctx ()>,
}

/// Run with both AppEffectHandler AND EffectHandler for full isolation.
///
/// This function is the ultimate test entry point that allows injecting BOTH:
/// - `AppEffectHandler` for CLI-layer operations (git require repo, set cwd, etc.)
/// - `EffectHandler` for reducer-layer operations (create commit, run rebase, etc.)
///
/// Using both handlers ensures tests make ZERO real git calls at any layer.
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
/// use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
///
/// let mut app_handler = MockAppEffectHandler::new().with_head_oid("abc123");
/// let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 0));
///
/// run_with_config_and_handlers(RunWithHandlersParams {
///     args, executor, config, registry, path_resolver: &env,
///     app_handler: &mut app_handler, effect_handler: &mut effect_handler,
///     workspace: None,
/// })?;
///
/// // Verify no real git operations at either layer
/// assert!(app_handler.captured().iter().any(|e| matches!(e, AppEffect::GitRequireRepo)));
/// assert!(effect_handler.captured_effects().iter().any(|e| matches!(e, Effect::CreateCommit { .. })));
/// ```
#[cfg(feature = "test-utils")]
pub fn run_with_config_and_handlers<'a, 'ctx, P, A, E>(
    params: RunWithHandlersParams<'a, 'ctx, P, A, E>,
) -> anyhow::Result<()>
where
    P: crate::config::ConfigEnvironment,
    A: effect::AppEffectHandler,
    E: crate::reducer::EffectHandler<'ctx> + crate::app::event_loop::StatefulHandler,
{
    let RunWithHandlersParams {
        args,
        executor,
        config,
        registry,
        path_resolver,
        app_handler,
        effect_handler,
        workspace,
        ..
    } = params;
    use crate::cli::{
        handle_extended_help, handle_init_global_with, handle_init_prompt_with,
        handle_list_work_guides, handle_smart_init_with,
    };

    let colors = Colors::new();
    let logger = Logger::new(colors);

    // Set working directory first if override is provided
    if let Some(ref override_dir) = args.working_dir_override {
        std::env::set_current_dir(override_dir)?;
    }

    // Handle --extended-help / --man flag
    if args.recovery.extended_help {
        handle_extended_help();
        if args.work_guide_list.list_work_guides {
            println!();
            handle_list_work_guides(colors);
        }
        return Ok(());
    }

    // Handle --list-work-guides / --list-templates flag
    if args.work_guide_list.list_work_guides && handle_list_work_guides(colors) {
        return Ok(());
    }

    // Handle --init-prompt flag
    if let Some(ref template_name) = args.init_prompt {
        if handle_init_prompt_with(
            template_name,
            args.unified_init.force_init,
            colors,
            path_resolver,
        )? {
            return Ok(());
        }
    }

    // Handle smart --init flag
    if args.unified_init.init.is_some()
        && handle_smart_init_with(
            args.unified_init.init.as_deref(),
            args.unified_init.force_init,
            colors,
            path_resolver,
        )?
    {
        return Ok(());
    }

    // Handle --init-config flag
    if args.unified_init.init_config && handle_init_global_with(colors, path_resolver)? {
        return Ok(());
    }

    // Handle --init-global flag
    if args.unified_init.init_global && handle_init_global_with(colors, path_resolver)? {
        return Ok(());
    }

    // Handle --init-legacy flag
    if args.legacy_init.init_legacy {
        let repo_root = match app_handler.execute(effect::AppEffect::GitGetRepoRoot) {
            effect::AppEffectResult::Path(p) => Some(p),
            _ => None,
        };
        let legacy_path = repo_root.map_or_else(
            || std::path::PathBuf::from(".agent/agents.toml"),
            |root| root.join(".agent/agents.toml"),
        );
        if crate::cli::handle_init_legacy(colors, &legacy_path)? {
            return Ok(());
        }
    }

    // Use provided config directly
    let config_path = std::path::PathBuf::from("test-config");

    // Resolve required agent names
    let validated = resolve_required_agents(&config)?;
    let developer_agent = validated.developer_agent;
    let reviewer_agent = validated.reviewer_agent;

    // Handle listing commands
    if handle_listing_commands(&args, &registry, colors) {
        return Ok(());
    }

    // Handle --diagnose
    if args.recovery.diagnose {
        handle_diagnose(colors, &config, &registry, &config_path, &[], &*executor);
        return Ok(());
    }

    // Handle plumbing commands with app_handler
    // Pass workspace reference for testability with MemoryWorkspace
    if handle_plumbing_commands(
        &args,
        &logger,
        colors,
        app_handler,
        workspace.as_ref().map(|w| w.as_ref()),
    )? {
        return Ok(());
    }

    // Validate agents and set up git repo with app_handler
    let Some(repo_root) = validate_and_setup_agents(
        AgentSetupParams {
            config: &config,
            registry: &registry,
            developer_agent: &developer_agent,
            reviewer_agent: &reviewer_agent,
            config_path: &config_path,
            colors,
            logger: &logger,
            working_dir_override: args.working_dir_override.as_deref(),
        },
        app_handler,
    )?
    else {
        return Ok(());
    };

    // Create workspace for explicit path resolution, or use injected workspace
    let workspace = workspace.unwrap_or_else(|| {
        std::sync::Arc::new(crate::workspace::WorkspaceFs::new(repo_root.clone()))
    });

    // Prepare pipeline context or exit early
    let ctx = prepare_pipeline_or_exit(PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        logger,
        colors,
        executor,
        handler: app_handler,
        workspace,
    })?;

    // Run pipeline with the injected effect_handler
    match ctx {
        Some(ctx) => run_pipeline_with_effect_handler(&ctx, effect_handler),
        None => Ok(()),
    }
}

/// Handles listing commands that don't require the full pipeline.
///
/// Returns `true` if a listing command was handled and we should exit.
fn handle_listing_commands(args: &Args, registry: &AgentRegistry, colors: Colors) -> bool {
    if args.agent_list.list_agents {
        handle_list_agents(registry);
        return true;
    }
    if args.agent_list.list_available_agents {
        handle_list_available_agents(registry);
        return true;
    }
    if args.provider_list.list_providers {
        handle_list_providers(colors);
        return true;
    }

    // Handle template commands
    let template_cmds = &args.template_commands;
    if template_cmds.init_templates_enabled()
        || template_cmds.validate
        || template_cmds.show.is_some()
        || template_cmds.list
        || template_cmds.list_all
        || template_cmds.variables.is_some()
        || template_cmds.render.is_some()
    {
        let _ = handle_template_commands(template_cmds, colors);
        return true;
    }

    false
}

/// Handles plumbing commands that require git repo but not full validation.
///
/// Returns `Ok(true)` if a plumbing command was handled and we should exit.
/// Returns `Ok(false)` if we should continue to the main pipeline.
///
/// # Workspace Support
///
/// When `workspace` is `Some`, the workspace-aware versions of plumbing commands
/// are used, enabling testing with `MemoryWorkspace`. When `None`, the direct
/// filesystem versions are used (production behavior).
fn handle_plumbing_commands<H: effect::AppEffectHandler>(
    args: &Args,
    logger: &Logger,
    colors: Colors,
    handler: &mut H,
    workspace: Option<&dyn crate::workspace::Workspace>,
) -> anyhow::Result<bool> {
    use plumbing::{handle_apply_commit_with_handler, handle_show_commit_msg_with_workspace};

    // Helper to set up working directory for plumbing commands using the effect handler
    fn setup_working_dir_via_handler<H: effect::AppEffectHandler>(
        override_dir: Option<&std::path::Path>,
        handler: &mut H,
    ) -> anyhow::Result<()> {
        use effect::{AppEffect, AppEffectResult};

        if let Some(dir) = override_dir {
            match handler.execute(AppEffect::SetCurrentDir {
                path: dir.to_path_buf(),
            }) {
                AppEffectResult::Ok => Ok(()),
                AppEffectResult::Error(e) => anyhow::bail!(e),
                other => anyhow::bail!("unexpected result from SetCurrentDir: {:?}", other),
            }
        } else {
            // Require git repo
            match handler.execute(AppEffect::GitRequireRepo) {
                AppEffectResult::Ok => {}
                AppEffectResult::Error(e) => anyhow::bail!(e),
                other => anyhow::bail!("unexpected result from GitRequireRepo: {:?}", other),
            }
            // Get repo root
            let repo_root = match handler.execute(AppEffect::GitGetRepoRoot) {
                AppEffectResult::Path(p) => p,
                AppEffectResult::Error(e) => anyhow::bail!(e),
                other => anyhow::bail!("unexpected result from GitGetRepoRoot: {:?}", other),
            };
            // Set current dir to repo root
            match handler.execute(AppEffect::SetCurrentDir { path: repo_root }) {
                AppEffectResult::Ok => Ok(()),
                AppEffectResult::Error(e) => anyhow::bail!(e),
                other => anyhow::bail!("unexpected result from SetCurrentDir: {:?}", other),
            }
        }
    }

    // Show commit message
    if args.commit_display.show_commit_msg {
        setup_working_dir_via_handler(args.working_dir_override.as_deref(), handler)?;
        let ws = workspace.ok_or_else(|| {
            anyhow::anyhow!(
                "--show-commit-msg requires workspace context. Run this command after the pipeline has initialized."
            )
        })?;
        return handle_show_commit_msg_with_workspace(ws).map(|()| true);
    }

    // Apply commit
    if args.commit_plumbing.apply_commit {
        setup_working_dir_via_handler(args.working_dir_override.as_deref(), handler)?;
        let ws = workspace.ok_or_else(|| {
            anyhow::anyhow!(
                "--apply-commit requires workspace context. Run this command after the pipeline has initialized."
            )
        })?;
        return handle_apply_commit_with_handler(ws, handler, logger, colors).map(|()| true);
    }

    // Reset start commit
    if args.commit_display.reset_start_commit {
        setup_working_dir_via_handler(args.working_dir_override.as_deref(), handler)?;

        // Use the effect handler for reset_start_commit
        return match handler.execute(effect::AppEffect::GitResetStartCommit) {
            effect::AppEffectResult::String(oid) => {
                // Simple case - just got the OID back
                let short_oid = &oid[..8.min(oid.len())];
                logger.success(&format!("Starting commit reference reset ({})", short_oid));
                logger.info(".agent/start_commit has been updated");
                Ok(true)
            }
            effect::AppEffectResult::Error(e) => {
                logger.error(&format!("Failed to reset starting commit: {e}"));
                anyhow::bail!("Failed to reset starting commit");
            }
            other => {
                // Fallback to old implementation for other result types
                // This allows gradual migration
                drop(other);
                match reset_start_commit() {
                    Ok(result) => {
                        let short_oid = &result.oid[..8.min(result.oid.len())];
                        if result.fell_back_to_head {
                            logger.success(&format!(
                                "Starting commit reference reset to current HEAD ({})",
                                short_oid
                            ));
                            logger.info("On main/master branch - using HEAD as baseline");
                        } else if let Some(ref branch) = result.default_branch {
                            logger.success(&format!(
                                "Starting commit reference reset to merge-base with '{}' ({})",
                                branch, short_oid
                            ));
                            logger.info("Baseline set to common ancestor with default branch");
                        } else {
                            logger.success(&format!(
                                "Starting commit reference reset ({})",
                                short_oid
                            ));
                        }
                        logger.info(".agent/start_commit has been updated");
                        Ok(true)
                    }
                    Err(e) => {
                        logger.error(&format!("Failed to reset starting commit: {e}"));
                        anyhow::bail!("Failed to reset starting commit");
                    }
                }
            }
        };
    }

    // Show baseline state
    if args.commit_display.show_baseline {
        setup_working_dir_via_handler(args.working_dir_override.as_deref(), handler)?;

        return match handle_show_baseline() {
            Ok(()) => Ok(true),
            Err(e) => {
                logger.error(&format!("Failed to show baseline: {e}"));
                anyhow::bail!("Failed to show baseline");
            }
        };
    }

    Ok(false)
}

/// Parameters for preparing the pipeline context.
///
/// Groups related parameters to avoid too many function arguments.
struct PipelinePreparationParams<'a, H: effect::AppEffectHandler> {
    args: Args,
    config: crate::config::Config,
    registry: AgentRegistry,
    developer_agent: String,
    reviewer_agent: String,
    repo_root: std::path::PathBuf,
    logger: Logger,
    colors: Colors,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    handler: &'a mut H,
    /// Workspace for explicit path resolution.
    ///
    /// Production code passes `Arc::new(WorkspaceFs::new(...))`.
    /// Tests can pass `Arc::new(MemoryWorkspace::new(...))`.
    workspace: std::sync::Arc<dyn crate::workspace::Workspace>,
}

/// Prepares the pipeline context after agent validation.
///
/// Returns `Some(ctx)` if pipeline should run, or `None` if we should exit early.
fn prepare_pipeline_or_exit<H: effect::AppEffectHandler>(
    params: PipelinePreparationParams<'_, H>,
) -> anyhow::Result<Option<PipelineContext>> {
    let PipelinePreparationParams {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        repo_root,
        mut logger,
        colors,
        executor,
        handler,
        workspace,
    } = params;

    // Ensure required files and directories exist via effects
    effectful::ensure_files_effectful(handler, config.isolation_mode)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Reset context for isolation mode via effects
    if config.isolation_mode {
        effectful::reset_context_for_isolation_effectful(handler)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    logger = logger.with_log_file(".agent/logs/pipeline.log");

    // Handle --dry-run
    if args.recovery.dry_run {
        let developer_display = registry.display_name(&developer_agent);
        let reviewer_display = registry.display_name(&reviewer_agent);
        handle_dry_run(
            &logger,
            colors,
            &config,
            &developer_display,
            &reviewer_display,
            &repo_root,
        )?;
        return Ok(None);
    }

    // Create template context for user template overrides
    let template_context =
        TemplateContext::from_user_templates_dir(config.user_templates_dir().cloned());

    // Handle --rebase-only
    if args.rebase_flags.rebase_only {
        handle_rebase_only(
            &args,
            &config,
            &template_context,
            &logger,
            colors,
            std::sync::Arc::clone(&executor),
            &repo_root,
        )?;
        return Ok(None);
    }

    // Handle --generate-commit-msg
    if args.commit_plumbing.generate_commit_msg {
        handle_generate_commit_msg(plumbing::CommitGenerationConfig {
            config: &config,
            template_context: &template_context,
            workspace: &*workspace,
            registry: &registry,
            logger: &logger,
            colors,
            developer_agent: &developer_agent,
            _reviewer_agent: &reviewer_agent,
            executor: std::sync::Arc::clone(&executor),
        })?;
        return Ok(None);
    }

    // Get display names before moving registry
    let developer_display = registry.display_name(&developer_agent);
    let reviewer_display = registry.display_name(&reviewer_agent);

    // Build pipeline context (workspace was injected via params)
    let ctx = PipelineContext {
        args,
        config,
        registry,
        developer_agent,
        reviewer_agent,
        developer_display,
        reviewer_display,
        repo_root,
        workspace,
        logger,
        colors,
        template_context,
        executor,
    };
    Ok(Some(ctx))
}

/// Parameters for agent validation and setup.
struct AgentSetupParams<'a> {
    config: &'a crate::config::Config,
    registry: &'a AgentRegistry,
    developer_agent: &'a str,
    reviewer_agent: &'a str,
    config_path: &'a std::path::Path,
    colors: Colors,
    logger: &'a Logger,
    /// If Some, use this path as the working directory without discovering the repo root
    /// or changing the global CWD. This enables test parallelism.
    working_dir_override: Option<&'a std::path::Path>,
}

/// Validates agent commands and workflow capability, then sets up git repo and PROMPT.md.
///
/// Returns `Some(repo_root)` if setup succeeded and should continue.
/// Returns `None` if the user declined PROMPT.md creation (to exit early).
fn validate_and_setup_agents<H: effect::AppEffectHandler>(
    params: AgentSetupParams<'_>,
    handler: &mut H,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    let AgentSetupParams {
        config,
        registry,
        developer_agent,
        reviewer_agent,
        config_path,
        colors,
        logger,
        working_dir_override,
    } = params;
    // Validate agent commands exist
    validate_agent_commands(
        config,
        registry,
        developer_agent,
        reviewer_agent,
        config_path,
    )?;

    // Validate agents are workflow-capable
    validate_can_commit(
        config,
        registry,
        developer_agent,
        reviewer_agent,
        config_path,
    )?;

    // Determine repo root - use override if provided (for testing), otherwise discover
    let repo_root = if let Some(override_dir) = working_dir_override {
        // Testing mode: use provided directory and change CWD to it via handler
        handler.execute(effect::AppEffect::SetCurrentDir {
            path: override_dir.to_path_buf(),
        });
        override_dir.to_path_buf()
    } else {
        // Production mode: discover repo root and change CWD via handler
        let require_result = handler.execute(effect::AppEffect::GitRequireRepo);
        if let effect::AppEffectResult::Error(e) = require_result {
            anyhow::bail!("Not in a git repository: {}", e);
        }

        let root_result = handler.execute(effect::AppEffect::GitGetRepoRoot);
        let root = match root_result {
            effect::AppEffectResult::Path(p) => p,
            effect::AppEffectResult::Error(e) => {
                anyhow::bail!("Failed to get repo root: {}", e);
            }
            _ => anyhow::bail!("Unexpected result from GitGetRepoRoot"),
        };

        handler.execute(effect::AppEffect::SetCurrentDir { path: root.clone() });
        root
    };

    // Set up PROMPT.md if needed (may return None to exit early)
    let should_continue = setup_git_and_prompt_file(config, colors, logger, handler)?;
    if should_continue.is_none() {
        return Ok(None);
    }

    Ok(Some(repo_root))
}

/// In interactive mode, prompts to create PROMPT.md from a template before `ensure_files()`.
///
/// Returns `Ok(Some(()))` if setup succeeded and should continue.
/// Returns `Ok(None)` if the user declined PROMPT.md creation (to exit early).
fn setup_git_and_prompt_file<H: effect::AppEffectHandler>(
    config: &crate::config::Config,
    colors: Colors,
    logger: &Logger,
    handler: &mut H,
) -> anyhow::Result<Option<()>> {
    let prompt_exists =
        effectful::check_prompt_exists_effectful(handler).map_err(|e| anyhow::anyhow!("{}", e))?;

    // In interactive mode, prompt to create PROMPT.md from a template BEFORE ensure_files().
    // If the user declines (or we can't prompt), exit without creating a placeholder PROMPT.md.
    if config.behavior.interactive && !prompt_exists {
        if let Some(template_name) = prompt_template_selection(colors) {
            create_prompt_from_template(&template_name, colors)?;
            println!();
            logger.info(
                "PROMPT.md created. Please edit it with your task details, then run ralph again.",
            );
            logger.info("Tip: Edit PROMPT.md, then run: ralph");
            return Ok(None);
        }
        println!();
        logger.error("PROMPT.md not found in current directory.");
        logger.warn("PROMPT.md is required to run the Ralph pipeline.");
        println!();
        logger.info("To get started:");
        logger.info("  ralph --init                    # Smart setup wizard");
        logger.info("  ralph --init bug-fix             # Create from Work Guide");
        logger.info("  ralph --list-work-guides          # See all Work Guides");
        println!();
        return Ok(None);
    }

    // Non-interactive mode: show helpful error if PROMPT.md doesn't exist
    if !prompt_exists {
        logger.error("PROMPT.md not found in current directory.");
        logger.warn("PROMPT.md is required to run the Ralph pipeline.");
        println!();
        logger.info("Quick start:");
        logger.info("  ralph --init                    # Smart setup wizard");
        logger.info("  ralph --init bug-fix             # Create from Work Guide");
        logger.info("  ralph --list-work-guides          # See all Work Guides");
        println!();
        logger.info("Use -i flag for interactive mode to be prompted for template selection.");
        println!();
        return Ok(None);
    }

    Ok(Some(()))
}

/// Runs the full development/review/commit pipeline using reducer-based event loop.
fn run_pipeline(ctx: &PipelineContext) -> anyhow::Result<()> {
    // Use MainEffectHandler for production
    run_pipeline_with_default_handler(ctx)
}

/// Runs the pipeline with the default MainEffectHandler.
///
/// This is the production entry point - it creates a MainEffectHandler internally.
fn run_pipeline_with_default_handler(ctx: &PipelineContext) -> anyhow::Result<()> {
    use crate::app::event_loop::EventLoopConfig;
    #[cfg(not(feature = "test-utils"))]
    use crate::reducer::MainEffectHandler;
    use crate::reducer::PipelineState;

    // First, offer interactive resume if checkpoint exists without --resume flag
    let resume_result = offer_resume_if_checkpoint_exists(
        &ctx.args,
        &ctx.config,
        &ctx.registry,
        &ctx.logger,
        &ctx.developer_agent,
        &ctx.reviewer_agent,
    );

    // If interactive resume didn't happen, check for --resume flag
    let resume_result = match resume_result {
        Some(result) => Some(result),
        None => handle_resume_with_validation(
            &ctx.args,
            &ctx.config,
            &ctx.registry,
            &ctx.logger,
            &ctx.developer_display,
            &ctx.reviewer_display,
        ),
    };

    let resume_checkpoint = resume_result.map(|r| r.checkpoint);

    // Create run context - either new or from checkpoint
    let run_context = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::RunContext;
        RunContext::from_checkpoint(checkpoint)
    } else {
        use crate::checkpoint::RunContext;
        RunContext::new()
    };

    // Apply checkpoint configuration restoration if resuming
    let config = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::apply_checkpoint_to_config;
        let mut restored_config = ctx.config.clone();
        apply_checkpoint_to_config(&mut restored_config, checkpoint);
        ctx.logger.info("Restored configuration from checkpoint:");
        if checkpoint.cli_args.developer_iters > 0 {
            ctx.logger.info(&format!(
                "  Developer iterations: {} (from checkpoint)",
                checkpoint.cli_args.developer_iters
            ));
        }
        if checkpoint.cli_args.reviewer_reviews > 0 {
            ctx.logger.info(&format!(
                "  Reviewer passes: {} (from checkpoint)",
                checkpoint.cli_args.reviewer_reviews
            ));
        }
        restored_config
    } else {
        ctx.config.clone()
    };

    // Restore environment variables from checkpoint if resuming
    if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::restore::restore_environment_from_checkpoint;
        let restored_count = restore_environment_from_checkpoint(checkpoint);
        if restored_count > 0 {
            ctx.logger.info(&format!(
                "  Restored {} environment variable(s) from checkpoint",
                restored_count
            ));
        }
    }

    // Set up git helpers and agent phase
    // Use workspace-aware versions when test-utils feature is enabled
    // to avoid real git operations that would cause test failures.
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    #[cfg(feature = "test-utils")]
    {
        use crate::git_helpers::{
            cleanup_orphaned_marker_with_workspace, create_marker_with_workspace,
        };
        // Use workspace-based operations that don't require real git
        cleanup_orphaned_marker_with_workspace(&*ctx.workspace, &ctx.logger)?;
        create_marker_with_workspace(&*ctx.workspace)?;
        // Skip hook installation and git wrapper in test mode
    }
    #[cfg(not(feature = "test-utils"))]
    {
        cleanup_orphaned_marker(&ctx.logger)?;
        start_agent_phase(&mut git_helpers)?;
    }
    let mut agent_phase_guard =
        AgentPhaseGuard::new(&mut git_helpers, &ctx.logger, &*ctx.workspace);

    // Print welcome banner and validate PROMPT.md
    print_welcome_banner(ctx.colors, &ctx.developer_display, &ctx.reviewer_display);
    print_pipeline_info_with_config(ctx, &config);
    validate_prompt_and_setup_backup(ctx)?;

    // Set up PROMPT.md monitoring
    let mut prompt_monitor = setup_prompt_monitor(ctx);

    // Detect project stack and review guidelines
    let (_project_stack, review_guidelines) =
        detect_project_stack(&config, &ctx.repo_root, &ctx.logger, ctx.colors);

    print_review_guidelines(ctx, review_guidelines.as_ref());
    println!();

    // Create phase context and save starting commit
    let (mut timer, mut stats) = (Timer::new(), Stats::new());
    let mut phase_ctx = create_phase_context_with_config(
        ctx,
        &config,
        &mut timer,
        &mut stats,
        review_guidelines.as_ref(),
        &run_context,
        resume_checkpoint.as_ref(),
    );
    save_start_commit_or_warn(ctx);

    // Set up interrupt context for checkpoint saving on Ctrl+C
    // This must be done after phase_ctx is created
    let initial_phase = if let Some(ref checkpoint) = resume_checkpoint {
        checkpoint.phase
    } else {
        PipelinePhase::Planning
    };
    setup_interrupt_context_for_pipeline(
        initial_phase,
        config.developer_iters,
        config.reviewer_reviews,
        &phase_ctx.execution_history,
        &phase_ctx.prompt_history,
        &run_context,
    );

    // Ensure interrupt context is cleared on completion
    let _interrupt_guard = defer_clear_interrupt_context();

    // Determine if we should run rebase based on checkpoint or current args
    let should_run_rebase = if let Some(ref checkpoint) = resume_checkpoint {
        // Use checkpoint's skip_rebase value if it has meaningful cli_args
        if checkpoint.cli_args.developer_iters > 0 || checkpoint.cli_args.reviewer_reviews > 0 {
            !checkpoint.cli_args.skip_rebase
        } else {
            // Fallback to current args
            ctx.args.rebase_flags.with_rebase
        }
    } else {
        ctx.args.rebase_flags.with_rebase
    };

    // Run pre-development rebase (only if explicitly requested via --with-rebase)
    if should_run_rebase {
        run_initial_rebase(ctx, &mut phase_ctx, &run_context, &*ctx.executor)?;
        // Update interrupt context after rebase
        update_interrupt_context_from_phase(
            &phase_ctx,
            PipelinePhase::Planning,
            config.developer_iters,
            config.reviewer_reviews,
            &run_context,
        );
    } else {
        // Save initial checkpoint when rebase is disabled
        if config.features.checkpoint_enabled && resume_checkpoint.is_none() {
            let builder = CheckpointBuilder::new()
                .phase(PipelinePhase::Planning, 0, config.developer_iters)
                .reviewer_pass(0, config.reviewer_reviews)
                .skip_rebase(true) // Rebase is disabled
                .capture_from_context(
                    &config,
                    &ctx.registry,
                    &ctx.developer_agent,
                    &ctx.reviewer_agent,
                    &ctx.logger,
                    &run_context,
                )
                .with_executor_from_context(std::sync::Arc::clone(&ctx.executor))
                .with_execution_history(phase_ctx.execution_history.clone())
                .with_prompt_history(phase_ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
            }
        }
        // Update interrupt context after initial checkpoint
        update_interrupt_context_from_phase(
            &phase_ctx,
            PipelinePhase::Planning,
            config.developer_iters,
            config.reviewer_reviews,
            &run_context,
        );
    }

    // ============================================
    // RUN PIPELINE PHASES VIA REDUCER EVENT LOOP
    // ============================================

    // Initialize pipeline state
    let initial_state = if let Some(ref checkpoint) = resume_checkpoint {
        // Migrate from old checkpoint format to new reducer state
        PipelineState::from(checkpoint.clone())
    } else {
        // Create new initial state
        PipelineState::initial(config.developer_iters, config.reviewer_reviews)
    };

    // Configure event loop
    let event_loop_config = EventLoopConfig {
        max_iterations: event_loop::MAX_EVENT_LOOP_ITERATIONS,
        enable_checkpointing: config.features.checkpoint_enabled,
    };

    // Clone execution_history and prompt_history BEFORE running event loop (to avoid borrow issues)
    let execution_history_before = phase_ctx.execution_history.clone();
    let prompt_history_before = phase_ctx.clone_prompt_history();

    // Create effect handler and run event loop.
    // Under test-utils feature, use MockEffectHandler to avoid real git operations.
    #[cfg(feature = "test-utils")]
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        use crate::reducer::mock_effect_handler::MockEffectHandler;
        let mut handler = MockEffectHandler::new(initial_state.clone());
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            &mut handler,
        )
    };
    #[cfg(not(feature = "test-utils"))]
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        let mut handler = MainEffectHandler::new(initial_state.clone());
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            &mut handler,
        )
    };

    // Handle event loop result
    let loop_result = loop_result?;
    if loop_result.completed {
        ctx.logger
            .success("Pipeline completed successfully via reducer event loop");
        ctx.logger.info(&format!(
            "Total events processed: {}",
            loop_result.events_processed
        ));
    } else {
        ctx.logger.warn("Pipeline exited without completion marker");
    }

    // Save Complete checkpoint before clearing (for idempotent resume)
    if config.features.checkpoint_enabled {
        let skip_rebase = !ctx.args.rebase_flags.with_rebase;
        let builder = CheckpointBuilder::new()
            .phase(
                PipelinePhase::Complete,
                config.developer_iters,
                config.developer_iters,
            )
            .reviewer_pass(config.reviewer_reviews, config.reviewer_reviews)
            .skip_rebase(skip_rebase)
            .capture_from_context(
                &config,
                &ctx.registry,
                &ctx.developer_agent,
                &ctx.reviewer_agent,
                &ctx.logger,
                &run_context,
            )
            .with_executor_from_context(std::sync::Arc::clone(&ctx.executor));

        let builder = builder
            .with_execution_history(execution_history_before)
            .with_prompt_history(prompt_history_before);

        if let Some(checkpoint) = builder.build() {
            let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
        }
    }

    // Post-pipeline operations
    check_prompt_restoration(ctx, &mut prompt_monitor, "event loop");
    update_status_with_workspace(&*ctx.workspace, "In progress.", config.isolation_mode)?;

    // Commit phase
    finalize_pipeline(
        &mut agent_phase_guard,
        &ctx.logger,
        ctx.colors,
        &config,
        finalization::RuntimeStats {
            timer: &timer,
            stats: &stats,
        },
        prompt_monitor,
        Some(&*ctx.workspace),
    );
    Ok(())
}

/// Runs the pipeline with a custom effect handler for testing.
///
/// This function is only available with the `test-utils` feature and allows
/// injecting a `MockEffectHandler` to prevent real git operations during tests.
///
/// # Arguments
///
/// * `ctx` - Pipeline context
/// * `effect_handler` - Custom effect handler (e.g., `MockEffectHandler`)
///
/// # Type Parameters
///
/// * `H` - Effect handler type that implements `EffectHandler` and `StatefulHandler`
#[cfg(feature = "test-utils")]
pub fn run_pipeline_with_effect_handler<'ctx, H>(
    ctx: &PipelineContext,
    effect_handler: &mut H,
) -> anyhow::Result<()>
where
    H: crate::reducer::EffectHandler<'ctx> + crate::app::event_loop::StatefulHandler,
{
    use crate::app::event_loop::EventLoopConfig;
    use crate::reducer::PipelineState;

    // First, offer interactive resume if checkpoint exists without --resume flag
    let resume_result = offer_resume_if_checkpoint_exists(
        &ctx.args,
        &ctx.config,
        &ctx.registry,
        &ctx.logger,
        &ctx.developer_agent,
        &ctx.reviewer_agent,
    );

    // If interactive resume didn't happen, check for --resume flag
    let resume_result = match resume_result {
        Some(result) => Some(result),
        None => handle_resume_with_validation(
            &ctx.args,
            &ctx.config,
            &ctx.registry,
            &ctx.logger,
            &ctx.developer_display,
            &ctx.reviewer_display,
        ),
    };

    let resume_checkpoint = resume_result.map(|r| r.checkpoint);

    // Create run context - either new or from checkpoint
    let run_context = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::RunContext;
        RunContext::from_checkpoint(checkpoint)
    } else {
        use crate::checkpoint::RunContext;
        RunContext::new()
    };

    // Apply checkpoint configuration restoration if resuming
    let config = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::apply_checkpoint_to_config;
        let mut restored_config = ctx.config.clone();
        apply_checkpoint_to_config(&mut restored_config, checkpoint);
        restored_config
    } else {
        ctx.config.clone()
    };

    // Set up git helpers and agent phase
    // Use workspace-aware versions when test-utils feature is enabled
    // to avoid real git operations that would cause test failures.
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    #[cfg(feature = "test-utils")]
    {
        use crate::git_helpers::{
            cleanup_orphaned_marker_with_workspace, create_marker_with_workspace,
        };
        // Use workspace-based operations that don't require real git
        cleanup_orphaned_marker_with_workspace(&*ctx.workspace, &ctx.logger)?;
        create_marker_with_workspace(&*ctx.workspace)?;
        // Skip hook installation and git wrapper in test mode
    }
    #[cfg(not(feature = "test-utils"))]
    {
        cleanup_orphaned_marker(&ctx.logger)?;
        start_agent_phase(&mut git_helpers)?;
    }
    let mut agent_phase_guard =
        AgentPhaseGuard::new(&mut git_helpers, &ctx.logger, &*ctx.workspace);

    // Print welcome banner and validate PROMPT.md
    print_welcome_banner(ctx.colors, &ctx.developer_display, &ctx.reviewer_display);
    print_pipeline_info_with_config(ctx, &config);
    validate_prompt_and_setup_backup(ctx)?;

    // Set up PROMPT.md monitoring
    let mut prompt_monitor = setup_prompt_monitor(ctx);

    // Detect project stack and review guidelines
    let (_project_stack, review_guidelines) =
        detect_project_stack(&config, &ctx.repo_root, &ctx.logger, ctx.colors);

    print_review_guidelines(ctx, review_guidelines.as_ref());
    println!();

    // Create phase context and save starting commit
    let (mut timer, mut stats) = (Timer::new(), Stats::new());
    let mut phase_ctx = create_phase_context_with_config(
        ctx,
        &config,
        &mut timer,
        &mut stats,
        review_guidelines.as_ref(),
        &run_context,
        resume_checkpoint.as_ref(),
    );
    save_start_commit_or_warn(ctx);

    // Set up interrupt context for checkpoint saving on Ctrl+C
    let initial_phase = if let Some(ref checkpoint) = resume_checkpoint {
        checkpoint.phase
    } else {
        PipelinePhase::Planning
    };
    setup_interrupt_context_for_pipeline(
        initial_phase,
        config.developer_iters,
        config.reviewer_reviews,
        &phase_ctx.execution_history,
        &phase_ctx.prompt_history,
        &run_context,
    );

    // Ensure interrupt context is cleared on completion
    let _interrupt_guard = defer_clear_interrupt_context();

    // Initialize pipeline state
    let initial_state = if let Some(ref checkpoint) = resume_checkpoint {
        PipelineState::from(checkpoint.clone())
    } else {
        PipelineState::initial(config.developer_iters, config.reviewer_reviews)
    };

    // Configure event loop
    let event_loop_config = EventLoopConfig {
        max_iterations: event_loop::MAX_EVENT_LOOP_ITERATIONS,
        enable_checkpointing: config.features.checkpoint_enabled,
    };

    // Clone execution_history and prompt_history BEFORE running event loop
    let execution_history_before = phase_ctx.execution_history.clone();
    let prompt_history_before = phase_ctx.clone_prompt_history();

    // Run event loop with the provided handler
    effect_handler.update_state(initial_state.clone());
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            effect_handler,
        )
    };

    // Handle event loop result
    let loop_result = loop_result?;
    if loop_result.completed {
        ctx.logger
            .success("Pipeline completed successfully via reducer event loop");
        ctx.logger.info(&format!(
            "Total events processed: {}",
            loop_result.events_processed
        ));
    } else {
        ctx.logger.warn("Pipeline exited without completion marker");
    }

    // Save Complete checkpoint before clearing (for idempotent resume)
    if config.features.checkpoint_enabled {
        let skip_rebase = !ctx.args.rebase_flags.with_rebase;
        let builder = CheckpointBuilder::new()
            .phase(
                PipelinePhase::Complete,
                config.developer_iters,
                config.developer_iters,
            )
            .reviewer_pass(config.reviewer_reviews, config.reviewer_reviews)
            .skip_rebase(skip_rebase)
            .capture_from_context(
                &config,
                &ctx.registry,
                &ctx.developer_agent,
                &ctx.reviewer_agent,
                &ctx.logger,
                &run_context,
            )
            .with_executor_from_context(std::sync::Arc::clone(&ctx.executor));

        let builder = builder
            .with_execution_history(execution_history_before)
            .with_prompt_history(prompt_history_before);

        if let Some(checkpoint) = builder.build() {
            let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
        }
    }

    // Post-pipeline operations
    check_prompt_restoration(ctx, &mut prompt_monitor, "event loop");
    update_status_with_workspace(&*ctx.workspace, "In progress.", config.isolation_mode)?;

    // Commit phase
    finalize_pipeline(
        &mut agent_phase_guard,
        &ctx.logger,
        ctx.colors,
        &config,
        finalization::RuntimeStats {
            timer: &timer,
            stats: &stats,
        },
        prompt_monitor,
        Some(&*ctx.workspace),
    );
    Ok(())
}

/// Set up the interrupt context with initial pipeline state.
///
/// This function initializes the global interrupt context so that if
/// the user presses Ctrl+C, the interrupt handler can save a checkpoint.
fn setup_interrupt_context_for_pipeline(
    phase: PipelinePhase,
    total_iterations: u32,
    total_reviewer_passes: u32,
    execution_history: &crate::checkpoint::ExecutionHistory,
    prompt_history: &std::collections::HashMap<String, String>,
    run_context: &crate::checkpoint::RunContext,
) {
    use crate::interrupt::{set_interrupt_context, InterruptContext};

    // Determine initial iteration based on phase
    let (iteration, reviewer_pass) = match phase {
        PipelinePhase::Development => (1, 0),
        PipelinePhase::Review | PipelinePhase::Fix | PipelinePhase::ReviewAgain => {
            (total_iterations, 1)
        }
        PipelinePhase::PostRebase | PipelinePhase::CommitMessage => {
            (total_iterations, total_reviewer_passes)
        }
        _ => (0, 0),
    };

    let context = InterruptContext {
        phase,
        iteration,
        total_iterations,
        reviewer_pass,
        total_reviewer_passes,
        run_context: run_context.clone(),
        execution_history: execution_history.clone(),
        prompt_history: prompt_history.clone(),
    };

    set_interrupt_context(context);
}

/// Update the interrupt context from the current phase context.
///
/// This function should be called after each major phase to keep the
/// interrupt context up-to-date with the latest execution history.
fn update_interrupt_context_from_phase(
    phase_ctx: &crate::phases::PhaseContext,
    phase: PipelinePhase,
    total_iterations: u32,
    total_reviewer_passes: u32,
    run_context: &crate::checkpoint::RunContext,
) {
    use crate::interrupt::{set_interrupt_context, InterruptContext};

    // Determine current iteration based on phase
    let (iteration, reviewer_pass) = match phase {
        PipelinePhase::Development => {
            // Estimate iteration from actual runs
            let iter = run_context.actual_developer_runs.max(1);
            (iter, 0)
        }
        PipelinePhase::Review | PipelinePhase::Fix | PipelinePhase::ReviewAgain => {
            (total_iterations, run_context.actual_reviewer_runs.max(1))
        }
        PipelinePhase::PostRebase | PipelinePhase::CommitMessage => {
            (total_iterations, total_reviewer_passes)
        }
        _ => (0, 0),
    };

    let context = InterruptContext {
        phase,
        iteration,
        total_iterations,
        reviewer_pass,
        total_reviewer_passes,
        run_context: run_context.clone(),
        execution_history: phase_ctx.execution_history.clone(),
        prompt_history: phase_ctx.clone_prompt_history(),
    };

    set_interrupt_context(context);
}

/// Helper to defer clearing interrupt context until function exit.
///
/// Uses a scope guard pattern to ensure the interrupt context is cleared
/// when the pipeline completes successfully, preventing an "interrupted"
/// checkpoint from being saved after normal completion.
fn defer_clear_interrupt_context() -> InterruptContextGuard {
    InterruptContextGuard
}

/// RAII guard for clearing interrupt context on drop.
///
/// Ensures the interrupt context is cleared when the guard is dropped,
/// preventing an "interrupted" checkpoint from being saved after normal
/// pipeline completion.
struct InterruptContextGuard;

impl Drop for InterruptContextGuard {
    fn drop(&mut self) {
        crate::interrupt::clear_interrupt_context();
    }
}

/// Validate PROMPT.md and set up backup/protection.
fn validate_prompt_and_setup_backup(ctx: &PipelineContext) -> anyhow::Result<()> {
    let prompt_validation = validate_prompt_md_with_workspace(
        &*ctx.workspace,
        ctx.config.behavior.strict_validation,
        ctx.args.interactive,
    );
    for err in &prompt_validation.errors {
        ctx.logger.error(err);
    }
    for warn in &prompt_validation.warnings {
        ctx.logger.warn(warn);
    }
    if !prompt_validation.is_valid() {
        anyhow::bail!("PROMPT.md validation errors");
    }

    // Create a backup of PROMPT.md to protect against accidental deletion.
    match create_prompt_backup_with_workspace(&*ctx.workspace) {
        Ok(None) => {}
        Ok(Some(warning)) => {
            ctx.logger.warn(&format!(
                "PROMPT.md backup created but: {warning}. Continuing anyway."
            ));
        }
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to create PROMPT.md backup: {e}. Continuing anyway."
            ));
        }
    }

    // Make PROMPT.md read-only to protect against accidental deletion.
    match make_prompt_read_only_with_workspace(&*ctx.workspace) {
        None => {}
        Some(warning) => {
            ctx.logger.warn(&format!("{warning}. Continuing anyway."));
        }
    }

    Ok(())
}

/// Set up PROMPT.md monitoring for deletion detection.
fn setup_prompt_monitor(ctx: &PipelineContext) -> Option<PromptMonitor> {
    match PromptMonitor::new() {
        Ok(mut monitor) => {
            if let Err(e) = monitor.start() {
                ctx.logger.warn(&format!(
                    "Failed to start PROMPT.md monitoring: {e}. Continuing anyway."
                ));
                None
            } else {
                if ctx.config.verbosity.is_debug() {
                    ctx.logger.info("Started real-time PROMPT.md monitoring");
                }
                Some(monitor)
            }
        }
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to create PROMPT.md monitor: {e}. Continuing anyway."
            ));
            None
        }
    }
}

/// Print review guidelines if detected.
fn print_review_guidelines(
    ctx: &PipelineContext,
    review_guidelines: Option<&crate::guidelines::ReviewGuidelines>,
) {
    if let Some(guidelines) = review_guidelines {
        ctx.logger.info(&format!(
            "Review guidelines: {}{}{}",
            ctx.colors.dim(),
            guidelines.summary(),
            ctx.colors.reset()
        ));
    }
}

/// Create the phase context with a modified config (for resume restoration).
fn create_phase_context_with_config<'ctx>(
    ctx: &'ctx PipelineContext,
    config: &'ctx crate::config::Config,
    timer: &'ctx mut Timer,
    stats: &'ctx mut Stats,
    review_guidelines: Option<&'ctx crate::guidelines::ReviewGuidelines>,
    run_context: &'ctx crate::checkpoint::RunContext,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> PhaseContext<'ctx> {
    // Restore execution history and prompt history from checkpoint if available
    let (execution_history, prompt_history) = if let Some(checkpoint) = resume_checkpoint {
        let exec_history = checkpoint
            .execution_history
            .clone()
            .unwrap_or_else(crate::checkpoint::execution_history::ExecutionHistory::new);
        let prompt_hist = checkpoint.prompt_history.clone().unwrap_or_default();
        (exec_history, prompt_hist)
    } else {
        (
            crate::checkpoint::execution_history::ExecutionHistory::new(),
            std::collections::HashMap::new(),
        )
    };

    PhaseContext {
        config,
        registry: &ctx.registry,
        logger: &ctx.logger,
        colors: &ctx.colors,
        timer,
        stats,
        developer_agent: &ctx.developer_agent,
        reviewer_agent: &ctx.reviewer_agent,
        review_guidelines,
        template_context: &ctx.template_context,
        run_context: run_context.clone(),
        execution_history,
        prompt_history,
        executor: &*ctx.executor,
        executor_arc: std::sync::Arc::clone(&ctx.executor),
        repo_root: &ctx.repo_root,
        workspace: &*ctx.workspace,
    }
}

/// Print pipeline info with a specific config.
fn print_pipeline_info_with_config(ctx: &PipelineContext, _config: &crate::config::Config) {
    ctx.logger.info(&format!(
        "Working directory: {}{}{}",
        ctx.colors.cyan(),
        ctx.repo_root.display(),
        ctx.colors.reset()
    ));
}

/// Save starting commit or warn if it fails.
///
/// Under `test-utils` feature, this function uses mock data to avoid real git operations.
fn save_start_commit_or_warn(ctx: &PipelineContext) {
    // Skip real git operations when test-utils feature is enabled.
    // These functions call git2::Repository::discover which requires a real git repo.
    #[cfg(feature = "test-utils")]
    {
        // In tests, just log a mock message
        if ctx.config.verbosity.is_debug() {
            ctx.logger.info("Start: 49cb8503 (+18 commits, STALE)");
        }
        ctx.logger
            .warn("Start commit is stale. Consider running: ralph --reset-start-commit");
    }

    #[cfg(not(feature = "test-utils"))]
    {
        match save_start_commit() {
            Ok(()) => {
                if ctx.config.verbosity.is_debug() {
                    ctx.logger
                        .info("Saved starting commit for incremental diff generation");
                }
            }
            Err(e) => {
                ctx.logger.warn(&format!(
                    "Failed to save starting commit: {e}. \
                     Incremental diffs may be unavailable as a result."
                ));
                ctx.logger.info(
                    "To fix this issue, ensure .agent directory is writable and you have a valid HEAD commit.",
                );
            }
        }

        // Display start commit information to user
        match get_start_commit_summary() {
            Ok(summary) => {
                if ctx.config.verbosity.is_debug() || summary.commits_since > 5 || summary.is_stale
                {
                    ctx.logger.info(&summary.format_compact());
                    if summary.is_stale {
                        ctx.logger.warn(
                            "Start commit is stale. Consider running: ralph --reset-start-commit",
                        );
                    } else if summary.commits_since > 5 {
                        ctx.logger
                            .info("Tip: Run 'ralph --show-baseline' for more details");
                    }
                }
            }
            Err(e) => {
                // Only show error in debug mode since this is informational
                if ctx.config.verbosity.is_debug() {
                    ctx.logger
                        .warn(&format!("Failed to get start commit summary: {e}"));
                }
            }
        }
    }
}

/// Check for PROMPT.md restoration after a phase.
fn check_prompt_restoration(
    ctx: &PipelineContext,
    prompt_monitor: &mut Option<PromptMonitor>,
    phase: &str,
) {
    if let Some(ref mut monitor) = prompt_monitor {
        if monitor.check_and_restore() {
            ctx.logger.warn(&format!(
                "PROMPT.md was deleted and restored during {phase} phase"
            ));
        }
    }
}

/// Handle --rebase-only flag.
///
/// This function performs a rebase to the default branch with AI conflict resolution and exits,
/// without running the full pipeline.
pub fn handle_rebase_only(
    _args: &Args,
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    repo_root: &std::path::Path,
) -> anyhow::Result<()> {
    // Check if we're on main/master branch
    if is_main_or_master_branch()? {
        logger.warn("Already on main/master branch - rebasing on main is not recommended");
        logger.info("Tip: Use git worktrees to work on feature branches in parallel:");
        logger.info("  git worktree add ../feature-branch feature-branch");
        logger.info("This allows multiple AI agents to work on different features simultaneously.");
        logger.info("Proceeding with rebase anyway as requested...");
    }

    logger.header("Rebase to default branch", Colors::cyan);

    match run_rebase_to_default(logger, colors, &*executor) {
        Ok(RebaseResult::Success) => {
            logger.success("Rebase completed successfully");
            Ok(())
        }
        Ok(RebaseResult::NoOp { reason }) => {
            logger.info(&format!("No rebase needed: {reason}"));
            Ok(())
        }
        Ok(RebaseResult::Failed(err)) => {
            logger.error(&format!("Rebase failed: {err}"));
            anyhow::bail!("Rebase failed: {err}")
        }
        Ok(RebaseResult::Conflicts(_conflicts)) => {
            // Get the actual conflicted files
            let conflicted_files = get_conflicted_files()?;
            if conflicted_files.is_empty() {
                logger.warn("Rebase reported conflicts but no conflicted files found");
                let _ = abort_rebase(&*executor);
                return Ok(());
            }

            logger.warn(&format!(
                "Rebase resulted in {} conflict(s), attempting AI resolution",
                conflicted_files.len()
            ));

            // For --rebase-only, we don't have a full PhaseContext, so we use a wrapper
            match try_resolve_conflicts_without_phase_ctx(
                &conflicted_files,
                config,
                template_context,
                logger,
                colors,
                std::sync::Arc::clone(&executor),
                repo_root,
            ) {
                Ok(true) => {
                    // Conflicts resolved, continue the rebase
                    logger.info("Continuing rebase after conflict resolution");
                    match continue_rebase(&*executor) {
                        Ok(()) => {
                            logger.success("Rebase completed successfully after AI resolution");
                            Ok(())
                        }
                        Err(e) => {
                            logger.error(&format!("Failed to continue rebase: {e}"));
                            let _ = abort_rebase(&*executor);
                            anyhow::bail!("Rebase failed after conflict resolution")
                        }
                    }
                }
                Ok(false) => {
                    // AI resolution failed
                    logger.error("AI conflict resolution failed, aborting rebase");
                    let _ = abort_rebase(&*executor);
                    anyhow::bail!("Rebase conflicts could not be resolved by AI")
                }
                Err(e) => {
                    logger.error(&format!("Conflict resolution error: {e}"));
                    let _ = abort_rebase(&*executor);
                    anyhow::bail!("Rebase conflict resolution failed: {e}")
                }
            }
        }
        Err(e) => {
            logger.error(&format!("Rebase failed: {e}"));
            anyhow::bail!("Rebase failed: {e}")
        }
    }
}
