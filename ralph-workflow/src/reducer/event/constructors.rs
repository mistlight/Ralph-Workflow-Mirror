// NOTE: split from reducer/event.rs to keep the facade small.
// Constructors are further split by event category to keep files under 500 lines.
use super::*;

// Include constructor implementations split by category
include!("constructors_lifecycle.rs");
include!("constructors_prompt_input.rs");
include!("constructors_development.rs");
include!("constructors_review.rs");
include!("constructors_agent.rs");
include!("constructors_commit.rs");

// ============================================================================
// Miscellaneous event constructors
// ============================================================================

impl PipelineEvent {
    /// Construct a LoopRecoveryTriggered event.
    pub fn loop_recovery_triggered(detected_loop: String, loop_count: u32) -> Self {
        PipelineEvent::LoopRecoveryTriggered {
            detected_loop,
            loop_count,
        }
    }

    /// Create a GitignoreEntriesEnsured event.
    pub fn gitignore_entries_ensured(
        entries_added: Vec<String>,
        already_present: Vec<String>,
        file_created: bool,
    ) -> Self {
        Self::Lifecycle(LifecycleEvent::GitignoreEntriesEnsured {
            added: entries_added,
            existing: already_present,
            created: file_created,
        })
    }
}
