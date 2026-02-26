//! Development phase effect-to-event mapping.
//!
//! This module handles effect execution for the Development phase of the pipeline.
//! Development involves iteratively implementing changes based on the plan.
//!
//! ## Development Phase Flow
//!
//! 1. **`PrepareDevelopmentContext`** - Set up context for development iteration
//! 2. **`MaterializeDevelopmentInputs`** - Prepare prompt and plan inputs
//! 3. **`PrepareDevelopmentPrompt`** - Generate development prompt
//! 4. **`CleanupDevelopmentXml`** - Clean any existing XML
//! 5. **`InvokeDevelopmentAgent`** - Execute development agent
//! 6. **`InvokeAnalysisAgent`** - (Optional) Execute analysis agent for complex tasks
//! 7. **`ExtractDevelopmentXml`** - Extract XML from agent output
//! 8. **`ValidateDevelopmentXml`** - Validate XML and parse status (completed/partial/blocked)
//! 9. **`ArchiveDevelopmentXml`** - Archive XML for audit trail
//! 10. **`ApplyDevelopmentOutcome`** - Apply outcome to state
//!
//! ## Development Status
//!
//! The development agent can return three statuses:
//! - **Completed**: All work done, ready to proceed
//! - **Partial**: Some work done, more iterations needed
//! - **Blocked**: Cannot proceed, requires human intervention
//!
//! ## Mock Behavior
//!
//! Mock always returns "completed" status with a simple file change list.
//! This allows tests to verify successful development flow without real agent execution.

use crate::reducer::effect::Effect;
use crate::reducer::event::{DevelopmentEvent, PipelineEvent};
use crate::reducer::state::{
    DevelopmentStatus, MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};

use super::super::MockEffectHandler;

impl MockEffectHandler {
    /// Handle development phase effects.
    ///
    /// Returns appropriate mock events for each development effect without
    /// performing real agent execution, XML validation, or file I/O.
    pub(super) fn handle_development_effect(
        &self,
        effect: Effect,
    ) -> Option<(PipelineEvent, Vec<UIEvent>)> {
        match effect {
            Effect::PrepareDevelopmentContext { iteration } => Some((
                PipelineEvent::development_context_prepared(iteration),
                vec![],
            )),

            Effect::MaterializeDevelopmentInputs { iteration } => {
                let prompt = MaterializedPromptInput {
                    kind: PromptInputKind::Prompt,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: PromptInputRepresentation::Inline,
                    reason: PromptMaterializationReason::WithinBudgets,
                };
                let plan = MaterializedPromptInput {
                    kind: PromptInputKind::Plan,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: self.state.agent_chain.consumer_signature_sha256(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: PromptInputRepresentation::Inline,
                    reason: PromptMaterializationReason::WithinBudgets,
                };
                Some((
                    PipelineEvent::development_inputs_materialized(iteration, prompt, plan),
                    vec![],
                ))
            }

            Effect::PrepareDevelopmentPrompt {
                iteration,
                prompt_mode: _,
            } => Some((
                PipelineEvent::development_prompt_prepared(iteration),
                vec![],
            )),

            Effect::CleanupDevelopmentXml { iteration } => {
                Some((PipelineEvent::development_xml_cleaned(iteration), vec![]))
            }

            Effect::InvokeDevelopmentAgent { iteration } => {
                Some((PipelineEvent::development_agent_invoked(iteration), vec![]))
            }

            Effect::InvokeAnalysisAgent { iteration } => Some((
                PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration }),
                vec![],
            )),

            Effect::ExtractDevelopmentXml { iteration } => {
                let mock_dev_result_xml = r"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Mock development iteration completed successfully</ralph-summary>
<ralph-files-changed>src/test.rs
src/lib.rs</ralph-files-changed>
</ralph-development-result>";
                let ui = vec![
                    UIEvent::IterationProgress {
                        current: iteration,
                        total: self.state.total_iterations,
                    },
                    UIEvent::XmlOutput {
                        xml_type: XmlOutputType::DevelopmentResult,
                        content: mock_dev_result_xml.to_string(),
                        context: Some(XmlOutputContext {
                            iteration: Some(iteration),
                            pass: None,
                            snippets: Vec::new(),
                        }),
                    },
                ];
                Some((PipelineEvent::development_xml_extracted(iteration), ui))
            }

            Effect::ValidateDevelopmentXml { iteration } => Some((
                PipelineEvent::development_xml_validated(
                    iteration,
                    DevelopmentStatus::Completed,
                    "Mock development iteration completed successfully".to_string(),
                    Some(vec!["src/test.rs".to_string(), "src/lib.rs".to_string()]),
                    None,
                ),
                vec![],
            )),

            Effect::ArchiveDevelopmentXml { iteration } => {
                Some((PipelineEvent::development_xml_archived(iteration), vec![]))
            }

            Effect::ApplyDevelopmentOutcome { iteration } => Some((
                PipelineEvent::development_outcome_applied(iteration),
                vec![],
            )),

            _ => None,
        }
    }
}
