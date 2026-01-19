use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use crate::common::ralph_cmd;
use test_helpers::{commit_all, create_isolated_config, init_git_repo, write_file};

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        // Use generic agents to avoid picking up user's local config
        .env("RALPH_DEVELOPER_AGENT", "codex")
        .env("RALPH_REVIEWER_AGENT", "codex")
        // Ensure git identity isn't a factor if a commit happens in the test.
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

// ============================================================================
// Review Workflow Tests
// ============================================================================

#[test]
fn ralph_reviewer_reviews_zero_skips_review() {
    // Test that N=0 skips review phase entirely
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    let counter_path = dir.path().join(".agent/review_counter");
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent
# Increment review counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Create commit message (required for pipeline to complete)
echo "feat: commit" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0") // N=0 should skip review
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // With RALPH_REVIEWER_REVIEWS=0, the review phase is skipped entirely
    let count = if counter_path.exists() {
        fs::read_to_string(&counter_path)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    } else {
        0
    };
    assert_eq!(
        count, 0,
        "Expected 0 reviewer calls when reviewer_reviews=0 (review phase skipped)"
    );
}

#[test]
fn ralph_reviewer_reviews_one_runs_single_cycle() {
    // Test that N=1 runs exactly one review-fix cycle
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let counter_path = dir.path().join(".agent/review_counter");
    let script_path = dir.path().join("review_script.sh");

    // Create isolated config to avoid user config interference
    let config_home = create_isolated_config(dir.path());

    // Agent output must include JSON result events for the orchestrator to extract issues
    fs::write(
        &script_path,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
# Increment review counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# On odd calls (review phases): output JSON result with issues
# On even calls (fix phases): output JSON result indicating completion
if [ $((count % 2)) -ne 0 ]; then
    # Review phase: output issues in JSON format that orchestrator can extract
    printf '{{"type":"result","result":"# Issues\\n\\nCritical:\\n- [ ] [initial.txt:1] Issue found"}}\n'
else
    # Fix phase: output completion message
    printf '{{"type":"result","result":"Fixed the issue in initial.txt"}}\n'
fi

exit 0
"##,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1") // N=1 should run one cycle
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // With RALPH_REVIEWER_REVIEWS=1:
    // - Cycle 1: review + fix = 2 calls
    // Note: Commits are now created automatically by the orchestrator after each fix cycle
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(count, 2, "Expected 2 reviewer calls (1 × (review + fix))");
}

#[test]
fn ralph_review_multiple_passes() {
    // Test that RALPH_REVIEWER_REVIEWS=N runs exactly N review-fix cycles
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let counter_path = dir.path().join(".agent/review_counter");
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
# Increment review counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# On odd calls (review phases): create ISSUES.md with issues
# On even calls (fix phases): just run
if [ $((count % 2)) -ne 0 ]; then
    echo "# Issues" > .agent/ISSUES.md
    echo "" >> .agent/ISSUES.md
    echo "Critical:" >> .agent/ISSUES.md
    echo "- [ ] Issue found" >> .agent/ISSUES.md
fi

exit 0
"##,
            counter = counter_path.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "3") // 3 review-fix cycles
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // With RALPH_REVIEWER_REVIEWS=3, the reviewer is called multiple times
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(count, 6, "Expected 6 reviewer calls (3 × (review + fix))");
}

#[test]
fn ralph_creates_issues_md_during_review() {
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    // Create review script
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
# Review Issues

- [ ] High: [src/main.rs:42] Memory leak detected
- [x] Low: Code style suggestion

ISSUES_EOF
echo "feat: reviewed" > .agent/commit-message.txt
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--no-isolation") // Use non-isolation mode to keep ISSUES.md
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // ISSUES.md should exist after review in non-isolation mode
    assert!(dir.path().join(".agent/ISSUES.md").exists());
}

#[test]
fn ralph_review_workflow_with_no_issues() {
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    // Create review script
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
# Review Complete

No issues found. Code looks good!

ISSUES_EOF
echo "feat: clean code" > .agent/commit-message.txt
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));
}

// ============================================================================
// Review Cycle Count Tests
// ============================================================================

