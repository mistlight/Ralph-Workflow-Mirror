//! Review phase effect-to-event mapping.
//!
//! This module handles effect execution for the Review phase of the pipeline.
//! Review involves analyzing implemented changes for issues and optionally fixing them.
//!
//! ## Review Phase Flow
//!
//! ### Review Pass
//! 1. **`PrepareReviewContext`** - Set up context for review pass
//! 2. **`MaterializeReviewInputs`** - Prepare plan and diff inputs
//! 3. **`PrepareReviewPrompt`** - Generate review prompt
//! 4. **`CleanupRequiredFiles`** - Clean any existing XML (handled in `lifecycle_effects`)
//! 5. **`InvokeReviewAgent`** - Execute review agent
//! 6. **`ExtractReviewIssuesXml`** - Extract XML from agent output
//! 7. **`ValidateReviewIssuesXml`** - Validate and parse issues
//! 8. **`WriteIssuesMarkdown`** - Convert issues to markdown
//! 9. **`ExtractReviewIssueSnippets`** - Extract code snippets from issues
//! 10. **`ArchiveReviewIssuesXml`** - Archive XML
//! 11. **`ApplyReviewOutcome`** - Apply outcome (found issues or clean)
//!
//! ### Fix Pass (if issues found)
//! 1. **`PrepareFixPrompt`** - Generate fix prompt with issues
//! 2. **`CleanupRequiredFiles`** - Clean any existing XML (handled in `lifecycle_effects`)
//! 3. **`InvokeFixAgent`** - Execute fix agent
//! 4. **`ExtractFixResultXml`** - Extract XML from agent output
//! 5. **`ValidateFixResultXml`** - Validate and parse fix status
//! 6. **`ApplyFixOutcome`** - Apply fix outcome
//! 7. **`ArchiveFixResultXml`** - Archive XML
//!
//! ## Fix Status
//!
//! The fix agent can return:
//! - **`AllIssuesAddressed`**: All issues fixed, ready to proceed
//! - **`PartialProgress`**: Some issues fixed, more passes needed
//! - **`CannotFix`**: Issues cannot be automatically fixed
//!
//! ## Mock Behavior
//!
//! Mock always returns "no issues found" for review and "all issues addressed" for fix.
//! This allows tests to verify successful review flow without real agent execution.

use crate::reducer::effect::Effect;
use crate::reducer::event::PipelineEvent;
use crate::reducer::state::{
    FixStatus, MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use crate::reducer::ui_event::{UIEvent, XmlOutputContext, XmlOutputType};

use super::super::MockEffectHandler;

impl MockEffectHandler {
    /// Handle review phase effects.
    ///
    /// Returns appropriate mock events for each review effect without
    /// performing real agent execution, XML validation, or file I/O.
    pub(super) fn handle_review_effect(
        &self,
        effect: &Effect,
    ) -> Option<(PipelineEvent, Vec<UIEvent>)> {
        match *effect {
            Effect::PrepareReviewContext { pass } => {
                Some((
                    PipelineEvent::review_context_prepared(pass),
                    vec![UIEvent::ReviewProgress {
                        pass,
                        total: self.state.total_reviewer_passes,
                    }],
                ))
            }

            Effect::MaterializeReviewInputs { pass } => {
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
                let diff = MaterializedPromptInput {
                    kind: PromptInputKind::Diff,
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
                    PipelineEvent::review_inputs_materialized(pass, plan, diff),
                    vec![],
                ))
            }

            Effect::PrepareReviewPrompt {
                pass,
                prompt_mode: _,
            } => Some((PipelineEvent::review_prompt_prepared(pass), vec![])),

            Effect::InvokeReviewAgent { pass } => {
                // In mock mode we only emit the review-specific progress event.
                Some((PipelineEvent::review_agent_invoked(pass), vec![]))
            }

            Effect::ExtractReviewIssuesXml { pass } => {
                Some((PipelineEvent::review_issues_xml_extracted(pass), vec![]))
            }

            Effect::ValidateReviewIssuesXml { pass } => {
                Some((
                    PipelineEvent::review_issues_xml_validated(
                        pass,
                        false,
                        true,
                        Vec::new(),
                        Some("ok".to_string()),
                    ),
                    vec![UIEvent::XmlOutput {
                        xml_type: XmlOutputType::ReviewIssues,
                        content: r"<ralph-issues><ralph-no-issues-found>ok</ralph-no-issues-found></ralph-issues>"
                            .to_string(),
                        context: Some(XmlOutputContext {
                            iteration: None,
                            pass: Some(pass),
                            snippets: Vec::new(),
                        }),
                    }],
                ))
            }

            Effect::WriteIssuesMarkdown { pass } => {
                Some((PipelineEvent::review_issues_markdown_written(pass), vec![]))
            }

            Effect::ExtractReviewIssueSnippets { pass } => Some((
                PipelineEvent::review_issue_snippets_extracted(pass),
                vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::ReviewIssues,
                    content: r"<ralph-issues><ralph-no-issues-found>ok</ralph-no-issues-found></ralph-issues>"
                        .to_string(),
                    context: Some(XmlOutputContext {
                        iteration: None,
                        pass: Some(pass),
                        snippets: Vec::new(),
                    }),
                }],
            )),

            Effect::ArchiveReviewIssuesXml { pass } => {
                Some((PipelineEvent::review_issues_xml_archived(pass), vec![]))
            }

            Effect::ApplyReviewOutcome {
                pass,
                issues_found,
                clean_no_issues,
            } => {
                if clean_no_issues {
                    Some((PipelineEvent::review_pass_completed_clean(pass), vec![]))
                } else {
                    Some((PipelineEvent::review_completed(pass, issues_found), vec![]))
                }
            }

            _ => None,
        }
    }

    /// Handle fix phase effects (part of review cycle).
    ///
    /// Returns appropriate mock events for each fix effect without
    /// performing real agent execution, XML validation, or file I/O.
    pub(super) fn handle_fix_effect(effect: &Effect) -> Option<(PipelineEvent, Vec<UIEvent>)> {
        match *effect {
            Effect::PrepareFixPrompt {
                pass,
                prompt_mode: _,
            } => Some((PipelineEvent::fix_prompt_prepared(pass), vec![])),

            Effect::InvokeFixAgent { pass } => {
                Some((PipelineEvent::fix_agent_invoked(pass), vec![]))
            }

            Effect::ExtractFixResultXml { pass } => {
                Some((PipelineEvent::fix_result_xml_extracted(pass), vec![]))
            }

            Effect::ValidateFixResultXml { pass } => Some((
                PipelineEvent::fix_result_xml_validated(pass, FixStatus::AllIssuesAddressed, None),
                vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::FixResult,
                    content: r"<ralph-fix-result><ralph-status>all_issues_addressed</ralph-status></ralph-fix-result>"
                        .to_string(),
                    context: Some(XmlOutputContext {
                        iteration: None,
                        pass: Some(pass),
                        snippets: Vec::new(),
                    }),
                }],
            )),

            Effect::ApplyFixOutcome { pass } => {
                Some((PipelineEvent::fix_outcome_applied(pass), vec![]))
            }

            Effect::ArchiveFixResultXml { pass } => {
                Some((PipelineEvent::fix_result_xml_archived(pass), vec![]))
            }

            _ => None,
        }
    }
}
