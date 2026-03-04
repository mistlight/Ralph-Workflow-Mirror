#!/bin/bash
# Audit integration tests for implementation detail leaks

set -e

echo "=== Checking for cfg!(test) usage ==="
# Check for actual code usage, not documentation/comments
# Allow #[cfg(test)] on module declarations (legitimate usage for test modules)
violations=$(rg "cfg!\(test\)|#\[cfg\(test\)\]" tests/integration_tests/ --type rust | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" | \
  grep -v "^[^:]*:#\[cfg(test)\][[:space:]]*$" || true)
if [ -n "$violations" ]; then
    echo "ERROR: cfg!(test) usage detected in integration tests"
    echo "Per integration testing guide, avoid cfg!(test) in production code."
    echo "Violations found:"
    echo "$violations"
    exit 1
else
    echo "None found ✓"
fi

printf "\n=== Checking for real filesystem usage ===\n"
# Check for actual code usage, not documentation/comments
# Filter out lines that start with // or /* or * (comments)
violations=$(rg "std::fs::|TempDir|tempfile::" tests/integration_tests/ --type rust | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$violations" ]; then
    echo "ERROR: Real filesystem usage detected in integration tests"
    echo "Per integration testing guide, integration tests must use MemoryWorkspace exclusively."
    echo "Use system tests for real filesystem operations."
    echo "Violations found:"
    echo "$violations"
    exit 1
else
    echo "None found ✓"
fi

printf "\n=== Checking for real process execution ===\n"
# Check for actual code usage, not documentation/comments
violations=$(rg "std::process::Command|Command::new" tests/integration_tests/ --type rust | \
  grep -v "MockProcessExecutor" | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$violations" ]; then
    echo "ERROR: Real process execution detected in integration tests"
    echo "Per integration testing guide, integration tests must use MockProcessExecutor."
    echo "Use system tests for real process execution."
    echo "Violations found:"
    echo "$violations"
    exit 1
else
    echo "None found ✓"
fi

printf "\n=== Checking for #[serial] in integration tests (BANNED) ===\n"
# #[serial] is BANNED in integration tests. It indicates a design problem:
# the test or production code couples to global mutable state. Fix by using
# dependency injection (env-injection pattern) instead of serializing tests.
# See docs/agents/testing-guide.md for details.
serial_violations=$(rg "#\[serial\]|use serial_test" tests/integration_tests/ --type rust | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$serial_violations" ]; then
    echo "ERROR: #[serial] or serial_test detected in integration tests"
    echo "This is BANNED. #[serial] in integration tests indicates a design problem:"
    echo "the test or production code couples to global mutable state."
    echo "Fix: use the env-injection pattern (see docs/agents/testing-guide.md)"
    echo "instead of serializing tests."
    echo "Violations found:"
    echo "$serial_violations"
    exit 1
else
    echo "No #[serial] in integration tests ✓"
fi

printf "\n=== Checking for #[serial] in src/ unit tests (BANNED) ===\n"
# #[serial] is also BANNED in src/ unit tests. Use the env-injection pattern
# (MemoryConfigEnvironment::with_env_var, or injectable Fn(&str)->Option<String>)
# instead of serializing tests around process environment mutation.
# System tests (tests/system_tests/) are excluded: their #[serial] usage is
# justified by libgit2 global state and documented in SYSTEM_TESTS.md.
VIOLATIONS=0
serial_src_violations=$(rg "#\[serial\]" ralph-workflow/src/ --type rust | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$serial_src_violations" ]; then
    echo "ERROR: #[serial] found in src/ unit tests (use env-injection pattern instead):"
    echo "$serial_src_violations"
    VIOLATIONS=$((VIOLATIONS + 1))
else
    echo "No #[serial] in src/ unit tests ✓"
fi
if [ "$VIOLATIONS" -gt 0 ]; then
    echo ""
    echo "Fix: refactor production code to accept injectable env accessor"
    echo "(MemoryConfigEnvironment::with_env_var or Fn(&str)->Option<String>)"
    echo "instead of calling std::env::var() directly."
    exit 1
fi

printf "\n=== Checking for test-helpers imports in src/ unit tests (WARNING) ===\n"
# test-helpers provides git2 utilities (init_git_repo, commit_all, with_temp_cwd, etc.)
# intended for git2-system-tests only. Importing test-helpers in src/ unit tests adds
# a git2 dependency to the unit test binary and can cause libgit2 global state issues
# when tests run in parallel.
# Goal: move these tests to tests/system_tests/ with #[serial].
# Currently WARNING (not error) because pre-existing violations exist and require
# a separate refactoring effort to move affected tests to the correct tier.
test_helpers_in_src=$(rg "use test_helpers::|init_git_repo|commit_all|git_switch" \
  ralph-workflow/src/ --type rust | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" | \
  grep -v "mod test_helpers" || true)
if [ -n "$test_helpers_in_src" ]; then
    echo "WARNING: test-helpers or git2 utilities found in src/ unit tests:"
    echo "$test_helpers_in_src"
    echo "Note: These tests should move to tests/system_tests/ with #[serial]."
    echo "Unit tests must remain parallel-safe and free of libgit2 dependencies."
    echo "This is a WARNING — promote to ERROR once pre-existing violations are resolved."
else
    echo "No test-helpers imports in src/ unit tests ✓"
fi

printf "\n=== Checking for std::env::set_var/remove_var in integration tests (BANNED) ===\n"
# Direct env mutation is BANNED in integration tests. It requires #[serial] to avoid
# races. Fix: refactor production code to accept an injectable env accessor
# (from_env_fn pattern). See docs/agents/testing-guide.md 'Env-Injection Pattern'.
env_mutations=$(rg "std::env::set_var|std::env::remove_var|env::set_var|env::remove_var" \
  tests/integration_tests/ --type rust | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$env_mutations" ]; then
    echo "ERROR: std::env::set_var/remove_var detected in integration tests"
    echo "Direct env mutation races with parallel tests and requires #[serial]."
    echo "Fix: use the env-injection pattern instead of mutating process environment."
    echo "See docs/agents/testing-guide.md 'Env-Injection Pattern' section."
    echo "Violations found:"
    echo "$env_mutations"
    exit 1
else
    echo "No env mutations in integration tests ✓"
fi

printf "\n=== Checking for MemoryWorkspace usage (should be present) ===\n"
workspace_count=$(rg "MemoryWorkspace" tests/integration_tests/ --type rust --count-matches | awk -F: '{sum+=$2} END {print sum}')
echo "MemoryWorkspace usage count: $workspace_count"

printf "\n=== Checking for MockProcessExecutor usage (should be present) ===\n"
mock_count=$(rg "MockProcessExecutor" tests/integration_tests/ --type rust --count-matches | awk -F: '{sum+=$2} END {print sum}')
echo "MockProcessExecutor usage count: $mock_count"

printf "\n=== Files over 1000 lines (should be split) ===\n"
find tests/integration_tests -name "*.rs" -exec wc -l {} \; | awk '$1 > 1000 {print}' || echo "None found ✓"

printf "\n=== Checking for internal field assertions ===\n"
rg "assert.*\.(internal_|_private|_impl)" tests/integration_tests/ --type rust || echo "None found ✓"

printf "\n=== Checking for TestPrinter/VirtualTerminal usage in parser tests ===\n"
parser_files=$(find tests/integration_tests -name "*parser*.rs" -o -name "*streaming*.rs")
if [ -n "$parser_files" ]; then
    missing_count=0
    for file in $parser_files; do
        # Check file itself or parent mod.rs
        dir=$(dirname "$file")
        if ! grep -q "TestPrinter\|VirtualTerminal" "$file" && ! grep -q "TestPrinter\|VirtualTerminal" "$dir/mod.rs" 2>/dev/null; then
            echo "WARNING: $file may not use TestPrinter or VirtualTerminal"
            missing_count=$((missing_count + 1))
        fi
    done
    if [ $missing_count -eq 0 ]; then
        echo "All parser tests use TestPrinter or VirtualTerminal ✓"
    fi
else
    echo "No parser test files found"
fi

printf "\n=== Checking for length assertions without content checks ===\n"
# Find .len() assertions and check if nearby lines have content assertions
# Exclusions (legitimate cases where length assertions are acceptable):
# - test_logger, TestPrinter, TestLogger: test utilities, length is the behavior being tested
# - get_logs, captured(): logger/output test helpers where count is meaningful
# - Summary, DebugSummary: test structures where field counts are part of the contract
# - _TEMPLATE.rs: documentation files, not actual tests
# - Lines with // OK: explicitly marked as acceptable by developer
# - Comment lines (//|/*|*): documentation, not actual assertions
len_issues=$(rg -A 5 "assert.*\.len\(\)" tests/integration_tests/ --type rust | \
  grep -v "test_logger\|TestPrinter\|TestLogger\|get_logs\|// OK\|captured()\|Summary\|DebugSummary\|_TEMPLATE\.rs\|^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*\*" | \
  grep "assert_eq.*\.len()" | wc -l)
if [ "$len_issues" -gt 0 ]; then
    echo "ERROR: Found $len_issues potential length assertions without content checks"
    echo "Per integration testing guide, length assertions must be combined with content verification."
    echo "Violations found:"
    rg -A 5 "assert.*\.len\(\)" tests/integration_tests/ --type rust | \
      grep -v "test_logger\|TestPrinter\|TestLogger\|get_logs\|// OK\|captured()\|Summary\|DebugSummary\|_TEMPLATE\.rs\|^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*\*" | \
      grep "assert_eq.*\.len()" | head -10
    exit 1
else
    echo "No suspicious length assertions found ✓"
fi

printf "\n=== Checking for thread::sleep in integration tests (prefer injectable timers) ===\n"
# thread::sleep in integration tests creates timing dependencies and flakiness.
# Use RetryTimer trait injection instead wherever possible.
# Exclusions:
#   timeout_file_activity.rs — tests the idle-timeout monitor and must use real sleep
#   channel_bounds.rs — tests real channel backpressure timing behaviour
#   test_timeout.rs — tests real wall-clock timeout firing behaviour
# This is a WARNING — promote to ERROR once all non-excluded files are confirmed clean.
sleep_violations=$(rg "thread::sleep|tokio::time::sleep" tests/integration_tests/ --type rust | \
  grep -v "timeout_file_activity\|channel_bounds\|test_timeout" | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$sleep_violations" ]; then
    echo "WARNING: thread::sleep or tokio::time::sleep in integration tests (should use RetryTimer injection):"
    echo "$sleep_violations"
    echo "Note: This is a warning - consider replacing with injectable delay via RetryTimer trait."
else
    echo "No avoidable sleep in integration tests ✓"
fi

printf "\n=== Checking for tests with implementation-focused names ===\n"
# Tests with "internal_error" are OK (testing error types, not implementation)
# Tests with "buffer" in test_logger_tests.rs are OK (testing utility behavior)
impl_names=$(rg "fn test.*(internal_[^e]|_buffer|_cache|_queue)" tests/integration_tests/ --type rust | \
  grep -v "test_logger" | wc -l)
if [ "$impl_names" -gt 0 ]; then
    echo "WARNING: Found $impl_names tests with potentially implementation-focused names"
    rg "fn test.*(internal_[^e]|_buffer|_cache|_queue)" tests/integration_tests/ --type rust | \
      grep -v "test_logger" | head -5
    echo "Note: This is a warning - manual review recommended to ensure tests focus on behavior"
else
    echo "All test names are behavior-focused ✓"
fi

printf "\n=== Checking for missing test documentation ===\n"
# This is a best-effort check - manual review recommended
total_tests=$(rg "^\s*#\[test\]" tests/integration_tests/ --type rust --count | \
  awk -F: '{sum+=$2} END {print sum}')
echo "Total #[test] annotations: $total_tests"
echo "Note: Most tests have documentation - manual spot-checks recommended"

printf "\n=== Verifying tests reference canonical testing guide ===\n"
# INTEGRATION_TESTS.md is a redirect stub; the canonical guide is docs/agents/testing-guide.md.
# Check that integration test files reference the canonical guide.
guide_refs=$(rg "testing-guide\.md" tests/integration_tests/ --type rust --count-matches | \
  awk -F: '{sum+=$2} END {print sum}')
echo "testing-guide.md references in integration tests: ${guide_refs:-0}"
if [ "${guide_refs:-0}" -lt 1 ]; then
  echo "WARNING: No references to docs/agents/testing-guide.md found in integration tests."
  echo "Consider adding a top-of-file comment linking to the canonical testing guide."
else
  echo "Canonical testing guide referenced ✓"
fi

printf "\n=== Checking for #[serial] in process_system_tests/ (BANNED) ===\n"
# process-system-tests runs in parallel; #[serial] is not allowed there.
# Tests that need intra-module serialization must use a module-local Mutex instead.
# git2-dependent tests must stay in system_tests/ (the serial binary).
serial_in_process_system=$(rg '#\[serial\]' tests/process_system_tests/ 2>/dev/null | \
  grep -v "^[^:]*:[[:space:]]*//\|^[^:]*:[[:space:]]*/\*\|^[^:]*:[[:space:]]*\*" || true)
if [ -n "$serial_in_process_system" ]; then
    echo "ERROR: #[serial] found in process_system_tests/ (not allowed):"
    echo "$serial_in_process_system"
    echo "Fix: use a module-local Mutex guard for env mutations, or move libgit2 tests to system_tests/"
    FAIL=1
else
    echo "No #[serial] in process_system_tests/ ✓"
fi

printf "\n=== Checking for libgit2 usage in process_system_tests/ (BANNED) ===\n"
# process-system-tests must not use git2 or init_git_repo — those tests belong in system_tests/
git2_in_process_system=$(rg -n 'git2::|init_git_repo' tests/process_system_tests/ 2>/dev/null || true)
if [ -n "$git2_in_process_system" ]; then
    echo "ERROR: git2:: or init_git_repo usage found in process_system_tests/ (use system_tests/ for libgit2 tests):"
    echo "$git2_in_process_system"
    FAIL=1
else
    echo "No libgit2 usage in process_system_tests/ ✓"
fi

printf "\n=== Checking #[ignore] attributes have issue URLs (flaky quarantine rule) ===\n"
# Every #[ignore] must include an issue URL so quarantined tests are tracked.
# Enforce: #[ignore = "flaky: https://github.com/.../issues/N"]
ignore_without_url=$(rg -n '#\[ignore\b' tests/ ralph-workflow/src/ \
  | grep -v 'https://' || true)
if [ -n "$ignore_without_url" ]; then
    echo "ERROR: #[ignore] without issue URL found (flaky quarantine requires a link):"
    echo "$ignore_without_url"
    echo "Fix: add an issue URL — e.g. #[ignore = \"flaky: https://github.com/org/repo/issues/N\"]"
    FAIL=1
else
    echo "All #[ignore] attributes include issue URLs ✓"
fi

if [ "${FAIL:-0}" -gt 0 ]; then
    echo ""
    echo "Audit FAILED. Fix all errors above before merging."
    exit 1
fi

printf "\n=== Audit complete ===\n"
