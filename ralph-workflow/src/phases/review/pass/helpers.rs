use super::super::validation::{post_flight_review_check, PostflightResult};
use crate::phases::context::PhaseContext;

/// Handle post-flight validation after a review pass.
///
/// This function checks that ISSUES.md was properly created and is well-formed
/// after the reviewer agent completes. It logs warnings for any issues found
/// but does not fail the pass.
pub(super) fn handle_postflight_validation(ctx: &PhaseContext<'_>, j: u32) {
    let postflight_result = post_flight_review_check(ctx.workspace, ctx.logger, j);
    match postflight_result {
        PostflightResult::Valid => {
            // ISSUES.md found and valid, continue
        }
        PostflightResult::Missing(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. Proceeding with fix pass anyway."
            ));
        }
        PostflightResult::Malformed(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. The fix pass may not work correctly."
            ));
            ctx.logger.info(&format!(
                "{}Tip:{} Try with generic parser: {}RALPH_REVIEWER_JSON_PARSER=generic ralph{}",
                ctx.colors.bold(),
                ctx.colors.reset(),
                ctx.colors.bold(),
                ctx.colors.reset()
            ));
        }
    }
}

/// Check if stderr contains authentication-related errors.
///
/// This function scans stderr output for common authentication error patterns
/// to help distinguish authentication failures from other agent failures.
///
/// # Arguments
///
/// * `stderr` - The stderr output from the agent invocation
///
/// # Returns
///
/// `true` if authentication errors are detected, `false` otherwise
pub(super) fn stderr_contains_auth_error(stderr: &str) -> bool {
    let combined = stderr.to_lowercase();
    combined.contains("authentication")
        || combined.contains("unauthorized")
        || combined.contains("credential")
        || combined.contains("api key")
        || combined.contains("not authorized")
}
