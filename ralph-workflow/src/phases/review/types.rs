/// Classification of review pass outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPassStatus {
    /// Agent invocation failed before output validation.
    AgentFailed { auth_failure: bool },
    /// Agent invocation succeeded but output XML was invalid.
    OutputInvalid,
    /// Output was valid and contained issues.
    ValidatedIssuesFound,
    /// Output was valid and explicitly reported no issues.
    ValidatedNoIssues,
}

/// Result of running a review pass.
#[derive(Debug)]
pub struct ReviewPassResult {
    /// Aggregated review pass status.
    pub status: ReviewPassStatus,
    /// Raw XML content for UI rendering (if available).
    pub xml_content: Option<String>,
}

impl ReviewPassResult {
    #[must_use]
    pub const fn agent_failed(auth_failure: bool) -> Self {
        Self {
            status: ReviewPassStatus::AgentFailed { auth_failure },
            xml_content: None,
        }
    }

    #[must_use]
    pub const fn issues_found(xml_content: String) -> Self {
        Self {
            status: ReviewPassStatus::ValidatedIssuesFound,
            xml_content: Some(xml_content),
        }
    }

    #[must_use]
    pub const fn no_issues(xml_content: String) -> Self {
        Self {
            status: ReviewPassStatus::ValidatedNoIssues,
            xml_content: Some(xml_content),
        }
    }

    #[must_use]
    pub const fn output_invalid() -> Self {
        Self {
            status: ReviewPassStatus::OutputInvalid,
            xml_content: None,
        }
    }

    #[must_use]
    pub const fn is_early_exit(&self) -> bool {
        matches!(self.status, ReviewPassStatus::ValidatedNoIssues)
    }

    #[must_use]
    pub const fn has_auth_failure(&self) -> bool {
        matches!(
            self.status,
            ReviewPassStatus::AgentFailed { auth_failure: true }
        )
    }

    #[must_use]
    pub const fn is_agent_failed(&self) -> bool {
        matches!(self.status, ReviewPassStatus::AgentFailed { .. })
    }

    #[must_use]
    pub const fn is_output_valid(&self) -> bool {
        matches!(
            self.status,
            ReviewPassStatus::ValidatedIssuesFound | ReviewPassStatus::ValidatedNoIssues
        )
    }

    #[must_use]
    pub const fn has_issues(&self) -> bool {
        matches!(self.status, ReviewPassStatus::ValidatedIssuesFound)
    }
}

/// Classification of fix pass outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixPassResultStatus {
    /// Agent invocation failed before output validation.
    AgentFailed { auth_failure: bool },
    /// Agent invocation succeeded but output XML was missing or invalid.
    OutputInvalid,
    /// Output was valid.
    Validated { changes_made: bool },
}

/// Result of running a fix pass.
#[derive(Debug)]
pub struct FixPassResult {
    /// Aggregated fix pass status.
    pub result_status: FixPassResultStatus,
    /// Parsed fix status from `<ralph-status>` (when output is valid).
    pub status: Option<String>,
    /// Optional summary from `<ralph-summary>` (when output is valid).
    pub summary: Option<String>,
    /// Raw XML content for UI rendering (if available).
    pub xml_content: Option<String>,
}

impl FixPassResult {
    #[must_use]
    pub const fn agent_failed(auth_failure: bool) -> Self {
        Self {
            result_status: FixPassResultStatus::AgentFailed { auth_failure },
            status: None,
            summary: None,
            xml_content: None,
        }
    }

    #[must_use]
    pub const fn validated(
        changes_made: bool,
        status: String,
        summary: Option<String>,
        xml_content: String,
    ) -> Self {
        Self {
            result_status: FixPassResultStatus::Validated { changes_made },
            status: Some(status),
            summary,
            xml_content: Some(xml_content),
        }
    }

    #[must_use]
    pub const fn output_invalid(xml_content: Option<String>) -> Self {
        Self {
            result_status: FixPassResultStatus::OutputInvalid,
            status: None,
            summary: None,
            xml_content,
        }
    }

    #[must_use]
    pub const fn has_auth_failure(&self) -> bool {
        matches!(
            self.result_status,
            FixPassResultStatus::AgentFailed { auth_failure: true }
        )
    }

    #[must_use]
    pub const fn is_agent_failed(&self) -> bool {
        matches!(self.result_status, FixPassResultStatus::AgentFailed { .. })
    }

    #[must_use]
    pub const fn is_output_valid(&self) -> bool {
        matches!(self.result_status, FixPassResultStatus::Validated { .. })
    }

    #[must_use]
    pub const fn has_changes(&self) -> bool {
        matches!(
            self.result_status,
            FixPassResultStatus::Validated { changes_made: true }
        )
    }
}

/// Result of parsing review output.
#[derive(Debug)]
pub(super) enum ParseResult {
    /// Successfully parsed with issues found.
    IssuesFound {
        issues: Vec<String>,
        xml_content: String,
    },
    /// Successfully parsed with explicit "no issues" declaration.
    NoIssuesExplicit { xml_content: String },
    /// Failed to parse - includes error description for re-prompting.
    ParseFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_result_constructors_capture_expected_flags() {
        let issues = ReviewPassResult::issues_found("<issues/>".to_string());
        assert!(!issues.is_early_exit());
        assert!(!issues.has_auth_failure());
        assert!(!issues.is_agent_failed());
        assert!(issues.is_output_valid());
        assert!(issues.has_issues());

        let no_issues = ReviewPassResult::no_issues("<none/>".to_string());
        assert!(no_issues.is_early_exit());
        assert!(!no_issues.has_issues());
    }

    #[test]
    fn fix_result_constructors_capture_expected_flags() {
        let failed = FixPassResult::agent_failed(true);
        assert!(failed.has_auth_failure());
        assert!(failed.is_agent_failed());
        assert!(!failed.is_output_valid());
        assert!(!failed.has_changes());

        let validated = FixPassResult::validated(
            true,
            "applied".to_string(),
            Some("done".to_string()),
            "<fix/>".to_string(),
        );
        assert!(!validated.is_agent_failed());
        assert!(validated.is_output_valid());
        assert!(validated.has_changes());
    }
}
