//! Tests for logfile module.

use super::*;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[test]
fn test_sanitize_agent_name() {
    assert_eq!(sanitize_agent_name("claude"), "claude");
    assert_eq!(sanitize_agent_name("ccs/glm"), "ccs-glm");
    assert_eq!(
        sanitize_agent_name("opencode/anthropic/claude-sonnet-4"),
        "opencode-anthropic-claude-sonnet-4"
    );
}

#[test]
fn test_build_logfile_path() {
    assert_eq!(
        build_logfile_path(".agent/logs/planning_1", "claude", 0),
        ".agent/logs/planning_1_claude_0.log"
    );
    assert_eq!(
        build_logfile_path(".agent/logs/planning_1", "ccs/glm", 0),
        ".agent/logs/planning_1_ccs-glm_0.log"
    );
    assert_eq!(
        build_logfile_path(".agent/logs/dev_2", "opencode/anthropic/claude-sonnet-4", 1),
        ".agent/logs/dev_2_opencode-anthropic-claude-sonnet-4_1.log"
    );
}

#[test]
fn test_build_logfile_path_with_attempt() {
    assert_eq!(
        build_logfile_path_with_attempt(".agent/logs/planning_1", "claude", 0, 0),
        ".agent/logs/planning_1_claude_0_a0.log"
    );
    assert_eq!(
        build_logfile_path_with_attempt(".agent/logs/planning_1", "ccs/glm", 1, 2),
        ".agent/logs/planning_1_ccs-glm_1_a2.log"
    );
    assert_eq!(
        build_logfile_path_with_attempt(
            ".agent/logs/dev_2",
            "opencode/anthropic/claude-sonnet-4",
            0,
            5
        ),
        ".agent/logs/dev_2_opencode-anthropic-claude-sonnet-4_0_a5.log"
    );
}

#[test]
fn test_next_logfile_attempt_index_returns_zero_when_no_matches() {
    let workspace = MemoryWorkspace::new_test();
    let prefix = Path::new(".agent/logs/planning_1");
    assert_eq!(
        next_logfile_attempt_index(prefix, "claude", 0, &workspace),
        0
    );
}

#[test]
fn test_next_logfile_attempt_index_increments_from_existing_attempts() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/logs/planning_1_claude_0_a0.log", "")
        .with_file(".agent/logs/planning_1_claude_0_a2.log", "")
        .with_file(".agent/logs/planning_1_claude_0_a10.log", "")
        // Different agent/model should be ignored
        .with_file(".agent/logs/planning_1_other_0_a99.log", "")
        .with_file(".agent/logs/planning_1_claude_1_a7.log", "");

    let prefix = Path::new(".agent/logs/planning_1");
    assert_eq!(
        next_logfile_attempt_index(prefix, "claude", 0, &workspace),
        11
    );
}

#[test]
fn test_extract_agent_name_with_model_index() {
    let log_file = Path::new(".agent/logs/planning_1_ccs-glm_0.log");
    let prefix = Path::new(".agent/logs/planning_1");
    assert_eq!(
        extract_agent_name_from_logfile(log_file, prefix),
        Some("ccs-glm".to_string())
    );
}

#[test]
fn test_extract_agent_name_opencode_style() {
    let log_file = Path::new(".agent/logs/dev_1_opencode-anthropic-claude-sonnet-4_0.log");
    let prefix = Path::new(".agent/logs/dev_1");
    assert_eq!(
        extract_agent_name_from_logfile(log_file, prefix),
        Some("opencode-anthropic-claude-sonnet-4".to_string())
    );
}

#[test]
fn test_extract_agent_name_with_attempt_suffix() {
    let log_file = Path::new(".agent/logs/planning_1_ccs-glm_0_a2.log");
    let prefix = Path::new(".agent/logs/planning_1");
    assert_eq!(
        extract_agent_name_from_logfile(log_file, prefix),
        Some("ccs-glm".to_string())
    );
}

#[test]
fn test_extract_agent_name_opencode_style_with_attempt_suffix() {
    let log_file = Path::new(".agent/logs/dev_1_opencode-anthropic-claude-sonnet-4_0_a5.log");
    let prefix = Path::new(".agent/logs/dev_1");
    assert_eq!(
        extract_agent_name_from_logfile(log_file, prefix),
        Some("opencode-anthropic-claude-sonnet-4".to_string())
    );
}

#[test]
fn test_extract_agent_name_does_not_strip_attempt_suffix_when_no_model_index() {
    // If a logfile uses the agent-only form (no model index) and the agent name
    // itself ends with "_a<digits>", we must NOT strip that suffix.
    let log_file = Path::new(".agent/logs/planning_1_agent_a123.log");
    let prefix = Path::new(".agent/logs/planning_1");
    assert_eq!(
        extract_agent_name_from_logfile(log_file, prefix),
        Some("agent_a123".to_string())
    );
}