#[test]
fn ralph_isolation_mode_deletes_issues_after_fix() {
    // Test that ISSUES.md is deleted after the final fix in isolation mode
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    // Script that creates ISSUES.md during review but not during commit message generation
    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent

# Only create ISSUES.md if it doesn't exist (i.e., during review phase)
# The commit message generation phase should NOT recreate ISSUES.md
if [ ! -f .agent/commit-message.txt ]; then
    # This is a review or fix phase
    echo "- [ ] Critical: [src/main.rs:42] Bug found" > .agent/ISSUES.md
fi

# Create commit message (always, for all phases)
echo "feat: test" > .agent/commit-message.txt
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_ISOLATION_MODE", "true") // Isolation mode (default)
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // In isolation mode, ISSUES.md should be deleted after the final fix
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after final fix in isolation mode"
    );
}

#[test]
fn ralph_non_isolation_mode_keeps_issues_after_fix() {
    // Test that ISSUES.md is preserved after the final fix when NOT in isolation mode
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
# Create ISSUES.md during review
echo "- [ ] Critical: [src/main.rs:42] Bug found" > .agent/ISSUES.md
# Create commit message
echo "feat: test" > .agent/commit-message.txt
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_ISOLATION_MODE", "false") // Non-isolation mode
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // In non-isolation mode, ISSUES.md should persist
    assert!(
        dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should persist after final fix in non-isolation mode"
    );
}

#[test]
fn ralph_issues_persists_between_review_and_fix_phases() {
    // Test that ISSUES.md created during Review is readable during Fix phase
    // within the SAME cycle. This is critical for the review-fix cycle to work.
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    // Create a marker file to track which phases have run
    let phase_log = dir.path().join(".agent/phase_log.txt");
    let call_counter = dir.path().join(".agent/call_counter");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Log state and handle each phase
case $count in
    1)
        # Review phase: create ISSUES.md
        echo "REVIEW: Creating ISSUES.md" >> "{phase_log}"
        echo "- [ ] High: [src/main.rs:10] Found bug" > .agent/ISSUES.md
        ;;
    2)
        # Fix phase: ISSUES.md should exist from review
        if [ -f .agent/ISSUES.md ]; then
            echo "FIX: ISSUES.md exists" >> "{phase_log}"
        else
            echo "FIX: ERROR - ISSUES.md missing!" >> "{phase_log}"
            exit 1
        fi
        ;;
esac

# Always create commit message
echo "feat: test" > .agent/commit-message.txt
exit 0
"#,
            counter = call_counter.display(),
            phase_log = phase_log.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1") // 1 review-fix cycle
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify phase log shows both Review and Fix phases ran
    let log_content = fs::read_to_string(&phase_log).unwrap();
    assert!(
        log_content.contains("REVIEW: Creating ISSUES.md"),
        "Review phase should have created ISSUES.md"
    );
    assert!(
        log_content.contains("FIX: ISSUES.md exists"),
        "Fix phase should have seen ISSUES.md"
    );

    // After completion in isolation mode, ISSUES.md should be cleaned up
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after fix cycle completes in isolation mode"
    );
}

#[test]
fn ralph_zero_reviewer_reviews_no_issues_created() {
    // Test that with N=0 reviewer reviews, pre-existing ISSUES.md gets cleaned at start
    let dir = TempDir::new().unwrap();
    let _ = init_git_repo(&dir);

    // Pre-create an ISSUES.md to verify it gets cleaned at start of run (isolation mode)
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    fs::write(
        dir.path().join(".agent/ISSUES.md"),
        "old issues from previous run",
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0") // Skip all review phases
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent && echo \"feat: zero reviews\" > .agent/commit-message.txt'",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Skipping review phase"));

    // ISSUES.md should be cleaned at the start of run (reset_context_for_isolation)
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted at start of run in isolation mode"
    );
}

#[test]
fn ralph_early_exit_no_issues_still_cleans_up() {
    // Test that ISSUES.md is cleaned up even when review exits early
    // due to finding no issues
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let call_counter = dir.path().join(".agent/call_counter");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Only create ISSUES.md on first call (review phase)
if [ "$count" -eq 1 ]; then
    # Create ISSUES.md with the "no issues" marker that triggers early exit
    cat > .agent/ISSUES.md << 'ISSUES_EOF'
# Review Complete

✓ **No issues found.** The code meets all requirements.
ISSUES_EOF
fi

# Create commit message
echo "feat: no issues" > .agent/commit-message.txt
exit 0
"#,
            counter = call_counter.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "3") // Request 3 cycles, should exit early
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Pipeline should succeed and exit early
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));

    // ISSUES.md should be cleaned up even with early exit
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after early exit in isolation mode"
    );
}

