//! Interrupt signal handling for graceful checkpoint save.
//!
//! This module provides signal handling for the Ralph pipeline, ensuring
//! clean shutdown when the user interrupts with Ctrl+C.
//!
//! When an interrupt is received, the handler will:
//! 1. Save a checkpoint with the `Interrupted` phase
//! 2. Set `interrupted_by_user = true` to exempt from pre-termination commit check
//! 3. Clean up temporary files
//! 4. Exit gracefully
//!
//! ## Ctrl+C Exception for Safety Check
//!
//! The `interrupted_by_user` flag distinguishes user-initiated interrupts (Ctrl+C)
//! from programmatic interrupts (AwaitingDevFix exhaustion, completion marker emission).
//! When set to `true`, the pre-termination commit safety check is skipped because
//! the user explicitly chose to interrupt execution. This respects user intent while
//! ensuring all other termination paths commit uncommitted work before exiting.

use std::sync::Mutex;

use crate::workspace::Workspace;

/// Global interrupt context for checkpoint saving on interrupt.
///
/// This is set during pipeline initialization and used by the interrupt
/// handler to save a checkpoint when the user presses Ctrl+C.
static INTERRUPT_CONTEXT: Mutex<Option<InterruptContext>> = Mutex::new(None);

/// Context needed to save a checkpoint when interrupted.
///
/// This structure holds references to all the state needed to create
/// a checkpoint when the user interrupts the pipeline with Ctrl+C.
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
    ctrlc::set_handler(|| {
        eprintln!("\nInterrupt received; saving checkpoint...");

        // Try to save checkpoint if context is available
        let ctx = INTERRUPT_CONTEXT.lock().unwrap_or_else(|poison| {
            // If mutex is poisoned, recover the guard (context may be inconsistent)
            poison.into_inner()
        });
        if let Some(ref context) = *ctx {
            if let Err(e) = save_interrupt_checkpoint(context) {
                eprintln!("Warning: Failed to save checkpoint: {}", e);
            } else {
                eprintln!("Checkpoint saved. Resume with: ralph --resume");
            }
        }
        drop(ctx); // Release lock before cleanup

        eprintln!("Cleaning up...");
        crate::git_helpers::cleanup_agent_phase_silent();
        std::process::exit(130); // Standard exit code for SIGINT
    })
    .ok(); // Ignore errors if handler can't be set
}

/// Save a checkpoint when the pipeline is interrupted.
///
/// This function creates a checkpoint with the `Interrupted` phase,
/// which has the highest phase rank so resuming will run the full pipeline.
///
/// The original phase information is preserved for display purposes
/// by updating the checkpoint with current progress information.
///
/// # Arguments
///
/// * `context` - The interrupt context containing the current pipeline state
fn save_interrupt_checkpoint(context: &InterruptContext) -> anyhow::Result<()> {
    use crate::checkpoint::state::{
        calculate_file_checksum_with_workspace, AgentConfigSnapshot, CheckpointParams,
        CliArgsSnapshotBuilder, PipelineCheckpoint, PipelinePhase, RebaseState,
    };
    use crate::checkpoint::{load_checkpoint_with_workspace, save_checkpoint_with_workspace};
    use std::path::Path;

    // Read checkpoint from file if exists, update it with Interrupted phase
    if let Ok(Some(mut checkpoint)) = load_checkpoint_with_workspace(&*context.workspace) {
        // Store the original phase for reference (it will be overwritten below)
        let _original_phase = context.phase;

        // Update existing checkpoint to Interrupted phase with current progress
        checkpoint.phase = PipelinePhase::Interrupted;
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
            phase: PipelinePhase::Interrupted,
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

        assert_eq!(
            checkpoint.phase,
            crate::checkpoint::PipelinePhase::Interrupted
        );
        assert!(checkpoint.interrupted_by_user);
        assert!(
            checkpoint.prompt_md_checksum.is_some(),
            "prompt checksum must be recorded for resume validation"
        );
        assert_eq!(checkpoint.cli_args.developer_iters, 3);
        assert_eq!(checkpoint.cli_args.reviewer_reviews, 1);
    }
}
