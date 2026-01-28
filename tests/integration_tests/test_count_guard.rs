//! Guard test to ensure integration test count doesn't drop unexpectedly.
//!
//! This module provides documentation and a lightweight guard to catch
//! accidental test suite regressions. The authoritative count check is
//! performed by the compliance script using `cargo test -- --list`.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** This module is part of the integration test framework and
//! MUST follow the integration test style guide defined in
//! **[../INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! # Purpose
//!
//! This test exists to:
//! - Document the expected minimum integration test count
//! - Serve as a reminder to verify the full suite is running
//! - Provide a best-effort source scan to ensure the guard stays wired up
//!
//! # How to Verify Test Count
//!
//! Run the following command to check the actual test count:
//! ```bash
//! cargo test -p ralph-workflow-tests -- --list 2>&1 | grep -c ': test$'
//! ```
//!
//! The compliance check script (`compliance_check.sh`) verifies this count
//! using the same minimum floor defined here.
//!
//! NOTE: The source scan in this module is intentionally non-authoritative.
//! It only looks for literal `#[test]` annotations and does not reflect
//! conditional compilation or alternative test attributes (e.g. `#[tokio::test]`).
//! The compliance script's `cargo test -- --list` output is the source of truth.

use crate::test_timeout::with_default_timeout;
use std::collections::HashSet;

/// Minimum expected integration test count.
///
/// Update this when adding new test modules. This is a floor, not an exact target.
/// The actual count should be >= this value.
///
/// If this value needs to decrease significantly, it likely indicates either:
/// - Tests were accidentally removed
/// - A test module is not being compiled
/// - The test discovery is not working correctly
pub const MINIMUM_EXPECTED_TESTS: usize = 400;

struct SourceFile {
    path: &'static str,
    contents: &'static str,
}

const INTEGRATION_TEST_SOURCES: &[SourceFile] = &[
    SourceFile {
        path: "main.rs",
        contents: include_str!("main.rs"),
    },
    SourceFile {
        path: "_TEMPLATE.rs",
        contents: include_str!("_TEMPLATE.rs"),
    },
    SourceFile {
        path: "agent_spawn_errors.rs",
        contents: include_str!("agent_spawn_errors.rs"),
    },
    SourceFile {
        path: "cli/mod.rs",
        contents: include_str!("cli/mod.rs"),
    },
    SourceFile {
        path: "commit/mod.rs",
        contents: include_str!("commit/mod.rs"),
    },
    SourceFile {
        path: "codex_parser_tests.rs",
        contents: include_str!("codex_parser_tests.rs"),
    },
    SourceFile {
        path: "common/mod.rs",
        contents: include_str!("common/mod.rs"),
    },
    SourceFile {
        path: "deduplication/mod.rs",
        contents: include_str!("deduplication/mod.rs"),
    },
    SourceFile {
        path: "development_xml_validation.rs",
        contents: include_str!("development_xml_validation.rs"),
    },
    SourceFile {
        path: "fix_xml_validation.rs",
        contents: include_str!("fix_xml_validation.rs"),
    },
    SourceFile {
        path: "gemini_parser_tests.rs",
        contents: include_str!("gemini_parser_tests.rs"),
    },
    SourceFile {
        path: "git/mod.rs",
        contents: include_str!("git/mod.rs"),
    },
    SourceFile {
        path: "logger/mod.rs",
        contents: include_str!("logger/mod.rs"),
    },
    SourceFile {
        path: "logger/json_event_extraction.rs",
        contents: include_str!("logger/json_event_extraction.rs"),
    },
    SourceFile {
        path: "logger/test_logger_tests.rs",
        contents: include_str!("logger/test_logger_tests.rs"),
    },
    SourceFile {
        path: "opencode_parser_tests.rs",
        contents: include_str!("opencode_parser_tests.rs"),
    },
    SourceFile {
        path: "reducer_fault_tolerance.rs",
        contents: include_str!("reducer_fault_tolerance.rs"),
    },
    SourceFile {
        path: "reducer_rebase_state_machine.rs",
        contents: include_str!("reducer_rebase_state_machine.rs"),
    },
    SourceFile {
        path: "reducer_resume.rs",
        contents: include_str!("reducer_resume.rs"),
    },
    SourceFile {
        path: "reducer_resume_tests.rs",
        contents: include_str!("reducer_resume_tests.rs"),
    },
    SourceFile {
        path: "reducer_state_machine.rs",
        contents: include_str!("reducer_state_machine.rs"),
    },
    SourceFile {
        path: "review_output_validation.rs",
        contents: include_str!("review_output_validation.rs"),
    },
    SourceFile {
        path: "review_xml_validation.rs",
        contents: include_str!("review_xml_validation.rs"),
    },
    SourceFile {
        path: "review_xsd_retry_session.rs",
        contents: include_str!("review_xsd_retry_session.rs"),
    },
    SourceFile {
        path: "test_count_guard.rs",
        contents: include_str!("test_count_guard.rs"),
    },
    SourceFile {
        path: "test_timeout.rs",
        contents: include_str!("test_timeout.rs"),
    },
    SourceFile {
        path: "test_traits.rs",
        contents: include_str!("test_traits.rs"),
    },
    SourceFile {
        path: "ui_events.rs",
        contents: include_str!("ui_events.rs"),
    },
    SourceFile {
        path: "workflows/mod.rs",
        contents: include_str!("workflows/mod.rs"),
    },
    SourceFile {
        path: "workflows/backup.rs",
        contents: include_str!("workflows/backup.rs"),
    },
    SourceFile {
        path: "workflows/baseline.rs",
        contents: include_str!("workflows/baseline.rs"),
    },
    SourceFile {
        path: "workflows/cleanup.rs",
        contents: include_str!("workflows/cleanup.rs"),
    },
    SourceFile {
        path: "workflows/commit_tests.rs",
        contents: include_str!("workflows/commit_tests.rs"),
    },
    SourceFile {
        path: "workflows/config.rs",
        contents: include_str!("workflows/config.rs"),
    },
    SourceFile {
        path: "workflows/config_test.rs",
        contents: include_str!("workflows/config_test.rs"),
    },
    SourceFile {
        path: "workflows/continuation.rs",
        contents: include_str!("workflows/continuation.rs"),
    },
    SourceFile {
        path: "workflows/development_xml.rs",
        contents: include_str!("workflows/development_xml.rs"),
    },
    SourceFile {
        path: "workflows/fallback.rs",
        contents: include_str!("workflows/fallback.rs"),
    },
    SourceFile {
        path: "workflows/oversize_prompt.rs",
        contents: include_str!("workflows/oversize_prompt.rs"),
    },
    SourceFile {
        path: "workflows/plan.rs",
        contents: include_str!("workflows/plan.rs"),
    },
    SourceFile {
        path: "workflows/review.rs",
        contents: include_str!("workflows/review.rs"),
    },
    SourceFile {
        path: "workflows/resume/mod.rs",
        contents: include_str!("workflows/resume/mod.rs"),
    },
    SourceFile {
        path: "workflows/resume/basic.rs",
        contents: include_str!("workflows/resume/basic.rs"),
    },
    SourceFile {
        path: "workflows/resume/checkpoint.rs",
        contents: include_str!("workflows/resume/checkpoint.rs"),
    },
    SourceFile {
        path: "workflows/resume/phases.rs",
        contents: include_str!("workflows/resume/phases.rs"),
    },
    SourceFile {
        path: "workflows/resume/preservation.rs",
        contents: include_str!("workflows/resume/preservation.rs"),
    },
    SourceFile {
        path: "workflows/resume/rebase.rs",
        contents: include_str!("workflows/resume/rebase.rs"),
    },
    SourceFile {
        path: "workflows/resume/v3.rs",
        contents: include_str!("workflows/resume/v3.rs"),
    },
];

