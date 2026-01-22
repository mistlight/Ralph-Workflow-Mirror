//! Pipeline state types for reducer architecture.
//!
//! Defines immutable state structures that capture complete pipeline execution context.
//! These state structures can be serialized as checkpoints for resume functionality.

use crate::agents::AgentRole;
use crate::checkpoint::execution_history::ExecutionStep;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::event::PipelinePhase;

/// Immutable pipeline state (this IS the checkpoint).
///
/// Contains all information needed to resume pipeline execution at any point.
/// The reducer updates this state by returning new immutable copies on each event.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineState {
    pub phase: PipelinePhase,
    pub iteration: u32,
    pub total_iterations: u32,
    pub reviewer_pass: u32,
    pub total_reviewer_passes: u32,
    pub agent_chain: AgentChainState,
    pub rebase: RebaseState,
    pub commit: CommitState,
    pub execution_history: Vec<ExecutionStep>,
}

impl PipelineState {
    pub fn initial(developer_iters: u32, reviewer_reviews: u32) -> Self {
        // Determine initial phase based on what work needs to be done
        let initial_phase = if developer_iters == 0 {
            // No development iterations → skip Planning and Development
            if reviewer_reviews == 0 {
                // No review passes either → go straight to commit
                PipelinePhase::CommitMessage
            } else {
                PipelinePhase::Review
            }
        } else {
            PipelinePhase::Planning
        };

        Self {
            phase: initial_phase,
            iteration: 0,
            total_iterations: developer_iters,
            reviewer_pass: 0,
            total_reviewer_passes: reviewer_reviews,
            agent_chain: AgentChainState::initial(),
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            execution_history: Vec::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.phase,
            PipelinePhase::Complete | PipelinePhase::Interrupted
        )
    }

    pub fn current_head(&self) -> String {
        self.rebase
            .current_head()
            .unwrap_or_else(|| "HEAD".to_string())
    }
}

/// Agent fallback chain state (explicit, not loop indices).
///
/// Tracks position in the multi-level fallback chain:
/// - Agent level (primary → fallback1 → fallback2)
/// - Model level (within each agent, try different models)
/// - Retry cycle (exhaust all agents, start over with exponential backoff)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentChainState {
    pub agents: Vec<String>,
    pub current_agent_index: usize,
    pub models_per_agent: Vec<Vec<String>>,
    pub current_model_index: usize,
    pub retry_cycle: u32,
    pub max_cycles: u32,
    pub current_role: AgentRole,
}

impl AgentChainState {
    pub fn initial() -> Self {
        Self {
            agents: Vec::new(),
            current_agent_index: 0,
            models_per_agent: Vec::new(),
            current_model_index: 0,
            retry_cycle: 0,
            max_cycles: 3,
            current_role: AgentRole::Developer,
        }
    }

    pub fn with_agents(
        mut self,
        agents: Vec<String>,
        models_per_agent: Vec<Vec<String>>,
        role: AgentRole,
    ) -> Self {
        self.agents = agents;
        self.models_per_agent = models_per_agent;
        self.current_role = role;
        self
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn with_max_cycles(mut self, max_cycles: u32) -> Self {
        self.max_cycles = max_cycles;
        self
    }

    pub fn current_agent(&self) -> Option<&String> {
        self.agents.get(self.current_agent_index)
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn current_model(&self) -> Option<&String> {
        self.models_per_agent
            .get(self.current_agent_index)
            .and_then(|models| models.get(self.current_model_index))
    }

    pub fn is_exhausted(&self) -> bool {
        self.retry_cycle >= self.max_cycles
            && self.current_agent_index == 0
            && self.current_model_index == 0
    }

    pub fn advance_to_next_model(&self) -> Self {
        let mut new = self.clone();
        if let Some(models) = new.models_per_agent.get(new.current_agent_index) {
            if new.current_model_index + 1 < models.len() {
                new.current_model_index += 1;
            } else {
                new.current_model_index = 0;
            }
        }
        new
    }

    pub fn switch_to_next_agent(&self) -> Self {
        let mut new = self.clone();
        if new.current_agent_index + 1 < new.agents.len() {
            new.current_agent_index += 1;
            new.current_model_index = 0;
        } else {
            new.current_agent_index = 0;
            new.current_model_index = 0;
            new.retry_cycle += 1;
        }
        new
    }

    pub fn reset_for_role(&self, role: AgentRole) -> Self {
        let mut new = self.clone();
        new.current_role = role;
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.retry_cycle = 0;
        new
    }

    pub fn reset(&self) -> Self {
        let mut new = self.clone();
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new
    }

    pub fn start_retry_cycle(&self) -> Self {
        let mut new = self.clone();
        new.current_agent_index = 0;
        new.current_model_index = 0;
        new.retry_cycle += 1;
        new
    }
}

/// Rebase operation state.
///
/// Tracks rebase progress through the state machine:
/// NotStarted → InProgress → Conflicted → Completed/Skipped
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RebaseState {
    NotStarted,
    InProgress {
        original_head: String,
        target_branch: String,
    },
    Conflicted {
        original_head: String,
        target_branch: String,
        files: Vec<PathBuf>,
        resolution_attempts: u32,
    },
    Completed {
        new_head: String,
    },
    Skipped,
}

impl RebaseState {
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(self, RebaseState::Completed { .. } | RebaseState::Skipped)
    }

    pub fn current_head(&self) -> Option<String> {
        match self {
            RebaseState::NotStarted | RebaseState::Skipped => None,
            RebaseState::InProgress { original_head, .. } => Some(original_head.clone()),
            RebaseState::Conflicted { .. } => None,
            RebaseState::Completed { new_head } => Some(new_head.clone()),
        }
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            RebaseState::InProgress { .. } | RebaseState::Conflicted { .. }
        )
    }
}