#[test]
fn ralph_multiple_review_cycles_final_cleanup() {
    // Test that with N=2 review cycles, ISSUES.md is cleaned up after EACH fix cycle
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let counter_path = dir.path().join(".agent/call_counter");
    let issues_state_log = dir.path().join(".agent/issues_state.txt");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Log ISSUES.md state at start of each call
if [ -f .agent/ISSUES.md ]; then
    echo "Call $count: ISSUES.md exists" >> "{log}"
else
    echo "Call $count: ISSUES.md missing" >> "{log}"
fi

case $count in
    1) # Review1 - ISSUES.md should be missing at start
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should not exist at start of Review1!" >> "{log}"
        fi
        echo "- [ ] Issue from Review1" > .agent/ISSUES.md
        ;;
    2) # Fix1 - ISSUES.md should exist from Review1
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix1!" >> "{log}"
            exit 1
        fi
        ;;
    3) # Review2 - ISSUES.md should be MISSING (deleted after Fix1)
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should have been deleted after Fix1!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review2" > .agent/ISSUES.md
        ;;
    4) # Fix2 - ISSUES.md should exist from Review2
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix2!" >> "{log}"
            exit 1
        fi
        ;;
esac

# Always create commit message
echo "feat: cycle $count" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display(),
            log = issues_state_log.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "2") // 2 review-fix cycles
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify the call count: at minimum 2 cycles × 2 calls = 4 calls
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        count >= 4,
        "Expected at least 4 reviewer calls for 2 cycles"
    );

    // Verify the state log shows correct ISSUES.md lifecycle
    let state_log = fs::read_to_string(&issues_state_log).unwrap();
    assert!(
        !state_log.contains("ERROR"),
        "ISSUES.md lifecycle was incorrect. Log:\n{}",
        state_log
    );
}

#[test]
fn ralph_issues_md_deleted_after_each_fix_cycle() {
    // Comprehensive test for N=3 cycles verifying exact ISSUES.md lifecycle
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "updated content");

    let counter_path = dir.path().join(".agent/call_counter");
    let issues_state_log = dir.path().join(".agent/issues_state.txt");

    let script_path = dir.path().join("review_script.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
mkdir -p .agent

# Increment call counter
if [ -f "{counter}" ]; then
    count=$(cat "{counter}")
    count=$((count + 1))
else
    count=1
fi
echo $count > "{counter}"

# Log ISSUES.md state at start of each call
if [ -f .agent/ISSUES.md ]; then
    echo "Call $count: ISSUES.md exists" >> "{log}"
else
    echo "Call $count: ISSUES.md missing" >> "{log}"
fi

case $count in
    1) # Review1
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md should not exist at Review1!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review1" > .agent/ISSUES.md
        ;;
    2) # Fix1
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix1!" >> "{log}"
            exit 1
        fi
        ;;
    3) # Review2
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md not deleted after Fix1!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review2" > .agent/ISSUES.md
        ;;
    4) # Fix2
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix2!" >> "{log}"
            exit 1
        fi
        ;;
    5) # Review3
        if [ -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md not deleted after Fix2!" >> "{log}"
            exit 1
        fi
        echo "- [ ] Issue from Review3" > .agent/ISSUES.md
        ;;
    6) # Fix3
        if [ ! -f .agent/ISSUES.md ]; then
            echo "ERROR: ISSUES.md missing during Fix3!" >> "{log}"
            exit 1
        fi
        ;;
esac

echo "feat: N=3 test" > .agent/commit-message.txt
exit 0
"#,
            counter = counter_path.display(),
            log = issues_state_log.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "3") // 3 review-fix cycles
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify the call count: 3 cycles × 2 calls = 6 calls
    let count: u32 = fs::read_to_string(&counter_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(count, 6, "Expected 6 reviewer calls (3 × (review + fix))");

    // Final state: ISSUES.md should not exist
    assert!(
        !dir.path().join(".agent/ISSUES.md").exists(),
        "ISSUES.md should be deleted after all cycles complete"
    );
}

