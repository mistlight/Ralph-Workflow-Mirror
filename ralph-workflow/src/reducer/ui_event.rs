//! UI events for user-facing display.
//!
//! UIEvent is separate from PipelineEvent to maintain reducer purity.
//! These events are emitted by effect handlers alongside PipelineEvents
//! and are displayed to users but do not affect pipeline state or checkpoints.

use super::event::PipelinePhase;
use serde::{Deserialize, Serialize};

/// Types of XML output for semantic rendering.
///
/// Each XML type has a dedicated renderer that transforms raw XML
/// into user-friendly terminal output.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum XmlOutputType {
    /// Development result XML (status, summary, files changed).
    DevelopmentResult,
    /// Development plan XML (steps, critical files, risks).
    DevelopmentPlan,
    /// Review issues XML (list of issues or no-issues-found).
    ReviewIssues,
    /// Fix result XML (status, summary of fixes).
    FixResult,
    /// Commit message XML (subject, body).
    CommitMessage,
}

/// Context for XML output events.
///
/// Provides additional context like iteration or pass number
/// for more informative rendering.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct XmlOutputContext {
    /// Development iteration number (1-based).
    pub iteration: Option<u32>,
    /// Review pass number (1-based).
    pub pass: Option<u32>,
    /// Optional code snippets to enrich rendering (e.g., review issues).
    ///
    /// This allows semantic renderers to show relevant code context even when the
    /// issue description itself does not embed a fenced code block.
    #[serde(default)]
    pub snippets: Vec<XmlCodeSnippet>,
}

/// A code snippet associated with a file and line range.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct XmlCodeSnippet {
    /// File path (workspace-relative).
    pub file: String,
    /// 1-based starting line number (inclusive).
    pub line_start: u32,
    /// 1-based ending line number (inclusive).
    pub line_end: u32,
    /// Snippet content (may include newlines).
    pub content: String,
}

/// UI events for user-facing display during pipeline execution.
///
/// These events do NOT affect pipeline state or checkpoints.
/// They are purely for terminal display and programmatic observation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum UIEvent {
    /// Phase transition occurred.
    PhaseTransition {
        from: Option<PipelinePhase>,
        to: PipelinePhase,
    },

    /// Development iteration progress.
    IterationProgress { current: u32, total: u32 },

    /// Review pass progress.
    ReviewProgress { pass: u32, total: u32 },

    /// Agent activity notification.
    AgentActivity { agent: String, message: String },

    /// XML output requiring semantic rendering.
    ///
    /// Phase functions emit raw XML content through this event,
    /// and the event loop renders it with appropriate semantic formatting.
    XmlOutput {
        /// The type of XML output (determines renderer).
        xml_type: XmlOutputType,
        /// The raw XML content to render.
        content: String,
        /// Optional context like iteration or pass number.
        context: Option<XmlOutputContext>,
    },
}

impl UIEvent {
    /// Get emoji indicator for phase.
    pub fn phase_emoji(phase: &PipelinePhase) -> &'static str {
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

    /// Format event for terminal display.
    ///
    /// This method delegates to the rendering module for actual formatting.
    /// Prefer calling `rendering::render_ui_event()` directly in new code.
    pub fn format_for_display(&self) -> String {
        crate::rendering::render_ui_event(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_transition_display() {
        let event = UIEvent::PhaseTransition {
            from: Some(PipelinePhase::Planning),
            to: PipelinePhase::Development,
        };
        let display = event.format_for_display();
        assert!(display.contains("🔨"));
        assert!(display.contains("Development"));
    }

    #[test]
    fn test_iteration_progress_display() {
        let event = UIEvent::IterationProgress {
            current: 2,
            total: 5,
        };
        let display = event.format_for_display();
        assert!(display.contains("2/5"));
    }

    #[test]
    fn test_review_progress_display() {
        let event = UIEvent::ReviewProgress { pass: 1, total: 3 };
        let display = event.format_for_display();
        assert!(display.contains("1/3"));
        assert!(display.contains("Review pass"));
    }

    #[test]
    fn test_agent_activity_display() {
        let event = UIEvent::AgentActivity {
            agent: "claude".to_string(),
            message: "Processing request".to_string(),
        };
        let display = event.format_for_display();
        assert!(display.contains("[claude]"));
        assert!(display.contains("Processing request"));
    }

    #[test]
    fn test_ui_event_serialization() {
        let event = UIEvent::PhaseTransition {
            from: None,
            to: PipelinePhase::Planning,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: UIEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_phase_emoji_all_phases() {
        // Verify all phases have emojis
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::Planning), "📋");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::Development), "🔨");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::Review), "👀");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::CommitMessage), "📝");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::FinalValidation), "✅");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::Finalizing), "🔄");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::Complete), "🎉");
        assert_eq!(UIEvent::phase_emoji(&PipelinePhase::Interrupted), "⏸️");
    }

    #[test]
    fn test_phase_transition_from_none() {
        // Test initial phase transition with no previous phase
        let event = UIEvent::PhaseTransition {
            from: None,
            to: PipelinePhase::Planning,
        };
        let display = event.format_for_display();
        assert!(display.contains("📋"));
        assert!(display.contains("Planning"));
    }

    // =========================================================================
    // XmlOutput Tests
    // =========================================================================

    #[test]
    fn test_xml_output_type_serialization() {
        let xml_type = XmlOutputType::DevelopmentResult;
        let json = serde_json::to_string(&xml_type).unwrap();
        let deserialized: XmlOutputType = serde_json::from_str(&json).unwrap();
        assert_eq!(xml_type, deserialized);
    }

    #[test]
    fn test_xml_output_context_default() {
        let context = XmlOutputContext::default();
        assert!(context.iteration.is_none());
        assert!(context.pass.is_none());
        assert!(context.snippets.is_empty());
    }

    #[test]
    fn test_xml_output_context_with_values() {
        let context = XmlOutputContext {
            iteration: Some(2),
            pass: Some(1),
            snippets: Vec::new(),
        };
        assert_eq!(context.iteration, Some(2));
        assert_eq!(context.pass, Some(1));
    }

    #[test]
    fn test_xml_output_event_serialization() {
        let event = UIEvent::XmlOutput {
            xml_type: XmlOutputType::ReviewIssues,
            content: "<ralph-issues><ralph-issue>Test</ralph-issue></ralph-issues>".to_string(),
            context: Some(XmlOutputContext {
                iteration: None,
                pass: Some(1),
                snippets: Vec::new(),
            }),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: UIEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_xml_output_types_all_variants() {
        // Ensure all variants are distinct
        let variants = [
            XmlOutputType::DevelopmentResult,
            XmlOutputType::DevelopmentPlan,
            XmlOutputType::ReviewIssues,
            XmlOutputType::FixResult,
            XmlOutputType::CommitMessage,
        ];
        for (i, v1) in variants.iter().enumerate() {
            for (j, v2) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(v1, v2);
                } else {
                    assert_ne!(v1, v2);
                }
            }
        }
    }
}
