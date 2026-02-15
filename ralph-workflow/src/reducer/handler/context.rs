use super::MainEffectHandler;
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, WorkspaceIoErrorKind};
use anyhow::Result;
use std::path::Path;

impl MainEffectHandler {
    pub(super) fn validate_final_state(
        &mut self,
        _ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        // Transition to Finalizing phase to restore PROMPT.md permissions
        // via the effect system before marking the pipeline complete
        let event = PipelineEvent::finalizing_started();

        // Emit phase transition UI event
        let ui_event = self.phase_transition_ui(PipelinePhase::Finalizing);

        Ok(EffectResult::with_ui(event, vec![ui_event]))
    }

    pub(super) fn cleanup_context(&mut self, ctx: &mut PhaseContext<'_>) -> Result<EffectResult> {
        ctx.logger
            .info("Cleaning up context files to prevent pollution...");

        let mut cleaned_count = 0;

        // Delete PLAN.md via workspace
        let plan_path = Path::new(".agent/PLAN.md");
        if ctx.workspace.exists(plan_path) {
            ctx.workspace
                .remove(plan_path)
                .map_err(|err| ErrorEvent::WorkspaceRemoveFailed {
                    path: plan_path.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                })?;
            cleaned_count += 1;
        }

        // Delete ISSUES.md (may not exist if in isolation mode) via workspace
        let issues_path = Path::new(".agent/ISSUES.md");
        if ctx.workspace.exists(issues_path) {
            ctx.workspace
                .remove(issues_path)
                .map_err(|err| ErrorEvent::WorkspaceRemoveFailed {
                    path: issues_path.display().to_string(),
                    kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                })?;
            cleaned_count += 1;
        }

        // Delete ALL .xml files in .agent/tmp/ to prevent context pollution via workspace
        let tmp_dir = Path::new(".agent/tmp");
        if ctx.workspace.exists(tmp_dir) {
            let entries =
                ctx.workspace
                    .read_dir(tmp_dir)
                    .map_err(|err| ErrorEvent::WorkspaceReadFailed {
                        path: tmp_dir.display().to_string(),
                        kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                    })?;

            for entry in entries {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("xml") {
                    ctx.workspace.remove(path).map_err(|err| {
                        ErrorEvent::WorkspaceRemoveFailed {
                            path: path.display().to_string(),
                            kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
                        }
                    })?;
                    cleaned_count += 1;
                }
            }
        }

        // Delete continuation context file (if present) via workspace
        cleanup_continuation_context_file(ctx)?;

        if cleaned_count > 0 {
            ctx.logger.success(&format!(
                "Context cleanup complete: {} files deleted",
                cleaned_count
            ));
        } else {
            ctx.logger.info("No context files to clean up");
        }

        Ok(EffectResult::event(PipelineEvent::context_cleaned()))
    }

    pub(super) fn restore_prompt_permissions(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        use crate::files::make_prompt_writable_with_workspace;

        ctx.logger.info("Restoring PROMPT.md write permissions...");

        let warning = make_prompt_writable_with_workspace(ctx.workspace);

        if let Some(ref msg) = warning {
            ctx.logger.warn(msg);
        }

        let event = PipelineEvent::prompt_permissions_restored();
        let mut result = EffectResult::event(event);

        if let Some(msg) = warning {
            result = result
                .with_additional_event(PipelineEvent::prompt_permissions_restore_warning(msg));
        }

        if self.state.phase == PipelinePhase::Finalizing {
            return Ok(result.with_ui_event(self.phase_transition_ui(PipelinePhase::Complete)));
        }

        Ok(result)
    }

    pub(super) fn lock_prompt_permissions(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        use crate::files::make_prompt_read_only_with_workspace;

        ctx.logger
            .info("Locking PROMPT.md (read-only protection during execution)...");

        let warning = make_prompt_read_only_with_workspace(ctx.workspace);

        if let Some(ref msg) = warning {
            ctx.logger.warn(&format!("{}. Continuing anyway.", msg));
        }

        let event = PipelineEvent::prompt_permissions_locked(warning);

        Ok(EffectResult::event(event))
    }

    pub(super) fn cleanup_continuation_context(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        cleanup_continuation_context_file(ctx)?;
        Ok(EffectResult::event(
            PipelineEvent::development_continuation_context_cleaned(),
        ))
    }

    pub(super) fn trigger_loop_recovery(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        detected_loop: String,
        loop_count: u32,
    ) -> Result<EffectResult> {
        ctx.logger.warn(&format!(
            "⚠️  LOOP DETECTED: Same effect repeated {} times: {}",
            loop_count, detected_loop
        ));
        ctx.logger
            .info("Triggering mandatory loop recovery to break the cycle...");
        ctx.logger
            .info("Emitting loop recovery event (state cleanup will occur in reducer)");

        // Note: The actual state cleanup (XSD retry reset, session clear, loop counter reset)
        // happens in the reducer when LoopRecoveryTriggered event is reduced.
        // This handler only emits the event to trigger that cleanup.

        ctx.logger
            .success("Loop recovery triggered. Pipeline will resume with fresh state.");

        Ok(EffectResult::event(PipelineEvent::loop_recovery_triggered(
            detected_loop,
            loop_count,
        )))
    }