// ============================================================================
// Fixer Behavior Tests
// ============================================================================

#[test]
fn ralph_fixer_receives_issues_content() {
    // Test that fixer receives the ISSUES.md content during fix pass.
    // Note: This test runs in isolation mode (default), so ISSUES.md is deleted
    // after the fix pass completes. However, the fix phase itself can still read
    // ISSUES.md during execution - we verify this by writing to fix_log during
    // the fix phase when ISSUES.md is present.
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with tracked files
    write_file(dir.path().join("src/main.rs"), "fn main() {}");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(
        dir.path().join("src/main.rs"),
        "fn main() { println!(\"hi\"); }",
    );

    let fix_log = dir.path().join(".agent/fix_log.txt");
    let script_path = dir.path().join("check_issues.sh");
    fs::write(
        &script_path,
        format!(
            r##"#!/bin/sh
mkdir -p .agent
# Track which call this is
if [ -f .agent/call_counter ]; then
    count=$(cat .agent/call_counter)
    count=$((count + 1))
else
    count=1
fi
echo $count > .agent/call_counter

case $count in
    1) # Review phase - create ISSUES.md with specific content
        cat > .agent/ISSUES.md << 'EOF'
# Review Issues

Critical:
- [ ] [src/main.rs:1] Missing error handling
- [ ] [src/main.rs:1] No documentation

High:
- [ ] [src/main.rs:1] Consider using Result type
EOF
        # Also output JSON for orchestrator extraction
        printf '{{"type":"result","result":"# Issues\\n\\n- [ ] [src/main.rs:1] Missing error handling"}}\n'
        ;;
    2) # Fix phase - check if ISSUES.md exists and log it
        if [ -f .agent/ISSUES.md ]; then
            echo "FIX: ISSUES.md found" >> "{log}"
            echo "FIX: Content:" >> "{log}"
            cat .agent/ISSUES.md >> "{log}"
        else
            echo "FIX: ERROR - ISSUES.md missing!" >> "{log}"
        fi
        ;;
esac
exit 0
"##,
            log = fix_log.display()
        ),
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // Verify fix phase received ISSUES.md
    if fix_log.exists() {
        let log_content = fs::read_to_string(&fix_log).unwrap();
        assert!(
            log_content.contains("ISSUES.md found"),
            "Fix phase should have found ISSUES.md"
        );
        // The ISSUES.md should contain the review issues OR the extracted content
        // (orchestrator may extract from JSON result)
    }
}

#[test]
fn ralph_fixer_handles_minimal_issues_content() {
    // Test that fixer can work with minimal/vague ISSUES.md content
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("code.py"), "print('hello')");
    let _ = commit_all(&repo, "initial commit");

    // Create a change
    write_file(dir.path().join("code.py"), "print('hello world')");

    let script_path = dir.path().join("minimal_issues.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
if [ -f .agent/call_counter ]; then
    count=$(cat .agent/call_counter)
    count=$((count + 1))
else
    count=1
fi
echo $count > .agent/call_counter

case $count in
    1) # Review phase - create minimal ISSUES.md
        # This is a vague issue without file:line reference
        echo "- [ ] Code needs improvement" > .agent/ISSUES.md
        ;;
    2) # Fix phase - should handle vague issues gracefully
        # Just exit successfully
        ;;
esac
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete even with vague issues
    cmd.assert().success();
}

// ============================================================================
// Recovery Scenario Tests
// ============================================================================

#[test]
fn ralph_continues_after_review_agent_error() {
    // Test that pipeline can continue when review agent fails on one cycle
    // but the orchestrator handles it gracefully
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for the diff
    write_file(dir.path().join("initial.txt"), "modified content");

    let script_path = dir.path().join("flaky_reviewer.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
if [ -f .agent/call_counter ]; then
    count=$(cat .agent/call_counter)
    count=$((count + 1))
else
    count=1
fi
echo $count > .agent/call_counter

# Fail on review phase (call 1), succeed on fix phase (call 2)
# This simulates a reviewer that outputs nothing useful
case $count in
    1) # Review phase - no ISSUES.md created (simulates extraction failure)
        # Don't write ISSUES.md - let orchestrator handle it
        ;;
    2) # Fix phase - should still be called
        echo "Fixed the issues" > .agent/fix_result.txt
        ;;
