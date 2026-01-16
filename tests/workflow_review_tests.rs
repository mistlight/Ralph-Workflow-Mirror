use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

use test_helpers::init_git_repo;

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

    // Pre-create an ISSUES.md to verify it gets cleaned at start of run (isolation mode)
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    fs::write(
        dir.path().join(".agent/ISSUES.md"),
        "old issues from previous run",
    )
    .unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
    init_git_repo(&dir);

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

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
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
