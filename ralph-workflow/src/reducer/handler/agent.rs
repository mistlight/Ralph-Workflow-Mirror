use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::phases::PhaseContext;
use crate::pipeline::PipelineRuntime;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::PipelineEvent;
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

        let model_name = self.state.agent_chain.current_model();

        // Use continuation prompt if available (from rate-limited predecessor).
        //
        // Important: only use it when it's the *same* prompt as this invocation.
        // If the pipeline has generated a new prompt (retry/fallback instructions,
        // different phase/role, etc.), do not override it with stale continuation
        // context.
        let effective_prompt = match self
            .state
            .agent_chain
            .rate_limit_continuation_prompt
            .as_ref()
        {
            Some(saved) if saved == &prompt => saved.clone(),
            _ => prompt,
        };

        ctx.logger.info(&format!(
            "Executing with agent: {}, model: {:?}",
            effective_agent, model_name
        ));

        // Get agent configuration from registry
        let agent_config = ctx
            .registry
            .resolve_config(&effective_agent)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", effective_agent))?;

        // Determine log file path
        let safe_agent_name =
            crate::pipeline::logfile::sanitize_agent_name(&effective_agent.to_lowercase());
        let logfile = format!(".agent/logs/{}.log", safe_agent_name);

        // Build command string, honoring reducer-selected model (if any).
        // The reducer's agent chain drives model fallback (advance_to_next_model).
        // When present, the selected model must be threaded into the command.
        let model_override = model_name
            .map(std::string::String::as_str)
            .or(model.as_deref());

        // Session ID reuse for XSD retry: when xsd_retry_pending is true and we have
        // a session ID from a previous invocation, reuse it for same-session retry.
        let session_id = if self.state.continuation.xsd_retry_pending {
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

        // Execute agent with fault-tolerant wrapper
        let config = AgentExecutionConfig {
            role,
            agent_name: &effective_agent,
            cmd_str: &cmd_str,
            parser_type: agent_config.json_parser,
            env_vars: &agent_config.env_vars,
            prompt: &effective_prompt,
            display_name: &effective_agent,
            logfile: &logfile,
        };

        let AgentExecutionResult { event, session_id } =
            execute_agent_fault_tolerantly(config, &mut runtime)?;

        // Emit UI event for agent activity
        let ui_event = UIEvent::AgentActivity {
            agent: effective_agent.clone(),
            message: format!("Completed {} task", role),
        };

        // Build result with the main event
        let mut result = EffectResult::with_ui(event, vec![ui_event]);

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
