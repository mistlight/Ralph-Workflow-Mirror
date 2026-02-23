//! Interrupt signal handling for graceful checkpoint save.
//!
//! This module provides signal handling for the Ralph pipeline, ensuring
//! clean shutdown when the user interrupts with Ctrl+C.
//!
//! When an interrupt is received:
//!
//! - If the reducer event loop is running, the handler sets a global interrupt request
//!   flag and returns. The event loop consumes that flag and performs the reducer-driven
//!   termination sequence (RestorePromptPermissions -> SaveCheckpoint -> shutdown).
//! - If the event loop is not running yet (early startup), the handler falls back to a
//!   best-effort checkpoint save and exits with the standard SIGINT code (130).
//!
//! ## Ctrl+C Exception for Safety Check
//!
//! The `interrupted_by_user` flag distinguishes user-initiated interrupts (Ctrl+C)
//! from programmatic interrupts (AwaitingDevFix exhaustion, completion marker emission).
//! When set to `true`, the pre-termination commit safety check is skipped because
//! the user explicitly chose to interrupt execution. This respects user intent while
//! ensuring all other termination paths commit uncommitted work before exiting.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::workspace::Workspace;
use std::path::Path;

/// Global interrupt context for checkpoint saving on interrupt.
///
/// This is set during pipeline initialization and used by the interrupt
/// handler to save a checkpoint when the user presses Ctrl+C.
static INTERRUPT_CONTEXT: Mutex<Option<InterruptContext>> = Mutex::new(None);

/// True when a user interrupt (SIGINT / Ctrl+C) has been requested.
///
/// The signal handler sets this flag. The reducer event loop consumes it and
/// transitions the pipeline to an Interrupted state so termination effects
/// (RestorePromptPermissions, SaveCheckpoint) execute deterministically.
static USER_INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// True once a user interrupt has occurred during this process lifetime.
///
/// Unlike `USER_INTERRUPT_REQUESTED`, this flag is NEVER cleared. It remains
/// set even after the event loop consumes the pending interrupt request via
/// `take_user_interrupt_request()`. Use this flag in shutdown code paths
/// (e.g., `capture_git_state`) where you need to know whether the process is
/// shutting down due to Ctrl+C, even after the pending request has been consumed.
static USER_INTERRUPTED_OCCURRED: AtomicBool = AtomicBool::new(false);

/// True while the reducer event loop is running.
///
/// When true, the Ctrl+C handler must NOT call `process::exit()`.
/// Instead it requests interruption and lets the event loop drive:
/// - RestorePromptPermissions
/// - SaveCheckpoint
/// - orderly shutdown
static EVENT_LOOP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// True when the process should exit with code 130 after the pipeline returns.
///
/// We intentionally do not call `process::exit(130)` from inside the pipeline runner,
/// because that would bypass Rust destructors (RAII cleanup like `AgentPhaseGuard::drop()`).
/// Instead, the pipeline requests this exit code and `main()` performs the actual
/// exit after stack unwinding and cleanup completes.
static EXIT_130_AFTER_RUN: AtomicBool = AtomicBool::new(false);

/// Request that the process exit with code 130 once the pipeline returns.
pub fn request_exit_130_after_run() {
    EXIT_130_AFTER_RUN.store(true, Ordering::SeqCst);
}

/// Consume a pending exit-130 request.
pub fn take_exit_130_after_run() -> bool {
    EXIT_130_AFTER_RUN.swap(false, Ordering::SeqCst)
}

#[cfg(unix)]
fn restore_prompt_md_writable_via_std_fs() {
    use std::os::unix::fs::PermissionsExt;

    fn make_writable(path: &std::path::Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };

        let mut perms = metadata.permissions();
        // Preserve existing mode bits but ensure owner write is enabled.
        perms.set_mode(perms.mode() | 0o200);
        std::fs::set_permissions(path, perms).is_ok()
    }

    // Fast path: current working directory is already the repo root in normal runs.
    if make_writable(std::path::Path::new("PROMPT.md")) {
        return;
    }

    // Fallback: discover repo root.
    let Ok(repo_root) = crate::git_helpers::get_repo_root() else {
        return;
    };

    let prompt_path = repo_root.join("PROMPT.md");
    let _ = make_writable(&prompt_path);
}

#[cfg(not(unix))]
fn restore_prompt_md_writable_via_std_fs() {}

/// RAII guard that marks the reducer event loop as active.
pub struct EventLoopActiveGuard;

impl Drop for EventLoopActiveGuard {
    fn drop(&mut self) {
        EVENT_LOOP_ACTIVE.store(false, Ordering::SeqCst);
    }
}

