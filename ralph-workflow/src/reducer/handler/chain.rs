use super::MainEffectHandler;
use crate::agents::AgentRole;
use crate::phases::{get_primary_commit_agent, PhaseContext};
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use crate::reducer::ui_event::UIEvent;
use anyhow::Result;

impl MainEffectHandler {
    pub(super) fn initialize_agent_chain(
        &self,
        ctx: &PhaseContext<'_>,
        role: AgentRole,
    ) -> Result<EffectResult> {
        let fallback_config = ctx.registry.fallback_config();

        // Get the full fallback chain for this role from the FallbackConfig
        let mut agents = fallback_config.get_fallbacks(role).to_vec();

        // If no fallbacks configured, fall back to context agent
        if agents.is_empty() {
            let fallback_agent = match role {
                AgentRole::Developer => ctx.developer_agent.to_string(),
                AgentRole::Reviewer => ctx.reviewer_agent.to_string(),
                AgentRole::Commit => {
                    if let Some(commit_agent) = get_primary_commit_agent(ctx) {
                        commit_agent
                    } else {
                        return Ok(EffectResult::event(PipelineEvent::agent_chain_initialized(
                            role,
                            vec![],
                            fallback_config.max_cycles,
                            fallback_config.retry_delay_ms,
                            fallback_config.backoff_multiplier,
                            fallback_config.max_backoff_ms,
                        )));
                    }
                }
                AgentRole::Analysis => ctx.developer_agent.to_string(),
            };
            agents.push(fallback_agent);
        }

        ctx.logger.info(&format!(
            "Agent fallback chain for {:?}: {}",
            role,
            agents.join(", ")
        ));

        let event = PipelineEvent::agent_chain_initialized(
            role,
            agents,
            fallback_config.max_cycles,
            fallback_config.retry_delay_ms,
            fallback_config.backoff_multiplier,
            fallback_config.max_backoff_ms,
        );

        // Emit phase transition when entering a new major phase
        let ui_events = match role {
            AgentRole::Developer if self.state.phase == PipelinePhase::Planning => {
                vec![UIEvent::PhaseTransition {
                    from: None,
                    to: PipelinePhase::Planning,
                }]
            }
            AgentRole::Reviewer if self.state.phase == PipelinePhase::Review => {
                vec![self.phase_transition_ui(PipelinePhase::Review)]
            }
            _ => vec![],
        };

        Ok(EffectResult::with_ui(event, ui_events))
    }
}