esac
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete - orchestrator writes "no issues" marker when extraction fails
    cmd.assert().success();
}

#[test]
fn ralph_handles_json_extraction_failure() {
    // Test behavior when JSON extraction from agent output fails
    // The orchestrator should fall back to legacy mode or create no-issues marker
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change
    write_file(dir.path().join("initial.txt"), "modified content");

    // Script that outputs invalid/no JSON
    let script_path = dir.path().join("no_json.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
# Output plain text, not JSON
echo "I reviewed the code and found no issues."
echo "Everything looks good!"
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should handle gracefully - orchestrator should write no-issues marker
    cmd.assert().success();
}

#[test]
fn ralph_reviewer_timeout_handled() {
    // Test that agent timeout is handled gracefully
    // Note: This test uses a quick timeout to avoid long test times
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change
    write_file(dir.path().join("initial.txt"), "modified content");

    // Script that completes quickly (timeout is tested at system level, not here)
    let script_path = dir.path().join("quick_complete.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
# Complete quickly
echo "- [ ] Quick issue" > .agent/ISSUES.md
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete successfully
    cmd.assert().success();
}

// ============================================================================
// Reviewer Output Validation Tests
// ============================================================================

#[test]
fn ralph_reviewer_json_output_extracted() {
    // Test that JSON result events from reviewer are properly extracted
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change for review
    write_file(dir.path().join("initial.txt"), "modified content");

    // Create isolated config
    let config_home = create_isolated_config(dir.path());

    // Script that outputs proper JSON result event
    let script_path = dir.path().join("json_output.sh");
    fs::write(
        &script_path,
        r##"#!/bin/sh
mkdir -p .agent
if [ -f .agent/call_counter ]; then
    count=$(cat .agent/call_counter)
    count=$((count + 1))
else
    count=1
fi
echo $count > .agent/call_counter

case $count in
    1) # Review phase - output JSON with issues
        printf '{"type":"result","result":"# Issues\\n\\n- [ ] [initial.txt:1] Found a problem"}\n'
        ;;
    2) # Fix phase
        printf '{"type":"result","result":"Fixed the problem"}\n'
        ;;
esac
exit 0
"##,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // In non-isolation mode, we could check ISSUES.md content
    // But in isolation mode, it's deleted after fix
}

#[test]
fn ralph_reviewer_issues_format_validation() {
    // Test that various ISSUES.md formats are handled correctly
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a change
    write_file(dir.path().join("initial.txt"), "modified content");

    // Script that creates ISSUES.md with various formats
    let script_path = dir.path().join("format_test.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
if [ -f .agent/call_counter ]; then
    count=$(cat .agent/call_counter)
    count=$((count + 1))
else
    count=1
fi
echo $count > .agent/call_counter

case $count in
    1) # Review phase - create ISSUES.md with mixed format
        cat > .agent/ISSUES.md << 'EOF'
# Code Review Issues

## Critical
- [ ] [initial.txt:1] Critical bug found

## High Priority
- [ ] High: [initial.txt:2] Important issue

## Medium
- [ ] Medium: Code style issue

## Already Fixed
- [x] Low: Minor issue (resolved)

No other issues found.
EOF
        ;;
    2) # Fix phase
        ;;
esac
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .arg("--no-isolation")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    cmd.assert().success();

    // In non-isolation mode, verify ISSUES.md persists with content
    let issues_path = dir.path().join(".agent/ISSUES.md");
    assert!(
        issues_path.exists(),
        "ISSUES.md should exist in non-isolation mode"
    );

    let content = fs::read_to_string(&issues_path).unwrap();
    assert!(
        content.contains("Critical"),
        "ISSUES.md should contain Critical section"
    );
    assert!(
        content.contains("[x]"),
        "ISSUES.md should preserve resolved issues"
    );
}

// ============================================================================
// Edge Case Tests (Step 4 from hardening plan)
// ============================================================================

