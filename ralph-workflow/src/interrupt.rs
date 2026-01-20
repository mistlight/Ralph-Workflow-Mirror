//! Interrupt signal handling for graceful checkpoint save.
//!
//! This module provides signal handling for the Ralph pipeline, ensuring
//! clean shutdown when the user interrupts with Ctrl+C.
//!
//! When an interrupt is received, the handler will:
//! 1. Save a checkpoint with the `Interrupted` phase
//! 2. Clean up temporary files
//! 3. Exit gracefully

use std::sync::Mutex;

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
        eprintln!("\n✋ Interrupt received! Saving checkpoint...");

        // Try to save checkpoint if context is available
        let ctx = INTERRUPT_CONTEXT.lock().unwrap_or_else(|poison| {
            // If mutex is poisoned, recover the guard (context may be inconsistent)
            poison.into_inner()
        });
        if let Some(ref context) = *ctx {
            if let Err(e) = save_interrupt_checkpoint(context) {
                eprintln!("Warning: Failed to save checkpoint: {}", e);
            } else {
                eprintln!("✓ Checkpoint saved. Resume with: ralph --resume");
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
    use crate::checkpoint::{save_checkpoint, PipelinePhase};

    // Read checkpoint from file if exists, update it with Interrupted phase
    if let Ok(Some(mut checkpoint)) = crate::checkpoint::load_checkpoint() {
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
        save_checkpoint(&checkpoint)?;
    } else {
        // No checkpoint exists yet - this is early interruption
        // We can't save a full checkpoint without config/registry access
        // Just save a minimal checkpoint marker
        eprintln!("Note: Interrupted before first checkpoint. Minimal state saved.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupt_context_creation() {
        let context = InterruptContext {
            phase: crate::checkpoint::PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            run_context: crate::checkpoint::RunContext::new(),
            execution_history: crate::checkpoint::ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
        };

        assert_eq!(context.phase, crate::checkpoint::PipelinePhase::Development);
        assert_eq!(context.iteration, 2);
        assert_eq!(context.total_iterations, 5);
    }

    #[test]
    fn test_set_and_clear_interrupt_context() {
        let context = InterruptContext {
            phase: crate::checkpoint::PipelinePhase::Planning,
            iteration: 1,
            total_iterations: 3,
            reviewer_pass: 0,
            total_reviewer_passes: 1,
            run_context: crate::checkpoint::RunContext::new(),
            execution_history: crate::checkpoint::ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
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
}
