/// Result of running a review pass.
#[derive(Debug)]
pub struct ReviewPassResult {
    /// Whether the review found no issues and should exit early.
    pub early_exit: bool,
    /// Whether an authentication/credential error was detected.
    /// When true, the caller should trigger agent fallback instead of retrying.
    pub auth_failure: bool,
    /// Whether the agent failed to run successfully.
    pub agent_failed: bool,
    /// Whether the review output was validated successfully.
    pub output_valid: bool,
    /// Whether issues were found in the validated output.
    pub issues_found: bool,
    /// Raw XML content for UI rendering (if available).
    pub xml_content: Option<String>,
}

/// Result of running a fix pass.
#[derive(Debug)]
pub struct FixPassResult {
    /// Whether an authentication/credential error was detected.
    pub auth_failure: bool,
    /// Whether the agent failed to run successfully.
    pub agent_failed: bool,
    /// Whether the fix output was validated successfully.
    pub output_valid: bool,
    /// Whether changes were made according to the fix output.
    pub changes_made: bool,
    /// Parsed fix status from `<ralph-status>` (when output is valid).
    pub status: Option<String>,
    /// Optional summary from `<ralph-summary>` (when output is valid).
    pub summary: Option<String>,
    /// Raw XML content for UI rendering (if available).
    pub xml_content: Option<String>,
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
