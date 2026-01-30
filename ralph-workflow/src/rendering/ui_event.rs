//! UI event rendering dispatch.
//!
//! This is the single entrypoint for all UI event rendering.
//! The event loop calls `render_ui_event()` and displays the result.

use crate::reducer::event::PipelinePhase;
use crate::reducer::ui_event::UIEvent;

/// Render a UIEvent to a displayable string.
///
/// This is the single entrypoint for all UI event rendering.
/// The event loop calls this function and displays the result.
pub fn render_ui_event(event: &UIEvent) -> String {
    match event {
        UIEvent::PhaseTransition { to, .. } => {
            format!("{} {}", phase_emoji(to), to)
        }
        UIEvent::IterationProgress { current, total } => {
            format!("🔄 Development iteration {}/{}", current, total)
        }
        UIEvent::ReviewProgress { pass, total } => {
            format!("👁 Review pass {}/{}", pass, total)
        }
        UIEvent::AgentActivity { agent, message } => {
            format!("🤖 [{}] {}", agent, message)
        }
        UIEvent::XmlOutput {
            xml_type,
            content,
            context,
        } => super::xml::render_xml(xml_type, content, context),
    }
}

/// Get emoji indicator for phase.
fn phase_emoji(phase: &PipelinePhase) -> &'static str {
    match phase {
        PipelinePhase::Planning => "📋",
        PipelinePhase::Development => "🔨",
        PipelinePhase::Review => "👀",
        PipelinePhase::CommitMessage => "📝",
        PipelinePhase::FinalValidation => "✅",
        PipelinePhase::Finalizing => "🔄",
        PipelinePhase::Complete => "🎉",
        PipelinePhase::Interrupted => "⏸️",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::ui_event::{XmlOutputContext, XmlOutputType};

    #[test]
    fn test_render_phase_transition() {
        let event = UIEvent::PhaseTransition {
            from: Some(PipelinePhase::Planning),
            to: PipelinePhase::Development,
        };
        let output = render_ui_event(&event);
        assert!(output.contains("🔨"));
        assert!(output.contains("Development"));
    }

    #[test]
    fn test_render_iteration_progress() {
        let event = UIEvent::IterationProgress {
            current: 2,
            total: 5,
        };
        let output = render_ui_event(&event);
        assert!(output.contains("2/5"));
        assert!(output.contains("🔄"));
    }

    #[test]
    fn test_render_review_progress() {
        let event = UIEvent::ReviewProgress { pass: 1, total: 3 };
        let output = render_ui_event(&event);
        assert!(output.contains("1/3"));
        assert!(output.contains("👁"));
    }

    #[test]
    fn test_render_agent_activity() {
        let event = UIEvent::AgentActivity {
            agent: "claude".to_string(),
            message: "Processing request".to_string(),
        };
        let output = render_ui_event(&event);
        assert!(output.contains("[claude]"));
        assert!(output.contains("Processing request"));
    }

    #[test]
    fn test_render_xml_output_routes_to_xml_module() {
        let event = UIEvent::XmlOutput {
            xml_type: XmlOutputType::DevelopmentResult,
            content: r#"<ralph-development-result>
<ralph-status>completed</ralph-status>
<ralph-summary>Done</ralph-summary>
</ralph-development-result>"#
                .to_string(),
            context: Some(XmlOutputContext::default()),
        };
        let output = render_ui_event(&event);
        // Should be semantically rendered, not raw XML
        assert!(output.contains("✅") || output.contains("Completed"));
        assert!(output.contains("Done"));
    }

    #[test]
    fn test_phase_emoji_all_phases() {
        // Verify all phases have non-empty emojis
        let phases = [
            PipelinePhase::Planning,
            PipelinePhase::Development,
            PipelinePhase::Review,
            PipelinePhase::CommitMessage,
            PipelinePhase::FinalValidation,
            PipelinePhase::Finalizing,
            PipelinePhase::Complete,
            PipelinePhase::Interrupted,
        ];
        for phase in phases {
            let emoji = phase_emoji(&phase);
            assert!(!emoji.is_empty(), "Phase {:?} should have an emoji", phase);
        }
    }
}
