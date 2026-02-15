//! Lifecycle event handlers for pipeline completion and dev-fix flows.
//!
//! This module implements handlers for pipeline lifecycle events including:
//! - Dev-fix flow triggering when pipeline failures occur
//! - Completion marker emission for pipeline termination
//!
//! # Dev-Fix Flow
//!
//! When the pipeline detects a failure (agent exhaustion, validation failures),
//! it triggers the dev-fix flow to attempt automated remediation:
//!
//! 1. Write failure completion marker
//! 2. Prepare dev-fix prompt with failure context
//! 3. Invoke dev-fix agent
//! 4. Emit events for reducer to track dev-fix state
//!
//! # Completion Markers
//!
//! Completion markers are written to `.agent/tmp/completion_marker` to signal
//! pipeline termination state (success/failure) to external orchestrators.

use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use anyhow::Result;

use super::MainEffectHandler;

impl MainEffectHandler {
    /// Trigger dev-fix flow for pipeline failure remediation.
    ///
    /// This handler executes when the pipeline encounters an unrecoverable failure
    /// (agent chain exhaustion, quota limits, etc.). It attempts automated remediation
    /// by invoking a dev-fix agent with failure context.
    ///
    /// # Process
    ///
    /// 1. Write failure completion marker immediately
    /// 2. Load prompt/plan context from workspace
    /// 3. Generate dev-fix prompt with failure diagnostics
    /// 4. Invoke dev-fix agent
    /// 5. Emit events based on agent availability and outcome
    ///
    /// # Events Emitted
    ///
    /// - `DevFixTriggered`: Dev-fix flow initiated
    /// - `DevFixAgentUnavailable`: Agent quota/rate limit exceeded (if applicable)
    /// - `CompletionMarkerEmitted`: Failure marker written
    /// - Additional agent events from invocation
    ///
    /// # Arguments
    ///
    /// * `ctx` - Phase context with workspace and logging
    /// * `failed_phase` - Phase where failure occurred
    /// * `failed_role` - Agent role that failed
    /// * `retry_cycle` - Retry cycle count at failure
    pub(super) fn trigger_dev_fix_flow(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        failed_phase: PipelinePhase,
        failed_role: crate::agents::AgentRole,
        retry_cycle: u32,
    ) -> Result<EffectResult> {
        ctx.logger.error("⚠️  PIPELINE FAILURE DETECTED ⚠️");
        ctx.logger.warn(&format!(
            "Pipeline failure detected (phase: {}, role: {:?}, cycle: {})",
            failed_phase, failed_role, retry_cycle
        ));
        ctx.logger.info("Entering AwaitingDevFix flow...");
        ctx.logger
            .info("Dispatching dev-fix agent for remediation...");

        // Helper to read workspace files with fallback on error
        let read_or_fallback = |path: &str, label: &str| -> String {
            match ctx.workspace.read(std::path::Path::new(path)) {
                Ok(content) => content,
                Err(err) => {
                    ctx.logger.warn(&format!(
                        "Dev-fix prompt fallback: failed to read {}: {}",
                        label, err
                    ));
                    format!("(Missing {}: {})", label, err)
                }
            }
        };

        let prompt_content = read_or_fallback("PROMPT.md", "PROMPT.md");
        let plan_content = read_or_fallback(".agent/PLAN.md", ".agent/PLAN.md");
        let issues_content = format!(
            "# Issues\n\n- [High] Pipeline failure (phase: {}, role: {:?}, cycle: {}).\n  Diagnose the root cause and fix the failure.\n",
            failed_phase, failed_role, retry_cycle
        );
        let dev_fix_prompt = crate::prompts::prompt_fix_with_context(
            ctx.template_context,
            &prompt_content,
            &plan_content,
            &issues_content,
            ctx.workspace,
        );

        if let Err(err) = ctx.workspace.write(
            std::path::Path::new(".agent/tmp/dev_fix_prompt.txt"),
            &dev_fix_prompt,
        ) {
            ctx.logger.warn(&format!(
                "Failed to write dev-fix prompt to workspace: {}",
                err
            ));
        }

        let agent = self
            .state
            .agent_chain
            .current_agent()
            .cloned()
            .unwrap_or_else(|| ctx.developer_agent.to_string());

        let completion_marker_content = format!(
            "failure\nPipeline failure: phase={}, role={:?}, cycle={}",
            failed_phase, failed_role, retry_cycle
        );
        Self::write_completion_marker(ctx, &completion_marker_content, true);

        /// Helper function to detect agent unavailability from error messages.
        /// Checks for quota/usage/rate limit indicators in error text.
        fn is_agent_unavailable_error(err_msg: &str) -> bool {
            let err_msg_lower = err_msg.to_lowercase();
            err_msg_lower.contains("usage limit")
                || err_msg_lower.contains("quota exceeded")
                || err_msg_lower.contains("rate limit")
        }

        let agent_result = match self.invoke_agent(
            ctx,
            crate::agents::AgentRole::Developer,
            agent,
            None,
            dev_fix_prompt,
        ) {
            Ok(result) => Ok(result),
            Err(err) => {
                let unavailable = is_agent_unavailable_error(&err.to_string());

                if unavailable {
                    ctx.logger.warn(&format!(
                        "Dev-fix agent unavailable: {}. Pipeline will terminate with failure marker.",
                        err
                    ));
                } else {
                    ctx.logger
                        .warn(&format!("Dev-fix agent invocation failed: {}", err));
                }
                Err(err)
            }
        };

        let is_agent_unavailable = agent_result
            .as_ref()
            .err()
            .map(|err| is_agent_unavailable_error(&err.to_string()))
            .unwrap_or(false);

        // Dev-fix success cannot be determined at invocation time - it requires
        // extraction and validation of fix_result.xml. The InvocationSucceeded event
        // only indicates the agent started successfully, not that the fix completed.
        // DevFixCompleted will be emitted by the reducer after validation.

        // Extract error reason for logging and summary
        let error_reason = agent_result.as_ref().err().map(|e| e.to_string());

        let mut result = match agent_result.as_ref() {
            Ok(result) => EffectResult::with_ui(
                PipelineEvent::AwaitingDevFix(
                    crate::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                        failed_phase,
                        failed_role,
                    },
                ),
                result.ui_events.clone(),
            ),
            Err(_) => EffectResult::event(PipelineEvent::AwaitingDevFix(
                crate::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                    failed_phase,
                    failed_role,
                },
            )),
        };

        // Add any additional events from the agent result if it succeeded
        if let Ok(ref result_events) = agent_result {
            result = result.with_additional_event(result_events.event.clone());
            for event in &result_events.additional_events {
                result = result.with_additional_event(event.clone());
            }
        }

        // Emit appropriate event based on agent availability.
        // CompletionMarkerEmitted is ALWAYS emitted because the marker is
        // written unconditionally at the start of TriggerDevFixFlow.
        if is_agent_unavailable {
            // Agent unavailable (quota/usage limit)
            result = result.with_additional_event(PipelineEvent::AwaitingDevFix(
                crate::reducer::event::AwaitingDevFixEvent::DevFixAgentUnavailable {
                    failed_phase,
                    reason: error_reason.unwrap_or_else(|| "unknown".to_string()),
                },
            ));
        }
        // Note: DevFixCompleted is NOT emitted here. The success of the dev-fix
        // attempt can only be determined after fix_result.xml is extracted and
        // validated, which happens in a later phase (during XML output extraction).
        // The reducer will emit DevFixCompleted with the proper success status
        // after validation succeeds or fails.

        // CompletionMarkerEmitted is NOT emitted here. The reducer will decide
        // whether to continue with recovery or emit completion marker based on
        // recovery escalation level and attempt count. Only catastrophic failures
        // (external events, not internal pipeline errors) should trigger immediate
        // completion marker emission.

        Ok(result)
    }

    /// Emit completion marker and terminate pipeline.
    ///
    /// This handler writes a completion marker to signal pipeline termination
    /// state (success/failure) to external orchestrators or monitoring systems.
    ///
    /// # Completion Marker Format
    ///
    /// Success: `success\n`
    /// Failure: `failure\n<reason>`
    ///
    /// # Arguments
    ///
    /// * `ctx` - Phase context with workspace access
    /// * `is_failure` - Whether this is a failure termination
    /// * `reason` - Optional failure reason (ignored for success)
    pub(super) fn emit_completion_marker_and_terminate(
        ctx: &PhaseContext<'_>,
        is_failure: bool,
        reason: Option<String>,
    ) -> Result<EffectResult> {
        // Write completion marker to .agent/tmp/completion_marker
        let content = if is_failure {
            format!(
                "failure\n{}",
                reason.unwrap_or_else(|| "unknown".to_string())
            )
        } else {
            "success\n".to_string()
        };

        Self::write_completion_marker(ctx, &content, is_failure);

        // Emit event to transition to Interrupted
        Ok(EffectResult::event(PipelineEvent::AwaitingDevFix(
            crate::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted { is_failure },
        )))
    }
}