#[test]
fn test_extract_agent_name_wrong_prefix() {
    let log_file = Path::new(".agent/logs/review_1_claude_0.log");
    let prefix = Path::new(".agent/logs/planning_1");
    assert_eq!(extract_agent_name_from_logfile(log_file, prefix), None);
}

#[test]
fn test_find_most_recent_logfile() {
    // Create workspace with two log files with different modification times
    let time1 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let time2 = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);

    let workspace = MemoryWorkspace::new_test()
        .with_file_at_time(".agent/logs/test_1_agent-a_0.log", "old", time1)
        .with_file_at_time(".agent/logs/test_1_agent-b_0.log", "new", time2);

    let prefix = Path::new(".agent/logs/test_1");
    let result = find_most_recent_logfile(prefix, &workspace);
    assert_eq!(
        result,
        Some(PathBuf::from(".agent/logs/test_1_agent-b_0.log"))
    );
}

#[test]
fn test_find_most_recent_logfile_no_match() {
    let workspace =
        MemoryWorkspace::new_test().with_file(".agent/logs/other_1_claude_0.log", "content");

    let prefix = Path::new(".agent/logs/test_1");
    let result = find_most_recent_logfile(prefix, &workspace);
    assert_eq!(result, None);
}

#[test]
fn test_read_most_recent_logfile() {
    let time1 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let time2 = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);

    let workspace = MemoryWorkspace::new_test()
        .with_file_at_time(".agent/logs/test_1_agent-a_0.log", "old content", time1)
        .with_file_at_time(".agent/logs/test_1_agent-b_0.log", "new content", time2);

    let prefix = Path::new(".agent/logs/test_1");
    let result = read_most_recent_logfile(prefix, &workspace);
    assert_eq!(result, "new content");
}

#[test]
fn test_read_most_recent_logfile_empty_when_not_found() {
    let workspace = MemoryWorkspace::new_test();

    let prefix = Path::new(".agent/logs/nonexistent");
    let result = read_most_recent_logfile(prefix, &workspace);
    assert_eq!(result, "");
}

#[test]
fn test_next_simplified_logfile_attempt_index_returns_zero_when_no_matches() {
    let workspace = MemoryWorkspace::new_test();
    // Create the run directory structure
    workspace
        .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
        .unwrap();

    let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
    assert_eq!(
        next_simplified_logfile_attempt_index(base_path, &workspace),
        0
    );
}

#[test]
fn test_next_simplified_logfile_attempt_index_increments_from_existing_attempts() {
    let workspace = MemoryWorkspace::new_test();
    // Create the run directory structure
    workspace
        .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
        .unwrap();

    // Pre-populate some log files with attempt suffixes
    let base = ".agent/logs-2026-02-06_14-03-27.123Z/agents";
    workspace
        .write(&PathBuf::from(format!("{base}/planning_1_a0.log")), "first")
        .unwrap();
    workspace
        .write(&PathBuf::from(format!("{base}/planning_1_a2.log")), "third")
        .unwrap();
    workspace
        .write(&PathBuf::from(format!("{base}/planning_1_a10.log")), "11th")
        .unwrap();
    // Different phase should be ignored
    workspace
        .write(
            &PathBuf::from(format!("{base}/developer_1_a5.log")),
            "other",
        )
        .unwrap();

    let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
    assert_eq!(
        next_simplified_logfile_attempt_index(base_path, &workspace),
        11
    );
}

#[test]
fn test_next_simplified_logfile_attempt_index_returns_one_when_base_file_exists() {
    let workspace = MemoryWorkspace::new_test();
    // Create the run directory structure
    workspace
        .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
        .unwrap();

    // Create only the base file (without attempt suffix)
    let base = ".agent/logs-2026-02-06_14-03-27.123Z/agents";
    workspace
        .write(&PathBuf::from(format!("{base}/planning_1.log")), "base")
        .unwrap();

    let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
    // Should return 1 (first retry) since base file exists
    assert_eq!(
        next_simplified_logfile_attempt_index(base_path, &workspace),
        1
    );
}

#[test]
fn test_next_simplified_logfile_attempt_index_returns_next_after_base_and_attempts() {
    let workspace = MemoryWorkspace::new_test();
    // Create the run directory structure
    workspace
        .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
        .unwrap();

    // Create the base file (without attempt suffix) and some attempt files
    let base = ".agent/logs-2026-02-06_14-03-27.123Z/agents";
    workspace
        .write(&PathBuf::from(format!("{base}/planning_1.log")), "base")
        .unwrap();
    workspace
        .write(
            &PathBuf::from(format!("{base}/planning_1_a1.log")),
            "first retry",
        )
        .unwrap();
    workspace
        .write(
            &PathBuf::from(format!("{base}/planning_1_a2.log")),
            "second retry",
        )
        .unwrap();

    let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
    // Should return 3 (max existing attempt + 1)
    assert_eq!(
        next_simplified_logfile_attempt_index(base_path, &workspace),
        3
    );
}
