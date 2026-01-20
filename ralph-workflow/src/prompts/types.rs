//! Type definitions for prompts.
//!
//! This module contains the enums and types used to configure prompts
//! for different roles, actions, and context levels.

/// Context level for agents.
///
/// Controls how much context information is included in prompts.
/// Lower context helps maintain "fresh eyes" perspective for reviewers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLevel {
    /// Minimal context (fresh eyes) - only essential info
    Minimal = 0,
    /// Normal context - includes status information
    Normal = 1,
}

impl From<u8> for ContextLevel {
    fn from(v: u8) -> Self {
        if v == 0 {
            Self::Minimal
        } else {
            Self::Normal
        }
    }
}

/// Role types for agents.
///
/// Determines which type of agent is being configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Developer agent - implements features
    Developer,
    /// Reviewer agent - reviews and fixes issues
    Reviewer,
}

/// Action types for prompts.
///
/// Specifies what action the agent should perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Create an implementation plan
    #[cfg(any(test, feature = "test-utils"))]
    Plan,
    /// Execute an iteration of development
    Iterate,
    /// Fix issues found during review
    Fix,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_level_from_u8() {
        assert_eq!(ContextLevel::from(0), ContextLevel::Minimal);
        assert_eq!(ContextLevel::from(1), ContextLevel::Normal);
        assert_eq!(ContextLevel::from(2), ContextLevel::Normal);
        assert_eq!(ContextLevel::from(255), ContextLevel::Normal);
    }
}