/// Mark the reducer event loop as active for the duration of the returned guard.
pub fn event_loop_active_guard() -> EventLoopActiveGuard {
    EVENT_LOOP_ACTIVE.store(true, Ordering::SeqCst);
    EventLoopActiveGuard
}

fn is_event_loop_active() -> bool {
    EVENT_LOOP_ACTIVE.load(Ordering::SeqCst)
}

/// Request that the running pipeline treat the run as user-interrupted.
///
/// This is called by the Ctrl+C handler. The event loop is responsible for
/// consuming the request and translating it into a reducer-visible transition.
///
/// Also sets the persistent `USER_INTERRUPTED_OCCURRED` flag, which is never
/// cleared and allows shutdown code paths (e.g., `capture_git_state`) to
/// detect the interrupt even after the event loop has consumed the pending
/// request via `take_user_interrupt_request()`.
pub fn request_user_interrupt() {
    USER_INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
    USER_INTERRUPTED_OCCURRED.store(true, Ordering::SeqCst);
}

/// Check if a user interrupt has occurred at any point during this process lifetime.
///
/// Returns true once a Ctrl+C has been received, and remains true for the rest
/// of the process lifetime even after `take_user_interrupt_request()` has consumed
/// the pending request.
///
/// Use this in shutdown code paths where you need to know whether the process
/// is shutting down due to user interruption, even if the event loop has already
/// consumed the interrupt request. For example, `capture_git_state` uses this
/// to skip git commands that could hang indefinitely during interrupt-triggered
/// shutdown.
pub fn user_interrupted_occurred() -> bool {
    USER_INTERRUPTED_OCCURRED.load(Ordering::SeqCst)
}

/// Check if a user interrupt request is pending without consuming it.
///
/// Returns true if an interrupt is pending. The flag remains set so that
/// the event loop can still consume it via `take_user_interrupt_request()`.
///
/// Use this when you need to react to an interrupt (e.g., kill a subprocess)
/// without stealing the flag from the event loop's per-iteration check.
pub fn is_user_interrupt_requested() -> bool {
    USER_INTERRUPT_REQUESTED.load(Ordering::SeqCst)
}

