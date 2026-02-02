mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use crate::checkpoint::execution_history::ExecutionHistory;
    use crate::checkpoint::RunContext;
    use crate::config::Config;
    use crate::executor::{MockProcessExecutor, ProcessExecutor};
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::workspace::MemoryWorkspace;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn test_truncate_diff_if_large() {
        let large_diff = "diff --git a/src/main.rs b/src/main.rs\n".repeat(1000);
        let truncated = truncate_diff_if_large(&large_diff, 10_000);

        assert!(truncated.len() <= 10_000 + 200);
        assert!(truncated.contains("[Truncated:"));
    }

    #[test]
    fn test_truncate_diff_no_truncation_needed() {
        let small_diff = "diff --git a/src/main.rs b/src/main.rs\n+change\n";
        let truncated = truncate_diff_if_large(&small_diff, 10_000);

        assert_eq!(truncated, small_diff);
    }

    #[test]
    fn test_truncate_diff_preserves_structure() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+change1\n\
            diff --git a/src/lib.rs b/src/lib.rs\n+change2\n";
        let truncated = truncate_diff_if_large(&diff, 10_000);

        assert!(truncated.contains("diff --git a/src/main.rs"));
        assert!(truncated.contains("diff --git a/src/lib.rs"));
    }

    #[test]
    fn test_truncate_diff_very_small_limit() {
        let large_diff = "diff --git a/src/main.rs b/src/main.rs\n".repeat(100);
        let truncated = truncate_diff_if_large(&large_diff, 50);

        assert!(truncated.len() <= 100);
        assert!(truncated.contains("diff --git"));
    }

    #[test]
    fn test_truncate_keeps_high_priority_files() {
        let diff = "diff --git a/README.md b/README.md\n\
            +doc change\n\
            diff --git a/src/main.rs b/src/main.rs\n\
            +important change\n\
            diff --git a/tests/test.rs b/tests/test.rs\n\
            +test change\n";

        let truncated = truncate_diff_if_large(diff, 80);
        assert!(truncated.contains("src/main.rs"));
    }

    #[test]
    fn test_truncate_lines_to_fit() {
        let lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
            "line4".to_string(),
        ];

        let truncated = truncate_lines_to_fit(&lines, 18);

        assert_eq!(truncated.len(), 3);
        assert!(truncated[2].ends_with("[truncated...]"));
    }

    #[test]
    fn test_extract_commit_message_from_file_reads_primary_xml() {
        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/tmp/commit_message.xml",
            "<ralph-commit><ralph-subject>feat: add</ralph-subject></ralph-commit>",
        );

        let extraction = extract_commit_message_from_file_with_workspace(&workspace);
        let CommitExtractionOutcome::Valid(extracted) = extraction else {
            panic!("expected extraction");
        };
        assert_eq!(extracted.into_message(), "feat: add");
    }

    #[test]
    fn test_extract_commit_message_from_file_ignores_processed_archive() {
        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/tmp/commit_message.xml.processed",
            "<ralph-commit><ralph-subject>feat: add</ralph-subject></ralph-commit>",
        );

        let extraction = extract_commit_message_from_file_with_workspace(&workspace);
        assert!(matches!(
            extraction,
            CommitExtractionOutcome::MissingFile(_)
        ));
    }

    #[test]
    fn test_run_commit_attempt_uses_unique_logfile_with_attempt_suffix() {
        let workspace = MemoryWorkspace::new_test().with_file(
            xml_paths::COMMIT_MESSAGE_XML,
            "<ralph-commit><ralph-subject>feat: x</ralph-subject></ralph-commit>",
        );
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        let mut stats = Stats::default();
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let template_context = TemplateContext::default();

        let executor = Arc::new(
            MockProcessExecutor::new()
                .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
        );
        let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();

        let repo_root = PathBuf::from("/mock/repo");
        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
            stats: &mut stats,
            developer_agent: "claude",
            reviewer_agent: "claude",
            review_guidelines: None,
            template_context: &template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: HashMap::new(),
            executor: executor_arc.as_ref(),
            executor_arc: executor_arc.clone(),
            repo_root: repo_root.as_path(),
            workspace: &workspace,
        };

        let _ = run_commit_attempt(&mut ctx, 2, "diff --git a/a b/a\n+change\n", "claude")
            .expect("run_commit_attempt should succeed");

        let calls = executor.agent_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].logfile, ".agent/logs/commit_generation/commit_generation_claude_0_a2.log",
            "commit generation log should include agent, model index, and attempt suffix"
        );
    }
}
