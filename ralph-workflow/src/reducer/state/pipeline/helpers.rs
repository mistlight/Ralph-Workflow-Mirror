// Pure helper methods for PipelineState.
//
// These methods provide state queries and derived values. They contain
// no side effects and operate solely on the immutable state struct.

impl PipelineState {
    /// Returns true if the pipeline is in a terminal state for event loop purposes.
    ///
    /// # Terminal States
    ///
    /// - **Complete phase**: Always terminal (successful completion)
    /// - **Interrupted phase**: Terminal under these conditions:
    ///   1. A checkpoint has been saved (normal Ctrl+C interruption path)
    ///   2. Transitioning from AwaitingDevFix phase (failure handling completed)
    ///
    /// # AwaitingDevFix → Interrupted Path
    ///
    /// When the pipeline encounters a terminal failure (e.g., AgentChainExhausted),
    /// it transitions through AwaitingDevFix phase where:
    /// 1. TriggerDevFixFlow effect writes completion marker to filesystem
    /// 2. Dev-fix agent is dispatched (optional remediation attempt)
    /// 3. CompletionMarkerEmitted event transitions to Interrupted phase
    ///
    /// At this point, the completion marker has been written, signaling external
    /// orchestration that the pipeline has terminated. The SaveCheckpoint effect
    /// will execute next, but the phase is already considered terminal because
    /// the failure has been properly signaled.
    ///
    /// # Edge Cases
    ///
    /// An Interrupted phase without a checkpoint and without previous_phase context
    /// is NOT considered terminal. This can occur when resuming from a checkpoint
    /// that was interrupted mid-execution.
    ///
    /// # Non-Terminating Pipeline Architecture
    ///
    /// The pipeline is designed to never exit early. All failure paths route through
    /// `AwaitingDevFix` → `TriggerDevFixFlow` → completion marker write → `Interrupted`.
    /// This ensures orchestration can reliably detect completion via the marker file,
    /// even when budget is exhausted or all agents fail.
    ///
    /// Terminal states:
    /// - `Complete`: Normal successful completion
    /// - `Interrupted` with checkpoint saved: Resumable state
    /// - `Interrupted` from `AwaitingDevFix`: Completion marker written, failure signaled
    pub fn is_complete(&self) -> bool {
        matches!(self.phase, PipelinePhase::Complete)
            || (matches!(self.phase, PipelinePhase::Interrupted)
                && (self.checkpoint_saved_count > 0
                    // CRITICAL: AwaitingDevFix→Interrupted transition means completion marker
                    // was written during TriggerDevFixFlow. This is terminal even without
                    // checkpoint because the failure has been properly signaled to orchestration.
                    // This prevents "Pipeline exited without completion marker" bug.
                    || matches!(self.previous_phase, Some(PipelinePhase::AwaitingDevFix))))
    }

    pub fn current_head(&self) -> String {
        self.rebase
            .current_head()
            .unwrap_or_else(|| "HEAD".to_string())
    }
}
