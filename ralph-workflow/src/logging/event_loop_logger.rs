use crate::reducer::event::PipelinePhase;
use crate::workspace::Workspace;
use chrono::Utc;
use std::path::Path;

/// Parameters for logging an effect execution.
pub struct LogEffectParams<'a> {
    pub workspace: &'a dyn Workspace,
    pub log_path: &'a Path,
    pub phase: PipelinePhase,
    pub effect: &'a str,
    pub primary_event: &'a str,
    pub extra_events: &'a [String],
    pub duration_ms: u64,
    pub context: &'a [(&'a str, &'a str)],
}

/// Logger for recording event loop execution.
///
/// This logger writes a human-readable log of the event loop's progression:
/// - which effects ran
/// - what events were emitted
/// - how long each effect took
/// - what phase/iteration/retry context was active
///
/// The log is always-on (not just for crashes) and is written to
/// `.agent/logs-<run_id>/event_loop.log` for easy diagnosis.
///
/// **Redaction:** This logger must never include sensitive content like
/// prompts, agent outputs, secrets, or credentials.
pub struct EventLoopLogger {
    seq: u64,
}

impl EventLoopLogger {
    /// Create a new EventLoopLogger.
    ///
    /// The sequence counter starts at 1 for the first logged effect.
    pub fn new() -> Self {
        Self { seq: 1 }
    }

    /// Log an effect execution.
    ///
    /// This writes a single line to the event loop log with the following format:
    /// ```text
    /// <seq> ts=<rfc3339> phase=<Phase> effect=<Effect> event=<Event> [extra=[E1,E2]] [ctx=k1=v1,k2=v2] ms=<N>
    /// ```
    ///
    /// Example:
    /// ```text
    /// 1 ts=2026-02-06T14:03:27.123Z phase=Development effect=InvokePrompt event=PromptCompleted ms=1234
    /// 2 ts=2026-02-06T14:03:28.456Z phase=Development effect=WriteFile event=FileWritten ctx=file=PLAN.md ms=12
    /// ```
    pub fn log_effect(&mut self, params: LogEffectParams) {
        let ts = Utc::now().to_rfc3339();

        // Format extra events (if any)
        let extra = if params.extra_events.is_empty() {
            String::new()
        } else {
            format!(" extra=[{}]", params.extra_events.join(","))
        };

        // Format context (if any)
        let ctx = if params.context.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = params
                .context
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!(" ctx={}", pairs.join(","))
        };

        let line = format!(
            "{} ts={} phase={:?} effect={} event={}{}{} ms={}\n",
            self.seq,
            ts,
            params.phase,
            params.effect,
            params.primary_event,
            extra,
            ctx,
            params.duration_ms
        );

        // Best-effort append (failures are silently ignored)
        let _ = params
            .workspace
            .append_bytes(params.log_path, line.as_bytes());

        self.seq += 1;
    }
}

impl Default for EventLoopLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceFs;

    #[test]
    fn test_event_loop_logger_basic() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let log_path = std::path::Path::new("event_loop.log");
        let mut logger = EventLoopLogger::new();

        // Log a few effects
        logger.log_effect(LogEffectParams {
            workspace: &workspace,
            log_path,
            phase: PipelinePhase::Development,
            effect: "InvokePrompt",
            primary_event: "PromptCompleted",
            extra_events: &[],
            duration_ms: 1234,
            context: &[("iteration", "1")],
        });

        logger.log_effect(LogEffectParams {
            workspace: &workspace,
            log_path,
            phase: PipelinePhase::Development,
            effect: "WriteFile",
            primary_event: "FileWritten",
            extra_events: &["CheckpointSaved".to_string()],
            duration_ms: 12,
            context: &[],
        });

        // Verify log file exists
        assert!(workspace.exists(log_path));

        // Verify content
        let content = workspace.read(log_path).unwrap();
        assert!(content.contains("1 ts="));
        assert!(content.contains("phase=Development"));
        assert!(content.contains("effect=InvokePrompt"));
        assert!(content.contains("event=PromptCompleted"));
        assert!(content.contains("ms=1234"));
        assert!(content.contains("ctx=iteration=1"));

        assert!(content.contains("2 ts="));
        assert!(content.contains("effect=WriteFile"));
        assert!(content.contains("event=FileWritten"));
        assert!(content.contains("extra=[CheckpointSaved]"));
        assert!(content.contains("ms=12"));
    }

    #[test]
    fn test_event_loop_logger_sequence_increment() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let log_path = std::path::Path::new("event_loop.log");
        let mut logger = EventLoopLogger::new();

        // Log several effects
        for i in 0..5 {
            logger.log_effect(LogEffectParams {
                workspace: &workspace,
                log_path,
                phase: PipelinePhase::Planning,
                effect: "TestEffect",
                primary_event: "TestEvent",
                extra_events: &[],
                duration_ms: 10 * i,
                context: &[],
            });
        }

        // Verify sequence numbers
        let content = workspace.read(log_path).unwrap();
        for i in 1..=5 {
            assert!(
                content.contains(&format!("{} ts=", i)),
                "Should contain sequence number {}",
                i
            );
        }
    }

    #[test]
    fn test_event_loop_logger_context_formatting() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let log_path = std::path::Path::new("event_loop.log");
        let mut logger = EventLoopLogger::new();

        logger.log_effect(LogEffectParams {
            workspace: &workspace,
            log_path,
            phase: PipelinePhase::Review,
            effect: "InvokeReviewer",
            primary_event: "ReviewCompleted",
            extra_events: &[],
            duration_ms: 5000,
            context: &[
                ("reviewer_pass", "2"),
                ("agent_index", "3"),
                ("retry_cycle", "1"),
            ],
        });

        let content = workspace.read(log_path).unwrap();
        assert!(content.contains("ctx=reviewer_pass=2,agent_index=3,retry_cycle=1"));
    }

    #[test]
    fn test_event_loop_logger_empty_context() {
        let tempdir = tempfile::tempdir().unwrap();
        let workspace = WorkspaceFs::new(tempdir.path().to_path_buf());

        let log_path = std::path::Path::new("event_loop.log");
        let mut logger = EventLoopLogger::new();

        logger.log_effect(LogEffectParams {
            workspace: &workspace,
            log_path,
            phase: PipelinePhase::CommitMessage,
            effect: "GenerateCommit",
            primary_event: "CommitGenerated",
            extra_events: &[],
            duration_ms: 100,
            context: &[],
        });

        let content = workspace.read(log_path).unwrap();
        // Should not contain "ctx=" when context is empty
        assert!(!content.contains("ctx="));
        // Should not contain "extra=" when no extra events
        assert!(!content.contains("extra="));
    }
}
