//! Issue Severity Types
//!
//! Defines severity levels for review issues.

/// Issue severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum IssueSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl IssueSeverity {
    /// Parse severity from a string
    pub(super) fn from_str(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        if lower.contains("critical") {
            Some(IssueSeverity::Critical)
        } else if lower.contains("high") {
            Some(IssueSeverity::High)
        } else if lower.contains("medium") {
            Some(IssueSeverity::Medium)
        } else if lower.contains("low") {
            Some(IssueSeverity::Low)
        } else {
            None
        }
    }
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueSeverity::Critical => write!(f, "Critical"),
            IssueSeverity::High => write!(f, "High"),
            IssueSeverity::Medium => write!(f, "Medium"),
            IssueSeverity::Low => write!(f, "Low"),
        }
    }
}
