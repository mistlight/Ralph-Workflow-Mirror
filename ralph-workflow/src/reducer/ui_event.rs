//! UI events for user-facing display.
//!
//! UIEvent is separate from PipelineEvent to maintain reducer purity.
//! These events are emitted by effect handlers alongside PipelineEvents
//! and are displayed to users but do not affect pipeline state or checkpoints.

use super::event::PipelinePhase;
use serde::{Deserialize, Serialize};

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
    pub fn format_for_display(&self) -> String {
        match self {
            UIEvent::PhaseTransition { to, .. } => {
                format!("{} {}", Self::phase_emoji(to), to)
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
        }
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
}