    pub(super) fn emit_recovery_reset(
        &self,
        ctx: &PhaseContext<'_>,
        reset_type: crate::reducer::effect::RecoveryResetType,
        target_phase: crate::reducer::event::PipelinePhase,
    ) -> Result<EffectResult> {
        use crate::reducer::event::AwaitingDevFixEvent;

        // Log the recovery reset for observability
        ctx.logger.info(&format!(
            "Recovery escalation: {:?} reset to phase {:?}",
            reset_type, target_phase
        ));

        // Emit RecoveryAttempted event to signal transition back to work
        Ok(EffectResult::event(PipelineEvent::AwaitingDevFix(
            AwaitingDevFixEvent::RecoveryAttempted {
                level: match reset_type {
                    crate::reducer::effect::RecoveryResetType::PhaseStart => 2,
                    crate::reducer::effect::RecoveryResetType::IterationReset => 3,
                    crate::reducer::effect::RecoveryResetType::CompleteReset => 4,
                },
                attempt_count: self.state.dev_fix_attempt_count,
                target_phase,
            },
        )))
    }

    pub(super) fn attempt_recovery(
        &self,
        ctx: &PhaseContext<'_>,
        level: u32,
        attempt_count: u32,
    ) -> Result<EffectResult> {
        use crate::reducer::event::AwaitingDevFixEvent;

        let target_phase = self
            .state
            .failed_phase_for_recovery
            .or(self.state.previous_phase)
            .unwrap_or(PipelinePhase::Development);
        let target_phase = if target_phase == PipelinePhase::AwaitingDevFix {
            PipelinePhase::Development
        } else {
            target_phase
        };

        ctx.logger.info(&format!(
            "Attempting recovery level {} (attempt {})",
            level, attempt_count
        ));

        // Emit RecoveryAttempted event to transition back to failed phase
        Ok(EffectResult::event(PipelineEvent::AwaitingDevFix(
            AwaitingDevFixEvent::RecoveryAttempted {
                level,
                attempt_count,
                target_phase,
            },
        )))
    }

    pub(super) fn emit_recovery_success(
        &self,
        ctx: &PhaseContext<'_>,
        level: u32,
        total_attempts: u32,
    ) -> Result<EffectResult> {
        use crate::reducer::event::AwaitingDevFixEvent;

        ctx.logger.info(&format!(
            "Recovery succeeded at level {} after {} attempts",
            level, total_attempts
        ));

        // Emit RecoverySucceeded event to clear recovery state
        Ok(EffectResult::event(PipelineEvent::AwaitingDevFix(
            AwaitingDevFixEvent::RecoverySucceeded {
                level,
                total_attempts,
            },
        )))
    }

    pub(super) fn ensure_gitignore_entries(
        &mut self,
        ctx: &mut PhaseContext<'_>,
    ) -> Result<EffectResult> {
        ctx.logger
            .info("Ensuring .gitignore contains agent artifact entries...");

        let gitignore_path = Path::new(".gitignore");
        let required_entries = vec!["/PROMPT*", ".agent/"];

        // Capture file_created status BEFORE any file operations to avoid race condition
        let file_created = !ctx.workspace.exists(gitignore_path);

        // Read existing .gitignore content (or empty string if doesn't exist)
        let existing_content = if ctx.workspace.exists(gitignore_path) {
            ctx.workspace
                .read(gitignore_path)
                .unwrap_or_else(|_| String::new())
        } else {
            String::new()
        };

        // Check which entries are missing
        let mut entries_added = Vec::new();
        let mut already_present = Vec::new();

        for pattern in &required_entries {
            if entry_exists(&existing_content, pattern) {
                already_present.push(pattern.to_string());
            } else {
                entries_added.push(pattern.to_string());
            }
        }

        // If any entries are missing, update .gitignore
        if !entries_added.is_empty() {
            let mut new_content = existing_content;

            // Ensure content ends with newline if not empty
            if !new_content.is_empty() && !new_content.ends_with('\n') {
                new_content.push('\n');
            }

            // Add comment header and new entries
            if !new_content.is_empty() {
                new_content.push('\n');
            }
            new_content.push_str("# Ralph-workflow artifacts (auto-generated)\n");
            for entry in &entries_added {
                new_content.push_str(entry);
                new_content.push('\n');
            }

            // Write updated content
            match ctx.workspace.write(gitignore_path, &new_content) {
                Ok(()) => {
                    ctx.logger.success(&format!(
                        "Added {} entries to .gitignore: {}",
                        entries_added.len(),
                        entries_added.join(", ")
                    ));
                }
                Err(err) => {
                    // Log warning but don't fail pipeline
                    ctx.logger.warn(&format!(
                        "Failed to write .gitignore (continuing anyway): {}",
                        err
                    ));
                    // Clear entries_added since write failed
                    entries_added.clear();
                }
            }
        } else {
            ctx.logger
                .info("All required .gitignore entries already present");
        }

        Ok(EffectResult::event(
            PipelineEvent::gitignore_entries_ensured(entries_added, already_present, file_created),
        ))
    }
}

fn cleanup_continuation_context_file(ctx: &mut PhaseContext<'_>) -> anyhow::Result<()> {
    let path = Path::new(".agent/tmp/continuation_context.md");
    if ctx.workspace.exists(path) {
        ctx.workspace
            .remove(path)
            .map_err(|err| ErrorEvent::WorkspaceRemoveFailed {
                path: path.display().to_string(),
                kind: WorkspaceIoErrorKind::from_io_error_kind(err.kind()),
            })?;
    }
    Ok(())
}

/// Check if a gitignore pattern exists in the content.
///
/// Matches exact pattern on its own line (ignoring comments and whitespace).
fn entry_exists(content: &str, pattern: &str) -> bool {
    content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .any(|line| line == pattern)
}