/// Commit generation state.
///
/// Tracks commit message generation progress through retries:
/// NotStarted → Generating → Generated → Committed/Skipped
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CommitState {
    NotStarted,
    Generating { attempt: u32, max_attempts: u32 },
    Generated { message: String },
    Committed { hash: String },
    Skipped,
}

impl CommitState {
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(self, CommitState::Committed { .. } | CommitState::Skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_state_initial() {
        let state = PipelineState::initial(5, 2);
        assert_eq!(state.phase, PipelinePhase::Planning);
        assert_eq!(state.total_iterations, 5);
        assert_eq!(state.total_reviewer_passes, 2);
        assert!(!state.is_complete());
    }

    #[test]
    fn test_agent_chain_initial() {
        let chain = AgentChainState::initial();
        assert!(chain.agents.is_empty());
        assert_eq!(chain.current_agent_index, 0);
        assert_eq!(chain.retry_cycle, 0);
    }

    #[test]
    fn test_agent_chain_with_agents() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string(), "codex".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        assert_eq!(chain.agents.len(), 2);
        assert_eq!(chain.current_agent(), Some(&"claude".to_string()));
        assert_eq!(chain.max_cycles, 3);
    }

    #[test]
    fn test_agent_chain_advance_to_next_model() {
        let chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        );

        let new_chain = chain.advance_to_next_model();
        assert_eq!(new_chain.current_model_index, 1);
        assert_eq!(new_chain.current_model(), Some(&"model2".to_string()));
    }

    #[test]
    fn test_agent_chain_switch_to_next_agent() {
        let chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string(), "codex".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        let new_chain = chain.switch_to_next_agent();
        assert_eq!(new_chain.current_agent_index, 1);
        assert_eq!(new_chain.current_agent(), Some(&"codex".to_string()));
        assert_eq!(new_chain.retry_cycle, 0);
    }

    #[test]
    fn test_agent_chain_exhausted() {
        let chain = AgentChainState::initial()
            .with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        let chain = chain.start_retry_cycle();
        let chain = chain.start_retry_cycle();
        let chain = chain.start_retry_cycle();

        assert!(chain.is_exhausted());
    }

    #[test]
    fn test_rebase_state_not_started() {
        let state = RebaseState::NotStarted;
        assert!(!state.is_terminal());
        assert!(state.current_head().is_none());
        assert!(!state.is_in_progress());
    }

    #[test]
    fn test_rebase_state_in_progress() {
        let state = RebaseState::InProgress {
            original_head: "abc123".to_string(),
            target_branch: "main".to_string(),
        };
        assert!(!state.is_terminal());
        assert_eq!(state.current_head(), Some("abc123".to_string()));
        assert!(state.is_in_progress());
    }

    #[test]
    fn test_rebase_state_completed() {
        let state = RebaseState::Completed {
            new_head: "def456".to_string(),
        };
        assert!(state.is_terminal());
        assert_eq!(state.current_head(), Some("def456".to_string()));
        assert!(!state.is_in_progress());
    }

    #[test]
    fn test_commit_state_not_started() {
        let state = CommitState::NotStarted;
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_commit_state_generating() {
        let state = CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        };
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_commit_state_committed() {
        let state = CommitState::Committed {
            hash: "abc123".to_string(),
        };
        assert!(state.is_terminal());
    }
}