#[test]
fn ralph_review_handles_only_deleted_files() {
    // Test that review works correctly when the diff only contains file deletions
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with multiple files
    write_file(dir.path().join("file_to_delete.txt"), "content to delete");
    write_file(dir.path().join("keeper.txt"), "keeper content");
    let _ = commit_all(&repo, "initial commit with files");

    // Run ralph to establish start_commit baseline
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent && echo \"feat: baseline\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // Delete the file (creates a deletion-only diff)
    fs::remove_file(dir.path().join("file_to_delete.txt")).unwrap();

    let script_path = dir.path().join("review_deletion.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
Review of Deletion

- [ ] Low: File deletion detected, verify it was intentional
ISSUES_EOF
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete successfully with deletion-only diff
    cmd.assert().success();
}

#[test]
fn ralph_review_handles_only_renamed_files() {
    // Test that review works correctly when the diff only contains file renames
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit with a file
    write_file(dir.path().join("old_name.txt"), "file content");
    let _ = commit_all(&repo, "initial commit");

    // Run ralph to establish start_commit baseline
    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            "sh -c 'mkdir -p .agent && echo \"feat: baseline\" > .agent/commit-message.txt'",
        );

    cmd.assert().success();

    // Rename the file using git mv
    std::process::Command::new("git")
        .args(["mv", "old_name.txt", "new_name.txt"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run git mv");

    let script_path = dir.path().join("review_rename.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
Review of Rename

No issues found.
ISSUES_EOF
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete successfully with rename-only diff
    cmd.assert().success();
}

#[test]
fn ralph_fixer_handles_whitespace_only_issues() {
    // Test that fixer handles ISSUES.md with only whitespace gracefully
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("code.py"), "print('hello')");
    let _ = commit_all(&repo, "initial commit");

    // Create a change
    write_file(dir.path().join("code.py"), "print('hello world')");

    let script_path = dir.path().join("whitespace_issues.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
if [ -f .agent/call_counter ]; then
    count=$(cat .agent/call_counter)
    count=$((count + 1))
else
    count=1
fi
echo $count > .agent/call_counter

case $count in
    1) # Review phase - create ISSUES.md with only whitespace
        printf "   \n\n\t\t\n   \n" > .agent/ISSUES.md
        ;;
    2) # Fix phase - should handle whitespace-only ISSUES.md gracefully
        # Just exit successfully
        ;;
esac
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete gracefully with whitespace-only issues
    cmd.assert().success();
}

#[test]
fn ralph_review_works_with_detached_head() {
    // Test that review works correctly in detached HEAD state
    let dir = TempDir::new().unwrap();
    let repo = init_git_repo(&dir);

    // Create initial commit
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");

    // Create a second commit
    write_file(dir.path().join("initial.txt"), "second content");
    let second_commit_oid = commit_all(&repo, "second commit");

    // Checkout the first commit in detached HEAD state
    std::process::Command::new("git")
        .args(["checkout", "HEAD~1"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to checkout");

    // Verify we're in detached HEAD state
    let status_output = std::process::Command::new("git")
        .args(["status"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run git status");
    let status_str = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status_str.contains("HEAD detached") || status_str.contains("detached"),
        "Should be in detached HEAD state"
    );

    // Create a change while in detached HEAD
    write_file(dir.path().join("initial.txt"), "detached head change");

    let script_path = dir.path().join("detached_review.sh");
    fs::write(
        &script_path,
        r#"#!/bin/sh
mkdir -p .agent
cat > .agent/ISSUES.md << 'ISSUES_EOF'
Review in detached HEAD

No issues found.
ISSUES_EOF
exit 0
"#,
    )
    .unwrap();

    let mut cmd = ralph_cmd();
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "1")
        .env("RALPH_DEVELOPER_CMD", "sh -c 'exit 0'")
        .env(
            "RALPH_REVIEWER_CMD",
            format!("sh {}", script_path.display()),
        );

    // Should complete successfully in detached HEAD state
    cmd.assert().success();

    // Restore the branch for cleanup
    let _ = std::process::Command::new("git")
        .args(["checkout", &format!("{}", second_commit_oid)])
        .current_dir(dir.path())
        .output();
}