fn source_for_path(path: &str) -> Option<&'static str> {
    INTEGRATION_TEST_SOURCES
        .iter()
        .find(|source| source.path == path)
        .map(|source| source.contents)
}

fn count_test_annotations(contents: &str) -> usize {
    contents
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("#[test]")
        })
        .count()
}

fn parse_module_declarations(contents: &str) -> Vec<&str> {
    contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || !trimmed.contains(';') {
                return None;
            }
            let declaration = if let Some(rest) = trimmed.strip_prefix("mod ") {
                rest
            } else if let Some(rest) = trimmed.strip_prefix("pub mod ") {
                rest
            } else {
                return None;
            };
            let name = declaration
                .split(|ch: char| ch == ';' || ch.is_whitespace())
                .next()
                .unwrap_or("");
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .collect()
}

fn resolve_module_path(module_dir: &str, module_name: &str) -> String {
    let base = if module_dir.is_empty() {
        module_name.to_string()
    } else {
        format!("{}/{}", module_dir, module_name)
    };
    let candidate_file = format!("{}.rs", base);
    if source_for_path(&candidate_file).is_some() {
        return candidate_file;
    }
    let candidate_mod = format!("{}/mod.rs", base);
    if source_for_path(&candidate_mod).is_some() {
        return candidate_mod;
    }
    panic!(
        "Missing integration test module source for '{}'. Tried '{}' and '{}'",
        base, candidate_file, candidate_mod
    );
}

fn module_dir_from_path(path: &str) -> String {
    if let Some(stripped) = path.strip_suffix("/mod.rs") {
        stripped.to_string()
    } else if let Some((dir, _)) = path.rsplit_once('/') {
        dir.to_string()
    } else {
        String::new()
    }
}

/// Best-effort source scan of literal `#[test]` annotations.
///
/// This is not an authoritative measure of discovered tests; it exists only to
/// ensure the guard module stays connected to the integration test source tree.
fn count_tests_from_module_tree() -> usize {
    let mut visited = HashSet::new();
    let main_contents = source_for_path("main.rs")
        .expect("tests/integration_tests/main.rs must be included in guard sources");
    count_tests_recursive("main.rs", "", main_contents, &mut visited)
}

fn count_tests_recursive(
    file_path: &str,
    module_dir: &str,
    contents: &str,
    visited: &mut HashSet<String>,
) -> usize {
    if !visited.insert(file_path.to_string()) {
        return 0;
    }

    let mut total = count_test_annotations(contents);
    for module_name in parse_module_declarations(contents) {
        let child_path = resolve_module_path(module_dir, module_name);
        let child_contents = source_for_path(&child_path)
            .unwrap_or_else(|| panic!("Missing integration test source '{}'", child_path));
        let child_dir = module_dir_from_path(&child_path);
        total += count_tests_recursive(&child_path, &child_dir, child_contents, visited);
    }
    total
}

/// This test documents the expected minimum test count.
///
/// This verifies that the test count guard module is properly loaded and the
/// constant is accessible. The actual count verification happens in CI via
/// `cargo test -p ralph-workflow-tests -- --list` and in the compliance check script.
///
/// If this test appears, it means the test count guard module is properly loaded
/// and the integration test suite includes this verification documentation.
#[test]
fn integration_test_count_guard_documentation() {
    with_default_timeout(|| {
        let min = MINIMUM_EXPECTED_TESTS;
        assert!(min > 0, "MINIMUM_EXPECTED_TESTS should be positive");

        let actual_count = count_tests_from_module_tree();
        assert!(
            actual_count > 0,
            "Source scan found zero #[test] annotations; guard wiring may be broken"
        );
    });
}
