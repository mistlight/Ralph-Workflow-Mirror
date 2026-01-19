//! Recovery strategy for checkpoint state.
//!
//! This module provides the recovery strategy enum for handling
//! checkpoint validation failures.

/// Recovery strategy to use when validation fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Fail fast - require user intervention
    Fail,
    /// Attempt automatic recovery where possible
    Auto,
    /// Warn but continue (not recommended)
    Force,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_strategy_variants() {
        let fail = RecoveryStrategy::Fail;
        let auto = RecoveryStrategy::Auto;
        let force = RecoveryStrategy::Force;

        assert_eq!(fail, RecoveryStrategy::Fail);
        assert_eq!(auto, RecoveryStrategy::Auto);
        assert_eq!(force, RecoveryStrategy::Force);
    }
}
