// Tests for state module.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::state::{AgentConfigSnapshot, CheckpointParams, CliArgsSnapshot};

    include!("tests/core_state.rs");
    include!("tests/continuation_state.rs");
    include!("tests/xsd_retry_and_session.rs");
    include!("tests/fix_status.rs");
}
