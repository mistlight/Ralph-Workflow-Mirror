use crate::reducer::state::{ContinuationState, SameAgentRetryReason};

pub(crate) fn same_agent_retry_preamble(continuation: &ContinuationState) -> String {
    let attempt = continuation.same_agent_retry_count;
    let reason = continuation.same_agent_retry_reason.clone();

    let reason_line = match reason {
        Some(SameAgentRetryReason::Timeout) => "Previous attempt timed out.",
        Some(SameAgentRetryReason::InternalError) => {
            "Previous attempt failed with an internal/unknown error."
        }
        Some(SameAgentRetryReason::Other) => {
            "Previous attempt failed with a non-retriable error (non-auth, non-rate-limit)."
        }
        None => "Retrying after a transient invocation failure.",
    };

    format!(
        "## Retry Note (attempt {attempt})\n\
{reason_line}\n\
\n\
Please retry with these constraints:\n\
- Reduce scope; do the smallest safe change.\n\
- Break work into small, verifiable steps; avoid long-running commands.\n\
- Prefer targeted tests and quick checks; only broaden if needed.\n\
- If output is large, summarize and write artifacts to the required files.\n\
- Always produce valid XML output that matches the schema.\n"
    )
}
