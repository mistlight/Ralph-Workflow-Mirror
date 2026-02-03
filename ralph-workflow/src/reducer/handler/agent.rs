use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use crate::pipeline::PipelineRuntime;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::PipelineEvent;
use crate::reducer::event::PipelinePhase;
use crate::reducer::fault_tolerant_executor::{
    execute_agent_fault_tolerantly, AgentExecutionConfig, AgentExecutionResult,
};
use crate::reducer::ui_event::UIEvent;
use anyhow::Result;

impl MainEffectHandler {
    pub(super) fn invoke_agent(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        role: AgentRole,
        agent: String,
        model: Option<String>,
        prompt: String,
    ) -> Result<EffectResult> {
        // Use agent from state.agent_chain if available
        let effective_agent = self
            .state
            .agent_chain
            .current_agent()
            .unwrap_or(&agent)
            .clone();

        // Use continuation prompt if available (from rate-limited predecessor).
        //
        // When an agent hits a rate limit, the prompt is saved in
        // rate_limit_continuation_prompt. On the next invocation (with a new
        // agent), we use this saved prompt to continue the same work.
        // The `InvocationSucceeded` event handler clears the saved prompt
        // in the reducer, so we don't need to handle that here.
        //
        let effective_prompt = self
            .state
            .agent_chain
            .rate_limit_continuation_prompt
            .clone()
            .unwrap_or(prompt);

        let model_name = self.state.agent_chain.current_model();

        ctx.logger.info(&format!(
            "Executing with agent: {}, model: {:?}",
            effective_agent, model_name
        ));

        // Get agent configuration from registry
        let agent_config = ctx
            .registry
            .resolve_config(&effective_agent)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", effective_agent))?;

        // Determine log file path.
        //
        // Logs must uniquely identify the invocation attempt to avoid collisions across:
        // - model fallback (model index)
        // - agent fallback cycles (retry_cycle)
        // - XSD retries and continuation attempts
        let phase_prefix = match self.state.phase {
            PipelinePhase::Planning => format!(".agent/logs/planning_{}", self.state.iteration + 1),
            PipelinePhase::Development => {
                format!(".agent/logs/developer_{}", self.state.iteration + 1)
            }
            PipelinePhase::Review => {
                format!(".agent/logs/reviewer_{}", self.state.reviewer_pass + 1)
            }
            PipelinePhase::CommitMessage => {
                let commit_attempt = match &self.state.commit {
                    crate::reducer::state::CommitState::Generating { attempt, .. } => *attempt,
                    _ => 1,
                };
                format!(".agent/logs/commit_{commit_attempt}")
            }
            PipelinePhase::FinalValidation => ".agent/logs/final_validation".to_string(),
            PipelinePhase::Finalizing => ".agent/logs/finalizing".to_string(),
            PipelinePhase::Complete => ".agent/logs/complete".to_string(),
            PipelinePhase::Interrupted => ".agent/logs/interrupted".to_string(),
        };

        // Determine a collision-free logfile attempt index.
        //
        // Rationale: The reducer tracks multiple retry counters (retry_cycle,
        // continuation_attempt, xsd_retry_count). Packing them into a single
        // arithmetic attempt value can collide when counters exceed assumed
        // bounds, causing later attempts to overwrite earlier logs.
        //
        // We avoid collisions by scanning existing logs for this
        // `(phase_prefix, agent, model_index)` family and using the next
        // available attempt index.
        let model_index = self.state.agent_chain.current_model_index;
        let agent_for_log = effective_agent.to_lowercase();
        let attempt = crate::pipeline::logfile::next_logfile_attempt_index(
            std::path::Path::new(&phase_prefix),
            &agent_for_log,
            model_index,
            ctx.workspace,
        );

        let logfile = crate::pipeline::logfile::build_logfile_path_with_attempt(
            &phase_prefix,
            &agent_for_log,
            model_index,
            attempt,
        );

        // Build command string, honoring reducer-selected model (if any).
        // The reducer's agent chain drives model fallback (advance_to_next_model).
        // When present, the selected model must be threaded into the command.
        let model_override = model_name
            .map(std::string::String::as_str)
            .or(model.as_deref());

        // Session ID reuse for XSD retry: preserve a "reuse session id" signal across
        // prompt preparation (which clears xsd_retry_pending to avoid effect loops).
        let session_id = if self.state.continuation.xsd_retry_session_reuse_pending {
            self.state.agent_chain.last_session_id.as_deref()
        } else {
            None
        };

        let cmd_str =
            agent_config.build_cmd_with_session(true, true, true, model_override, session_id);

        // Build pipeline runtime
        let mut runtime = PipelineRuntime {
            timer: ctx.timer,
            logger: ctx.logger,
            colors: ctx.colors,
            config: ctx.config,
            executor: ctx.executor,
            executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
            workspace: ctx.workspace,
        };

        let started_event = PipelineEvent::agent_invocation_started(
            role,
            effective_agent.clone(),
            model_name.cloned().or(model.clone()),
        );

        // Execute agent with fault-tolerant wrapper
        let config = AgentExecutionConfig {
            role,
            agent_name: &effective_agent,
            cmd_str: &cmd_str,
            parser_type: agent_config.json_parser,
            env_vars: &agent_config.env_vars,
            prompt: &effective_prompt,
            display_name: &effective_agent,
            log_prefix: &phase_prefix,
            model_index,
            attempt,
            logfile: &logfile,
        };

        let AgentExecutionResult { event, session_id } =
            execute_agent_fault_tolerantly(config, &mut runtime)?;

        // Emit UI event for agent activity
        let ui_event = UIEvent::AgentActivity {
            agent: effective_agent.clone(),
            message: format!("Completed {} task", role),
        };

        // Build result with started event first, then the execution result(s).
        let mut result =
            EffectResult::with_ui(started_event, vec![ui_event]).with_additional_event(event);

        // If session_id was extracted, emit SessionEstablished as a separate event.
        if let Some(sid) = session_id {
            result = result.with_additional_event(PipelineEvent::agent_session_established(
                role,
                effective_agent.clone(),
                sid,
            ));
        }

        Ok(result)
    }
}