/// Consume a pending user interrupt request.
///
/// Returns true if an interrupt was pending.
pub fn take_user_interrupt_request() -> bool {
    USER_INTERRUPT_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Reset the persistent user-interrupted flag.
///
/// Only intended for use in tests to restore a clean state between test cases
/// that exercise interrupt behavior. Production code must not call this.
#[cfg(test)]
pub fn reset_user_interrupted_occurred() {
    USER_INTERRUPTED_OCCURRED.store(false, Ordering::SeqCst);
}

/// Context needed to save a checkpoint when interrupted.
///
/// This structure holds references to all the state needed to create
/// a checkpoint when the user interrupts the pipeline with Ctrl+C.
#[derive(Clone)]
pub struct InterruptContext {
    /// Current pipeline phase
    pub phase: crate::checkpoint::PipelinePhase,
    /// Current iteration number
    pub iteration: u32,
    /// Total iterations configured
    pub total_iterations: u32,
    /// Current reviewer pass number
    pub reviewer_pass: u32,
    /// Total reviewer passes configured
    pub total_reviewer_passes: u32,
    /// Run context for tracking execution lineage
    pub run_context: crate::checkpoint::RunContext,
    /// Execution history tracking
    pub execution_history: crate::checkpoint::ExecutionHistory,
    /// Prompt history for deterministic resume
    pub prompt_history: std::collections::HashMap<String, String>,
    /// Workspace for checkpoint persistence
    pub workspace: std::sync::Arc<dyn Workspace>,
}

/// Set the global interrupt context.
///
/// This function should be called during pipeline initialization to
/// provide the interrupt handler with the context needed to save
/// a checkpoint when interrupted.
///
/// # Arguments
///
/// * `context` - The interrupt context to store
///
/// # Note
///
/// This function is typically called at the start of `run_pipeline()`
/// to ensure the interrupt handler has the most up-to-date context.
pub fn set_interrupt_context(context: InterruptContext) {
    let mut ctx = INTERRUPT_CONTEXT.lock().unwrap_or_else(|poison| {
        // If mutex is poisoned, recover the guard and clear the state
        poison.into_inner()
    });
    *ctx = Some(context);
}

/// Clear the global interrupt context.
///
/// This should be called when the pipeline completes successfully
/// to prevent saving an interrupt checkpoint after normal completion.
pub fn clear_interrupt_context() {
    let mut ctx = INTERRUPT_CONTEXT.lock().unwrap_or_else(|poison| {
        // If mutex is poisoned, recover the guard and clear the state
        poison.into_inner()
    });
    *ctx = None;
}

/// Set up the interrupt handler for graceful shutdown with checkpoint saving.
///
/// This function registers a SIGINT handler that will:
/// 1. Save a checkpoint with the current pipeline state
/// 2. Clean up generated files
/// 3. Exit gracefully
///
/// Call this early in main() after initializing the pipeline context.
pub fn setup_interrupt_handler() {
    let install = ctrlc::set_handler(|| {
        request_user_interrupt();

        // If the reducer event loop is running, do not exit here.
        // The event loop will observe the request, restore permissions, and checkpoint.
        if is_event_loop_active() {
            eprintln!(
                "\nInterrupt received; requesting graceful shutdown (waiting for checkpoint)..."
            );
            return;
        }

        eprintln!("\nInterrupt received; saving checkpoint...");

        // Clone the entire context (small, Arc-backed) and then perform I/O without
        // holding the mutex.
        let context = {
            let ctx = INTERRUPT_CONTEXT
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            ctx.clone()
        };

        if let Some(ref context) = context {
            if let Err(e) = save_interrupt_checkpoint(context) {
                eprintln!("Warning: Failed to save checkpoint: {}", e);
            } else {
                eprintln!("Checkpoint saved. Resume with: ralph --resume");
            }
        }

        // Best-effort: restore PROMPT.md permissions so we don't leave the repo locked.
        // This is primarily for early-interrupt cases before the reducer event loop starts.
        //
        // Always attempt a std::fs fallback using repo discovery. This covers:
        // - interrupt context not yet installed (very early SIGINT)
        // - workspace implementations that cannot mutate real filesystem permissions
        //   (e.g., MemoryWorkspace)
        restore_prompt_md_writable_via_std_fs();

        if let Some(ref context) = context {
            let _ = context.workspace.set_writable(Path::new("PROMPT.md"));
        }

        eprintln!("Cleaning up...");
        crate::git_helpers::cleanup_agent_phase_silent();
        std::process::exit(130); // Standard exit code for SIGINT
    });

    if let Err(e) = install {
        // Handler installation failure is a reliability issue: without it, Ctrl+C will not
        // trigger checkpointing/cleanup and can leave the repo in a broken state.
        eprintln!("Warning: failed to install Ctrl+C handler: {e}");
    }
}

/// Save a checkpoint when the pipeline is interrupted.
///
/// This function persists a checkpoint that records the *current operational phase*
/// and sets `interrupted_by_user=true`.
///
/// We intentionally do NOT overwrite the phase to `Interrupted` because that makes
/// `--resume` terminate immediately in `PipelinePhase::Interrupted`.
///
/// # Arguments
///
/// * `context` - The interrupt context containing the current pipeline state
fn save_interrupt_checkpoint(context: &InterruptContext) -> anyhow::Result<()> {
    use crate::checkpoint::state::{
        calculate_file_checksum_with_workspace, AgentConfigSnapshot, CheckpointParams,
        CliArgsSnapshotBuilder, PipelineCheckpoint, RebaseState,
    };
    use crate::checkpoint::{load_checkpoint_with_workspace, save_checkpoint_with_workspace};
    use std::path::Path;

    // Read checkpoint from file if exists, update it with current operational phase
    if let Ok(Some(mut checkpoint)) = load_checkpoint_with_workspace(&*context.workspace) {
        // Update existing checkpoint with current operational phase and progress.
        checkpoint.phase = context.phase;
        checkpoint.iteration = context.iteration;
        checkpoint.total_iterations = context.total_iterations;
        checkpoint.reviewer_pass = context.reviewer_pass;
        checkpoint.total_reviewer_passes = context.total_reviewer_passes;
        checkpoint.actual_developer_runs = context.run_context.actual_developer_runs;
        checkpoint.actual_reviewer_runs = context.run_context.actual_reviewer_runs;
        checkpoint.execution_history = Some(context.execution_history.clone());
        checkpoint.prompt_history = Some(context.prompt_history.clone());

        // Mark this as a user-initiated interrupt (Ctrl+C)
        // This exempts the pipeline from the pre-termination commit safety check
        checkpoint.interrupted_by_user = true;

        save_checkpoint_with_workspace(&*context.workspace, &checkpoint)?;
    } else {
        // No checkpoint exists yet - this is early interruption.
        //
        // We still MUST persist a checkpoint (not just print) so that resume can reliably
        // honor the Ctrl+C exemption via `interrupted_by_user=true`.
        //
        // This checkpoint uses conservative placeholder agent snapshots because we don't
        // have access to Config/AgentRegistry in the signal handler.
        let prompt_md_checksum =
            calculate_file_checksum_with_workspace(&*context.workspace, Path::new("PROMPT.md"))
                .or_else(|| Some("unknown".to_string()));

        let cli_args = CliArgsSnapshotBuilder::new(
            context.total_iterations,
            context.total_reviewer_passes,
            /* review_depth */ None,
            /* isolation_mode */ true,
        )
        .build();

        let developer_agent = "unknown";
        let reviewer_agent = "unknown";
        let developer_agent_config = AgentConfigSnapshot::new(
            developer_agent.to_string(),
            "unknown".to_string(),
            "-o".to_string(),
            None,
            /* can_commit */ true,
        );
        let reviewer_agent_config = AgentConfigSnapshot::new(
            reviewer_agent.to_string(),
            "unknown".to_string(),
            "-o".to_string(),
            None,
            /* can_commit */ true,
        );

        let working_dir = context.workspace.root().to_string_lossy().to_string();
        let mut checkpoint = PipelineCheckpoint::from_params(CheckpointParams {
            phase: context.phase,
            iteration: context.iteration,
            total_iterations: context.total_iterations,
            reviewer_pass: context.reviewer_pass,
            total_reviewer_passes: context.total_reviewer_passes,
            developer_agent,
            reviewer_agent,
            cli_args,
            developer_agent_config,
            reviewer_agent_config,
            rebase_state: RebaseState::default(),
            git_user_name: None,
            git_user_email: None,
            run_id: &context.run_context.run_id,
            parent_run_id: context.run_context.parent_run_id.as_deref(),
            resume_count: context.run_context.resume_count,
            actual_developer_runs: context.run_context.actual_developer_runs,
            actual_reviewer_runs: context.run_context.actual_reviewer_runs,
            working_dir,
            prompt_md_checksum,
            config_path: None,
            config_checksum: None,
        });

        checkpoint.execution_history = Some(context.execution_history.clone());
        checkpoint.prompt_history = Some(context.prompt_history.clone());
        checkpoint.interrupted_by_user = true;

        save_checkpoint_with_workspace(&*context.workspace, &checkpoint)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::load_checkpoint_with_workspace;
    use crate::workspace::MemoryWorkspace;
    use std::sync::Arc;

    #[test]
    fn interrupt_handler_source_is_ascii_only() {
        // Guardrail: keep interrupt handler output ASCII-only to avoid encoding/rendering
        // issues in unattended environments.
        let src = include_str!("interrupt.rs");
        assert!(
            src.is_ascii(),
            "interrupt.rs must remain ASCII-only; found non-ASCII characters"
        );
    }

    #[test]
    fn test_interrupt_context_creation() {
        let workspace = Arc::new(MemoryWorkspace::new_test());
        let context = InterruptContext {
            phase: crate::checkpoint::PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            run_context: crate::checkpoint::RunContext::new(),
            execution_history: crate::checkpoint::ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            workspace,
        };

        assert_eq!(context.phase, crate::checkpoint::PipelinePhase::Development);
        assert_eq!(context.iteration, 2);
        assert_eq!(context.total_iterations, 5);
    }

    #[test]
    fn test_set_and_clear_interrupt_context() {
        let workspace = Arc::new(MemoryWorkspace::new_test());
        let context = InterruptContext {
            phase: crate::checkpoint::PipelinePhase::Planning,
            iteration: 1,
            total_iterations: 3,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            run_context: crate::checkpoint::RunContext::new(),
            execution_history: crate::checkpoint::ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            workspace,
        };

        set_interrupt_context(context);
        {
            let ctx = INTERRUPT_CONTEXT.lock().unwrap();
            assert!(ctx.is_some());
            assert_eq!(
                ctx.as_ref().unwrap().phase,
                crate::checkpoint::PipelinePhase::Planning
            );
        }

        clear_interrupt_context();
        let ctx = INTERRUPT_CONTEXT.lock().unwrap();
        assert!(ctx.is_none());
    }

    #[test]
    fn test_early_interrupt_persists_minimal_checkpoint_with_user_interrupt_flag() {
        // Regression: if Ctrl+C happens before the first regular checkpoint exists,
        // we must still persist a checkpoint (or marker) that records interrupted_by_user=true.
        let workspace: Arc<dyn Workspace> =
            Arc::new(MemoryWorkspace::new_test().with_file("PROMPT.md", "# prompt"));
        let context = InterruptContext {
            phase: crate::checkpoint::PipelinePhase::Planning,
            iteration: 0,
            total_iterations: 3,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            run_context: crate::checkpoint::RunContext::new(),
            execution_history: crate::checkpoint::ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            workspace: Arc::clone(&workspace),
        };

        // No checkpoint exists yet
        assert!(
            load_checkpoint_with_workspace(&*workspace)
                .expect("load should not error")
                .is_none(),
            "test precondition: no checkpoint should exist"
        );

        save_interrupt_checkpoint(&context).expect("save should succeed");

        let checkpoint = load_checkpoint_with_workspace(&*workspace)
            .expect("checkpoint should be readable")
            .expect("checkpoint should exist after early interrupt");

        // On Ctrl+C we persist the *current operational phase* so `--resume`
        // continues work instead of immediately terminating in Interrupted phase.
        assert_eq!(checkpoint.phase, context.phase);
        assert!(checkpoint.interrupted_by_user);
        assert!(
            checkpoint.prompt_md_checksum.is_some(),
            "prompt checksum must be recorded for resume validation"
        );
        assert_eq!(checkpoint.cli_args.developer_iters, 3);
        assert_eq!(checkpoint.cli_args.reviewer_reviews, 1);
    }

    #[test]
    fn test_interrupt_checkpoint_does_not_overwrite_existing_checkpoint_phase() {
        // Regression: if a checkpoint already exists, Ctrl+C should NOT overwrite the
        // saved phase to Interrupted. Doing so makes `--resume` terminate immediately.
        let workspace: Arc<dyn Workspace> =
            Arc::new(MemoryWorkspace::new_test().with_file("PROMPT.md", "# prompt"));

        // Seed an existing checkpoint on disk.
        // Use the full from_params constructor to avoid builder preconditions.
        let run_context = crate::checkpoint::RunContext::new();
        let cli_args = crate::checkpoint::state::CliArgsSnapshotBuilder::new(
            /* developer_iters */ 3, /* reviewer_reviews */ 1,
            /* review_depth */ None, /* isolation_mode */ true,
        )
        .build();

        let developer_agent_config = crate::checkpoint::state::AgentConfigSnapshot::new(
            "dev".to_string(),
            "dev".to_string(),
            "-o".to_string(),
            None,
            /* can_commit */ true,
        );
        let reviewer_agent_config = crate::checkpoint::state::AgentConfigSnapshot::new(
            "rev".to_string(),
            "rev".to_string(),
            "-o".to_string(),
            None,
            /* can_commit */ true,
        );

        let working_dir = workspace.root().to_string_lossy().to_string();
        let mut existing = crate::checkpoint::state::PipelineCheckpoint::from_params(
            crate::checkpoint::state::CheckpointParams {
                phase: crate::checkpoint::PipelinePhase::Development,
                iteration: 1,
                total_iterations: 3,
                reviewer_pass: 0,
                total_reviewer_passes: 1,
                developer_agent: "dev",
                reviewer_agent: "rev",
                cli_args,
                developer_agent_config,
                reviewer_agent_config,
                rebase_state: crate::checkpoint::state::RebaseState::default(),
                git_user_name: None,
                git_user_email: None,
                run_id: &run_context.run_id,
                parent_run_id: run_context.parent_run_id.as_deref(),
                resume_count: run_context.resume_count,
                actual_developer_runs: run_context.actual_developer_runs,
                actual_reviewer_runs: run_context.actual_reviewer_runs,
                working_dir,
                prompt_md_checksum: Some("seed".to_string()),
                config_path: None,
                config_checksum: None,
            },
        );
        existing.interrupted_by_user = false;
        crate::checkpoint::save_checkpoint_with_workspace(&*workspace, &existing)
            .expect("seed checkpoint should save");

        let context = InterruptContext {
            phase: crate::checkpoint::PipelinePhase::Development,
            iteration: 1,
            total_iterations: 3,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            run_context: crate::checkpoint::RunContext::new(),
            execution_history: crate::checkpoint::ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            workspace: Arc::clone(&workspace),
        };

        save_interrupt_checkpoint(&context).expect("interrupt save should succeed");

        let checkpoint = load_checkpoint_with_workspace(&*workspace)
            .expect("checkpoint should be readable")
            .expect("checkpoint should exist");

        assert_eq!(
            checkpoint.phase,
            crate::checkpoint::PipelinePhase::Development
        );
        assert!(checkpoint.interrupted_by_user);
    }
}
