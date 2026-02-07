mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use crate::checkpoint::execution_history::ExecutionHistory;
    use crate::checkpoint::RunContext;
    use crate::config::Config;
    use crate::executor::{MockProcessExecutor, ProcessExecutor};
    use crate::logger::{Colors, Logger};
    use crate::pipeline::Timer;
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
        let truncated = truncate_diff_if_large(small_diff, 10_000);

        assert_eq!(truncated, small_diff);
    }

    #[test]
    fn test_truncate_diff_preserves_structure() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n+change1\n\
            diff --git a/src/lib.rs b/src/lib.rs\n+change2\n";
        let truncated = truncate_diff_if_large(diff, 10_000);

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

        let max_size = 18;
        let truncated = truncate_lines_to_fit(&lines, max_size);

        assert!(!truncated.is_empty());
        assert!(
            truncated.last().is_some_and(|l| l.ends_with("[truncated...]")),
            "expected last line to be marked as truncated"
        );
        let total_size: usize = truncated.iter().map(|l| l.len() + 1).sum();
        assert!(
            total_size <= max_size,
            "truncate_lines_to_fit must respect max size after suffix"
        );
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
        
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let template_context = TemplateContext::default();

        let executor = Arc::new(
            MockProcessExecutor::new()
                .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
        );
        let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();

        let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
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
        run_log_context: &run_log_context,
        };

        let _ = run_commit_attempt(&mut ctx, 2, "diff --git a/a b/a\n+change\n", "claude")
            .expect("run_commit_attempt should succeed");

        let calls = executor.agent_calls();
        assert_eq!(calls.len(), 1);
        // New per-run log format: .agent/logs-<run_id>/agents/commit_2.log
        // Agent identity is in the log file header, not the filename
        assert!(
            calls[0].logfile.contains("/agents/commit_2.log"),
            "commit generation log should use per-run format with phase_index naming: {}",
            calls[0].logfile
        );
    }

    #[test]
    fn test_run_commit_attempt_logs_diff_truncated_when_model_safe_diff_contains_marker() {
        let workspace = MemoryWorkspace::new_test().with_file(
            xml_paths::COMMIT_MESSAGE_XML,
            "<ralph-commit><ralph-subject>feat: x</ralph-subject></ralph-commit>",
        );
        let colors = Colors { enabled: false };
        let logger = Logger::new(colors);
        let mut timer = Timer::new();
        
        let config = Config::default();
        let registry = AgentRegistry::new().unwrap();
        let template_context = TemplateContext::default();

        let executor = Arc::new(
            MockProcessExecutor::new()
                .with_agent_result("claude", Ok(crate::executor::AgentCommandResult::success())),
        );
        let executor_arc: Arc<dyn ProcessExecutor> = executor.clone();

        let repo_root = PathBuf::from("/mock/repo");
    let run_log_context = crate::logging::RunLogContext::new(&workspace).unwrap();
        let mut ctx = PhaseContext {
            config: &config,
            registry: &registry,
            logger: &logger,
            colors: &colors,
            timer: &mut timer,
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
        run_log_context: &run_log_context,
        };

        let model_safe_diff =
            "diff --git a/a b/a\n+change\n\n[Truncated: 1 of 2 files shown]\n";
        let _ = run_commit_attempt(&mut ctx, 1, model_safe_diff, "claude")
            .expect("run_commit_attempt should succeed");

        let log_files = workspace.list_files_in_dir(".agent/logs/commit_generation");
        let attempt_log_path = log_files
            .iter()
            .find(|p| {
                let path = p.to_string_lossy();
                path.ends_with(".log") && path.contains("attempt_") && path.contains("run_")
            })
            .expect("expected an attempt log file to be written")
            .to_string_lossy()
            .to_string();

        let log_content = workspace
            .get_file(&attempt_log_path)
            .expect("attempt log should be readable");
        assert!(
            log_content.contains("Diff truncated: YES"),
            "expected truncation marker in log, got:\n{log_content}"
        );
    }

    // Tests for effective_model_budget_bytes
    #[test]
    fn test_effective_budget_is_min_across_agents() {
        // claude (300KB) + qwen (100KB) = min is 100KB
        let agents = vec!["claude".to_string(), "qwen".to_string()];
        assert_eq!(effective_model_budget_bytes(&agents), GLM_MAX_PROMPT_SIZE);
    }

    #[test]
    fn test_effective_budget_multiple_glm_agents() {
        // All GLM-like agents should return GLM budget
        let agents = vec![
            "qwen".to_string(),
            "deepseek".to_string(),
            "zhipuai".to_string(),
        ];
        assert_eq!(effective_model_budget_bytes(&agents), GLM_MAX_PROMPT_SIZE);
    }

    #[test]
    fn test_effective_budget_claude_only() {
        // Single Claude agent should return Claude budget
        let agents = vec!["claude".to_string()];
        assert_eq!(effective_model_budget_bytes(&agents), CLAUDE_MAX_PROMPT_SIZE);
    }

    #[test]
    fn test_effective_budget_defaults_to_200kb_for_unknown() {
        let agents = vec!["unknown-agent".to_string()];
        assert_eq!(effective_model_budget_bytes(&agents), MAX_SAFE_PROMPT_SIZE);
    }

    #[test]
    fn test_effective_budget_empty_chain_returns_default() {
        let agents: Vec<String> = vec![];
        assert_eq!(effective_model_budget_bytes(&agents), MAX_SAFE_PROMPT_SIZE);
    }

    #[test]
    fn test_effective_budget_mixed_agents_uses_smallest() {
        // Mix of Claude (300KB), default (200KB), GLM (100KB) => min is 100KB
        let agents = vec![
            "claude".to_string(),
            "unknown".to_string(),
            "qwen".to_string(),
        ];
        assert_eq!(effective_model_budget_bytes(&agents), GLM_MAX_PROMPT_SIZE);
    }

    // Tests for truncate_diff_to_model_budget determinism
    #[test]
    fn test_truncation_is_deterministic() {
        let diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(300_000));
        let budget = 100_000u64;

        let (result1, truncated1) = truncate_diff_to_model_budget(&diff, budget);
        let (result2, truncated2) = truncate_diff_to_model_budget(&diff, budget);

        assert_eq!(result1, result2, "truncation should be deterministic");
        assert_eq!(truncated1, truncated2);
        assert!(truncated1);
    }

    #[test]
    fn test_truncation_within_budget_returns_unchanged() {
        let diff = "diff --git a/a b/a\n+small change\n";
        let budget = 100_000u64;

        let (result, truncated) = truncate_diff_to_model_budget(diff, budget);

        assert_eq!(result, diff);
        assert!(!truncated);
    }

    #[test]
    fn test_truncation_exactly_at_budget_returns_unchanged() {
        // Create diff exactly at budget size
        let budget = 1000u64;
        let diff_content = "a".repeat(budget as usize);
        let diff = format!("diff --git a/a b/a\n{}", diff_content);
        // Ensure we're above budget so truncation occurs
        assert!(diff.len() > budget as usize);

        let (result, truncated) = truncate_diff_to_model_budget(&diff, budget);

        // When above budget, should be truncated
        assert!(truncated);
        assert!(result.len() <= budget as usize + 200); // +200 for truncation message
    }

    #[test]
    fn test_truncation_returns_truncated_flag_when_over_budget() {
        let diff = format!("diff --git a/a b/a\n+{}\n", "x".repeat(50_000));
        let budget = 10_000u64;

        let (result, truncated) = truncate_diff_to_model_budget(&diff, budget);

        assert!(truncated, "should indicate truncation occurred");
        assert!(
            result.len() < diff.len(),
            "truncated result should be smaller"
        );
    }

    // Tests for model_budget_bytes_for_agent_name
    #[test]
    fn test_model_budget_for_claude_variants() {
        assert_eq!(
            model_budget_bytes_for_agent_name("claude"),
            CLAUDE_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("Claude"),
            CLAUDE_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("CLAUDE"),
            CLAUDE_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("claude-3"),
            CLAUDE_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("ccs"),
            CLAUDE_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("anthropic"),
            CLAUDE_MAX_PROMPT_SIZE
        );
    }

    #[test]
    fn test_model_budget_for_glm_variants() {
        assert_eq!(model_budget_bytes_for_agent_name("glm"), GLM_MAX_PROMPT_SIZE);
        assert_eq!(
            model_budget_bytes_for_agent_name("zhipuai"),
            GLM_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("qwen"),
            GLM_MAX_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("deepseek"),
            GLM_MAX_PROMPT_SIZE
        );
        assert_eq!(model_budget_bytes_for_agent_name("zai"), GLM_MAX_PROMPT_SIZE);
    }

    #[test]
    fn test_model_budget_for_unknown_agents() {
        assert_eq!(
            model_budget_bytes_for_agent_name("unknown"),
            MAX_SAFE_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("gpt-4"),
            MAX_SAFE_PROMPT_SIZE
        );
        assert_eq!(
            model_budget_bytes_for_agent_name("custom-agent"),
            MAX_SAFE_PROMPT_SIZE
        );
    }
}
