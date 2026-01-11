#!/usr/bin/env zsh
#
# Unit tests for ralph.zsh helper functions
#
# Run: zsh test_ralph.zsh
#
# This is a minimal test harness for zsh scripts. It sources the helper
# functions from ralph.zsh and validates their behavior.
#
set -euo pipefail

############################################
# Test framework (minimal assertion helpers)
############################################
typeset -g TESTS_RUN=0
typeset -g TESTS_PASSED=0
typeset -g TESTS_FAILED=0

# ANSI colors for test output
if [[ -t 1 ]]; then
  T_GREEN=$'\e[32m'
  T_RED=$'\e[31m'
  T_YELLOW=$'\e[33m'
  T_RESET=$'\e[0m'
  T_BOLD=$'\e[1m'
else
  T_GREEN="" T_RED="" T_YELLOW="" T_RESET="" T_BOLD=""
fi

test_pass() {
  local name="$1"
  ((TESTS_PASSED++))
  print "${T_GREEN}✓${T_RESET} $name"
}

test_fail() {
  local name="$1"
  local reason="${2:-}"
  ((TESTS_FAILED++))
  print "${T_RED}✗${T_RESET} $name"
  [[ -n "$reason" ]] && print "  ${T_YELLOW}→ $reason${T_RESET}"
}

assert_eq() {
  local expected="$1" actual="$2" name="${3:-assertion}"
  ((TESTS_RUN++))
  if [[ "$expected" == "$actual" ]]; then
    test_pass "$name"
    return 0
  else
    test_fail "$name" "expected '$expected', got '$actual'"
    return 1
  fi
}

assert_contains() {
  local haystack="$1" needle="$2" name="${3:-assertion}"
  ((TESTS_RUN++))
  if [[ "$haystack" == *"$needle"* ]]; then
    test_pass "$name"
    return 0
  else
    test_fail "$name" "expected string to contain '$needle'"
    return 1
  fi
}

assert_not_empty() {
  local value="$1" name="${2:-assertion}"
  ((TESTS_RUN++))
  if [[ -n "$value" ]]; then
    test_pass "$name"
    return 0
  else
    test_fail "$name" "expected non-empty value"
    return 1
  fi
}

assert_numeric() {
  local value="$1" name="${2:-assertion}"
  ((TESTS_RUN++))
  if [[ "$value" =~ ^[0-9]+$ ]]; then
    test_pass "$name"
    return 0
  else
    test_fail "$name" "expected numeric value, got '$value'"
    return 1
  fi
}

run_test() {
  local name="$1"
  local fn="$2"
  print ""
  print "${T_BOLD}▸ $name${T_RESET}"
  if $fn; then
    return 0
  else
    return 1
  fi
}

############################################
# Source library modules for testing
############################################
RALPH_SCRIPT_DIR="${0:A:h}"
source "${RALPH_SCRIPT_DIR}/lib/colors.zsh"
source "${RALPH_SCRIPT_DIR}/lib/timer.zsh"
source "${RALPH_SCRIPT_DIR}/lib/utils.zsh"
source "${RALPH_SCRIPT_DIR}/lib/json_parser.zsh"
source "${RALPH_SCRIPT_DIR}/lib/git_helpers.zsh"
source "${RALPH_SCRIPT_DIR}/lib/prompts.zsh"
source "${RALPH_SCRIPT_DIR}/lib/agents.zsh"

############################################
# Tests
############################################

test_ts_format() {
  local result="$(ts)"
  # Format: YYYY-MM-DD HH:MM:SS
  assert_contains "$result" "-" "ts() contains dashes"
  assert_contains "$result" ":" "ts() contains colons"
  # Check length (19 chars for full timestamp)
  local len=${#result}
  assert_eq "19" "$len" "ts() returns 19-character string"
}

test_timer_start() {
  timer_start
  assert_numeric "$RALPH_START_TIME" "RALPH_START_TIME is numeric after timer_start"
  assert_numeric "$RALPH_PHASE_START" "RALPH_PHASE_START is numeric after timer_start"
}

test_timer_elapsed_format() {
  RALPH_START_TIME=$((SECONDS - 65))  # 1m 5s ago
  local result="$(timer_elapsed)"
  assert_contains "$result" "m" "timer_elapsed contains 'm'"
  assert_contains "$result" "s" "timer_elapsed contains 's'"
  assert_eq "1m 05s" "$result" "timer_elapsed formats 65s as '1m 05s'"
}

test_timer_phase_elapsed() {
  RALPH_PHASE_START=$((SECONDS - 30))  # 30s ago
  local result="$(timer_phase_elapsed)"
  assert_eq "0m 30s" "$result" "timer_phase_elapsed formats 30s as '0m 30s'"
}

test_timer_zero_elapsed() {
  RALPH_START_TIME=$SECONDS
  local result="$(timer_elapsed)"
  assert_eq "0m 00s" "$result" "timer_elapsed at start is '0m 00s'"
}

test_print_progress_50_percent() {
  local result="$(print_progress 5 10 "Test")"
  assert_contains "$result" "50%" "print_progress shows 50%"
  assert_contains "$result" "5/10" "print_progress shows 5/10"
  assert_contains "$result" "Test:" "print_progress shows label"
  assert_contains "$result" "██████████░░░░░░░░░░" "print_progress has correct bar at 50%"
}

test_print_progress_100_percent() {
  local result="$(print_progress 10 10)"
  assert_contains "$result" "100%" "print_progress shows 100%"
  assert_contains "$result" "████████████████████" "print_progress shows full bar"
}

test_print_progress_0_percent() {
  local result="$(print_progress 0 10)"
  assert_contains "$result" "0%" "print_progress shows 0%"
  assert_contains "$result" "░░░░░░░░░░░░░░░░░░░░" "print_progress shows empty bar"
}

test_file_contains_marker_found() {
  local tmpfile="$(mktemp)"
  print "line1\nMARKER_TEST\nline3" > "$tmpfile"
  if file_contains_marker "$tmpfile" "MARKER_TEST"; then
    test_pass "file_contains_marker finds existing marker"
  else
    test_fail "file_contains_marker finds existing marker"
  fi
  ((TESTS_RUN++))
  rm -f "$tmpfile"
}

test_file_contains_marker_not_found() {
  local tmpfile="$(mktemp)"
  print "line1\nline2\nline3" > "$tmpfile"
  if file_contains_marker "$tmpfile" "NONEXISTENT"; then
    test_fail "file_contains_marker returns false for missing marker"
  else
    test_pass "file_contains_marker returns false for missing marker"
  fi
  ((TESTS_RUN++))
  rm -f "$tmpfile"
}

test_file_contains_marker_missing_file() {
  if file_contains_marker "/nonexistent/path/file.txt" "MARKER"; then
    test_fail "file_contains_marker returns false for missing file"
  else
    test_pass "file_contains_marker returns false for missing file"
  fi
  ((TESTS_RUN++))
}

############################################
# JSON Stream Parsing Tests
# (parse_claude_event and stream_parse_claude sourced from lib/json_parser.zsh)
############################################

test_stream_parse_complete_lines() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_complete_lines (skipped - jq not available)"
    return 0
  fi

  local result
  result=$(printf '{"type":"system","subtype":"init"}\n{"type":"result","subtype":"success"}\n' | stream_parse_claude)

  assert_contains "$result" "Session" "stream_parse handles init event"
  assert_contains "$result" "Completed" "stream_parse handles result event"
}

test_stream_parse_chunked_data() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_chunked_data (skipped - jq not available)"
    return 0
  fi

  local result
  result=$(printf '{"type":"system","subtype":"init"}\n' | stream_parse_claude)

  assert_contains "$result" "Session" "stream_parse handles single JSON line"
}

test_stream_parse_no_trailing_newline() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_no_trailing_newline (skipped - jq not available)"
    return 0
  fi

  local result
  result=$(printf '{"type":"system","subtype":"init"}' | stream_parse_claude)

  assert_contains "$result" "Session" "stream_parse handles JSON without trailing newline"
}

test_stream_parse_empty_lines() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_empty_lines (skipped - jq not available)"
    return 0
  fi

  local result
  result=$(printf '{"type":"system","subtype":"init"}\n\n{"type":"result","subtype":"success"}\n' | stream_parse_claude)

  assert_contains "$result" "Session" "stream_parse handles JSON with empty lines (init)"
  assert_contains "$result" "Completed" "stream_parse handles JSON with empty lines (result)"
}

test_detect_agent_type_claude() {
  local result
  result=$(detect_agent_type "claude -p --dangerously-skip-permissions")
  assert_eq "claude" "$result" "detect_agent_type identifies claude command"
}

test_detect_agent_type_codex() {
  local result
  result=$(detect_agent_type "codex exec --json --yolo")
  assert_eq "codex" "$result" "detect_agent_type identifies codex command"
}

test_detect_agent_type_unknown() {
  local result
  result=$(detect_agent_type "some-other-tool --flag")
  assert_eq "unknown" "$result" "detect_agent_type returns unknown for other commands"
}

############################################
# Git Helpers Tests
############################################

test_git_snapshot() {
  # Create a temp git repo for testing
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q
  git config user.email "test@test.com"
  git config user.name "Test"

  # Create a file and check status
  echo "test" > testfile.txt
  local result
  result=$(git_snapshot)
  assert_contains "$result" "??" "git_snapshot shows untracked file"

  # Add file and check status
  git add testfile.txt
  result=$(git_snapshot)
  assert_contains "$result" "A" "git_snapshot shows added file"

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_require_git_repo_in_repo() {
  # Create a temp git repo for testing
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  # Should not fail
  ((TESTS_RUN++))
  if require_git_repo 2>/dev/null; then
    test_pass "require_git_repo succeeds in a git repo"
  else
    test_fail "require_git_repo succeeds in a git repo"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_require_git_repo_outside_repo() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"

  # Should fail (exit) but we catch it
  ((TESTS_RUN++))
  if (require_git_repo 2>/dev/null); then
    test_fail "require_git_repo fails outside git repo"
  else
    test_pass "require_git_repo fails outside git repo"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_install_hook_creates_file() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  local hooks_dir="${tmpdir}/.git/hooks"
  install_hook "TestHook" "${hooks_dir}/pre-commit"

  ((TESTS_RUN++))
  if [[ -f "${hooks_dir}/pre-commit" ]]; then
    test_pass "install_hook creates hook file"
  else
    test_fail "install_hook creates hook file"
  fi

  # Check hook is executable
  ((TESTS_RUN++))
  if [[ -x "${hooks_dir}/pre-commit" ]]; then
    test_pass "install_hook makes hook executable"
  else
    test_fail "install_hook makes hook executable"
  fi

  # Check hook contains marker
  assert_contains "$(cat ${hooks_dir}/pre-commit)" "$HOOK_MARKER" "install_hook adds marker to hook"

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_start_end_agent_phase() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  # Start agent phase
  start_agent_phase

  ((TESTS_RUN++))
  if [[ -f ".no_agent_commit" ]]; then
    test_pass "start_agent_phase creates .no_agent_commit"
  else
    test_fail "start_agent_phase creates .no_agent_commit"
  fi

  # End agent phase
  end_agent_phase

  ((TESTS_RUN++))
  if [[ ! -f ".no_agent_commit" ]]; then
    test_pass "end_agent_phase removes .no_agent_commit"
  else
    test_fail "end_agent_phase removes .no_agent_commit"
  fi

  # Clean up wrapper
  disable_git_wrapper

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

############################################
# Verbosity / Truncation Tests
############################################

test_verbosity_truncate_limits() {
  # Test quiet mode (0)
  RALPH_VERBOSITY=0
  local limit=$(_get_truncate_limit "text")
  assert_eq "60" "$limit" "_get_truncate_limit returns 60 for text in quiet mode"

  # Test normal mode (1)
  RALPH_VERBOSITY=1
  limit=$(_get_truncate_limit "text")
  assert_eq "120" "$limit" "_get_truncate_limit returns 120 for text in normal mode"

  # Test verbose mode (2)
  RALPH_VERBOSITY=2
  limit=$(_get_truncate_limit "text")
  assert_eq "500" "$limit" "_get_truncate_limit returns 500 for text in verbose mode"

  # Test full mode (3)
  RALPH_VERBOSITY=3
  limit=$(_get_truncate_limit "text")
  assert_eq "999999" "$limit" "_get_truncate_limit returns 999999 for text in full mode"

  # Reset to default
  RALPH_VERBOSITY=1
}

test_truncate_text_function() {
  local short_text="Hello world"
  local long_text="This is a very long text that should be truncated when the limit is small"

  # Test no truncation when under limit
  local result=$(_truncate_text "$short_text" 50)
  assert_eq "$short_text" "$result" "_truncate_text preserves short text"

  # Test truncation when over limit
  result=$(_truncate_text "$long_text" 20)
  assert_eq "This is a very long ..." "$result" "_truncate_text truncates with ellipsis"
}

test_verbosity_affects_claude_output() {
  # Test that verbosity setting affects parse output
  # Use a sample Claude text event
  local json='{"type":"assistant","message":{"content":[{"type":"text","text":"This is a test message that will be truncated depending on verbosity settings"}]}}'

  # Save and unset STREAM_LOGFILE to prevent "no such file" errors
  # when running tests outside the normal context (P1 test hermeticity fix)
  local saved_logfile="${STREAM_LOGFILE:-}"
  unset STREAM_LOGFILE

  RALPH_VERBOSITY=0  # quiet mode - 60 char limit
  local output=$(print -r -- "$json" | stream_parse_claude)
  # In quiet mode, text should be shorter
  ((TESTS_RUN++))
  if [[ ${#output} -lt 100 ]]; then
    test_pass "Claude output truncated in quiet mode"
  else
    test_fail "Claude output truncated in quiet mode" "Expected shorter output"
  fi

  RALPH_VERBOSITY=3  # full mode - no truncation
  output=$(print -r -- "$json" | stream_parse_claude)
  # In full mode, should contain full text
  assert_contains "$output" "verbosity settings" "Claude output complete in full mode"

  # Reset to default
  RALPH_VERBOSITY=1
  # Restore STREAM_LOGFILE if it was set
  [[ -n "$saved_logfile" ]] && STREAM_LOGFILE="$saved_logfile"
}

############################################
# Prompts Module Tests
############################################

test_prompt_claude_iteration() {
  local result=$(prompt_claude_iteration 2 5)
  # Note: We intentionally do NOT include iteration count in the prompt
  # to avoid context pollution - agents should not know loop structure
  # Also, no "stop" instruction - agent works until all goals satisfied
  assert_contains "$result" "PROMPT.md" "prompt_claude_iteration references PROMPT.md"
  assert_contains "$result" "STATUS.md" "prompt_claude_iteration references STATUS.md"
  assert_contains "$result" "until all are satisfied" "prompt_claude_iteration works until complete"
}

test_prompt_codex_review_fresh_eyes() {
  RALPH_REVIEWER_CONTEXT=0
  local result=$(prompt_codex_review)
  assert_contains "$result" "fresh eyes" "prompt_codex_review mentions fresh eyes in minimal context"
  assert_contains "$result" "DO NOT read" "prompt_codex_review warns not to read status in minimal context"
  RALPH_REVIEWER_CONTEXT=1  # reset
}

test_prompt_codex_review_normal() {
  RALPH_REVIEWER_CONTEXT=1
  local result=$(prompt_codex_review)
  assert_contains "$result" "PROMPT.md" "prompt_codex_review references PROMPT.md in normal context"
  # In normal mode, should NOT contain fresh eyes language
  ((TESTS_RUN++))
  if [[ "$result" != *"fresh eyes"* ]]; then
    test_pass "prompt_codex_review omits fresh eyes in normal context"
  else
    test_fail "prompt_codex_review omits fresh eyes in normal context"
  fi
  RALPH_REVIEWER_CONTEXT=0  # reset to default
}

test_prompt_codex_fix() {
  local result=$(prompt_codex_fix)
  assert_contains "$result" "ISSUES.md" "prompt_codex_fix references ISSUES.md"
  assert_contains "$result" "NOTES.md" "prompt_codex_fix references NOTES.md"
}

test_prompt_codex_review_again_fresh_eyes() {
  RALPH_REVIEWER_CONTEXT=0
  local result=$(prompt_codex_review_again)
  assert_contains "$result" "fresh eyes" "prompt_codex_review_again mentions fresh eyes"
  assert_contains "$result" "DO NOT assume" "prompt_codex_review_again warns not to assume"
  RALPH_REVIEWER_CONTEXT=1  # reset
}

test_prompt_commit() {
  local result=$(prompt_commit "feat: test commit")
  assert_contains "$result" "git add -A" "prompt_commit includes git add"
  assert_contains "$result" "git commit" "prompt_commit includes git commit"
  assert_contains "$result" "feat: test commit" "prompt_commit includes commit message"
}

test_prompt_for_agent_developer() {
  local result=$(prompt_for_agent developer iterate 3 10)
  # Note: Iteration count is intentionally not in prompt to avoid context pollution
  assert_contains "$result" "PROMPT.md" "prompt_for_agent developer:iterate works"
}

test_prompt_for_agent_reviewer() {
  RALPH_REVIEWER_CONTEXT=0
  local result=$(prompt_for_agent reviewer review)
  assert_contains "$result" "fresh eyes" "prompt_for_agent reviewer:review works"
  RALPH_REVIEWER_CONTEXT=1  # reset
}

############################################
# Reviewer Commit Tests
############################################

test_allow_reviewer_commit() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  # Start agent phase (creates .no_agent_commit)
  start_agent_phase

  ((TESTS_RUN++))
  if [[ -f ".no_agent_commit" ]]; then
    test_pass "start_agent_phase creates .no_agent_commit marker"
  else
    test_fail "start_agent_phase creates .no_agent_commit marker"
  fi

  # Allow reviewer to commit
  allow_reviewer_commit

  ((TESTS_RUN++))
  if [[ ! -f ".no_agent_commit" ]]; then
    test_pass "allow_reviewer_commit removes .no_agent_commit"
  else
    test_fail "allow_reviewer_commit removes .no_agent_commit"
  fi

  # Cleanup
  disable_git_wrapper
  cd /
  rm -rf "$tmpdir"
}

test_block_commits_again() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  # Start with no block
  rm -f .no_agent_commit

  # Block commits again
  block_commits_again

  ((TESTS_RUN++))
  if [[ -f ".no_agent_commit" ]]; then
    test_pass "block_commits_again creates .no_agent_commit"
  else
    test_fail "block_commits_again creates .no_agent_commit"
  fi

  # Cleanup
  disable_git_wrapper
  rm -f .no_agent_commit
  cd /
  rm -rf "$tmpdir"
}

test_reviewer_commit_workflow() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q
  git config user.email "test@test.com"
  git config user.name "Test"

  # Simulate reviewer commit workflow
  start_agent_phase

  # Create a file (simulating work done by reviewer)
  echo "test content" > testfile.txt

  # Allow reviewer to commit
  allow_reviewer_commit

  # Verify git commit is now possible (no wrapper blocking)
  git add testfile.txt
  ((TESTS_RUN++))
  if git commit -m "test: reviewer commit" 2>/dev/null; then
    test_pass "reviewer can commit after allow_reviewer_commit"
  else
    test_fail "reviewer can commit after allow_reviewer_commit"
  fi

  # Verify commit was created
  local last_msg=$(git log -1 --pretty=%s 2>/dev/null)
  assert_eq "test: reviewer commit" "$last_msg" "reviewer commit message is correct"

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

############################################
# Context cleanup tests
############################################

test_archive_context_file() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  mkdir -p .agent

  # Create a context file
  echo "test content" > .agent/STATUS.md

  # Archive it
  archive_context_file ".agent/STATUS.md"

  # Check archive was created
  ((TESTS_RUN++))
  if [[ -d ".agent/archive" ]]; then
    test_pass "archive_context_file creates archive directory"
  else
    test_fail "archive_context_file creates archive directory"
  fi

  ((TESTS_RUN++))
  local archive_count=$(ls .agent/archive/ 2>/dev/null | wc -l)
  if [[ "$archive_count" -gt 0 ]]; then
    test_pass "archive_context_file creates archive file"
  else
    test_fail "archive_context_file creates archive file"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_clear_context_file() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  mkdir -p .agent

  # Create a context file with content
  echo "test content" > .agent/NOTES.md

  # Clear it
  clear_context_file ".agent/NOTES.md"

  # Check file exists but is empty
  ((TESTS_RUN++))
  if [[ -f ".agent/NOTES.md" ]]; then
    test_pass "clear_context_file preserves file"
  else
    test_fail "clear_context_file preserves file"
  fi

  ((TESTS_RUN++))
  local content=$(cat .agent/NOTES.md)
  if [[ -z "$content" ]]; then
    test_pass "clear_context_file empties file"
  else
    test_fail "clear_context_file empties file" "expected empty, got '$content'"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_clean_context_for_reviewer() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  mkdir -p .agent/logs

  # Create context files with content
  echo "Developer notes here" > .agent/NOTES.md
  echo "Developer status here" > .agent/STATUS.md
  echo "Stale issues here" > .agent/ISSUES.md

  # Clean context for reviewer
  clean_context_for_reviewer >/dev/null 2>&1

  # Check NOTES.md is cleared
  ((TESTS_RUN++))
  local notes_content=$(cat .agent/NOTES.md)
  if [[ -z "$notes_content" ]]; then
    test_pass "clean_context_for_reviewer clears NOTES.md"
  else
    test_fail "clean_context_for_reviewer clears NOTES.md" "expected empty, got '$notes_content'"
  fi

  # Check STATUS.md is reset (not empty, but reset to non-revealing content)
  ((TESTS_RUN++))
  local status_content=$(cat .agent/STATUS.md)
  if [[ "$status_content" == *"Code changes made"* ]]; then
    test_pass "clean_context_for_reviewer resets STATUS.md"
  else
    test_fail "clean_context_for_reviewer resets STATUS.md"
  fi

  # Check ISSUES.md is cleared (reviewer should find fresh issues)
  ((TESTS_RUN++))
  local issues_content=$(cat .agent/ISSUES.md)
  if [[ -z "$issues_content" ]]; then
    test_pass "clean_context_for_reviewer clears ISSUES.md"
  else
    test_fail "clean_context_for_reviewer clears ISSUES.md" "expected empty, got '$issues_content'"
  fi

  # Check archive was created
  ((TESTS_RUN++))
  if [[ -d ".agent/archive" ]]; then
    test_pass "clean_context_for_reviewer archives old files"
  else
    test_fail "clean_context_for_reviewer archives old files"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_reset_iteration_context() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  mkdir -p .agent

  # Reset iteration context
  reset_iteration_context 3 "Continue working"

  # Check STATUS.md was created with correct content
  ((TESTS_RUN++))
  local status_content=$(cat .agent/STATUS.md)
  if [[ "$status_content" == *"iteration 3"* ]]; then
    test_pass "reset_iteration_context sets iteration number"
  else
    test_fail "reset_iteration_context sets iteration number"
  fi

  ((TESTS_RUN++))
  if [[ "$status_content" == *"Continue working"* ]]; then
    test_pass "reset_iteration_context sets next action"
  else
    test_fail "reset_iteration_context sets next action"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

############################################
# Run all tests
############################################
print ""
print "${T_BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${T_RESET}"
print "${T_BOLD}  ralph.zsh Unit Tests${T_RESET}"
print "${T_BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${T_RESET}"

run_test "Timestamp format" test_ts_format
run_test "Timer initialization" test_timer_start
run_test "Timer elapsed format" test_timer_elapsed_format
run_test "Timer phase elapsed" test_timer_phase_elapsed
run_test "Timer zero elapsed" test_timer_zero_elapsed
run_test "Progress bar 50%" test_print_progress_50_percent
run_test "Progress bar 100%" test_print_progress_100_percent
run_test "Progress bar 0%" test_print_progress_0_percent
run_test "Marker detection - found" test_file_contains_marker_found
run_test "Marker detection - not found" test_file_contains_marker_not_found
run_test "Marker detection - missing file" test_file_contains_marker_missing_file
run_test "Stream parse - complete lines" test_stream_parse_complete_lines
run_test "Stream parse - chunked data" test_stream_parse_chunked_data
run_test "Stream parse - no trailing newline" test_stream_parse_no_trailing_newline
run_test "Stream parse - empty lines" test_stream_parse_empty_lines
run_test "Detect agent type - claude" test_detect_agent_type_claude
run_test "Detect agent type - codex" test_detect_agent_type_codex
run_test "Detect agent type - unknown" test_detect_agent_type_unknown
run_test "Git snapshot" test_git_snapshot
run_test "Require git repo - inside repo" test_require_git_repo_in_repo
run_test "Require git repo - outside repo" test_require_git_repo_outside_repo
run_test "Install hook - creates file" test_install_hook_creates_file
run_test "Start/end agent phase" test_start_end_agent_phase
run_test "Verbosity truncate limits" test_verbosity_truncate_limits
run_test "Truncate text function" test_truncate_text_function
run_test "Verbosity affects Claude output" test_verbosity_affects_claude_output
run_test "Prompt - Claude iteration" test_prompt_claude_iteration
run_test "Prompt - Codex review fresh eyes" test_prompt_codex_review_fresh_eyes
run_test "Prompt - Codex review normal" test_prompt_codex_review_normal
run_test "Prompt - Codex fix" test_prompt_codex_fix
run_test "Prompt - Codex re-review fresh eyes" test_prompt_codex_review_again_fresh_eyes
run_test "Prompt - Commit message" test_prompt_commit
run_test "Prompt - Generic developer prompt" test_prompt_for_agent_developer
run_test "Prompt - Generic reviewer prompt" test_prompt_for_agent_reviewer
run_test "Reviewer commit - allow_reviewer_commit" test_allow_reviewer_commit
run_test "Reviewer commit - block_commits_again" test_block_commits_again
run_test "Reviewer commit - workflow" test_reviewer_commit_workflow
run_test "Context - archive_context_file" test_archive_context_file
run_test "Context - clear_context_file" test_clear_context_file
run_test "Context - clean_context_for_reviewer" test_clean_context_for_reviewer
run_test "Context - reset_iteration_context" test_reset_iteration_context

############################################
# Agent abstraction tests
############################################

test_agent_get_cmd_claude() {
  local result="$(agent_get_cmd claude)"
  assert_contains "$result" "claude" "agent_get_cmd claude returns claude command"
}

test_agent_get_cmd_codex() {
  local result="$(agent_get_cmd codex)"
  assert_contains "$result" "codex" "agent_get_cmd codex returns codex command"
}

test_agent_get_json_flag() {
  local claude_flag="$(agent_get_json_flag claude)"
  local codex_flag="$(agent_get_json_flag codex)"
  assert_contains "$claude_flag" "json" "claude JSON flag contains 'json'"
  assert_contains "$codex_flag" "json" "codex JSON flag contains 'json'"
}

test_agent_get_parser() {
  local claude_parser="$(agent_get_parser claude)"
  local codex_parser="$(agent_get_parser codex)"
  assert_eq "stream_parse_claude" "$claude_parser" "claude parser is stream_parse_claude"
  assert_eq "stream_parse_codex" "$codex_parser" "codex parser is stream_parse_codex"
}

test_agent_can_commit() {
  agent_can_commit claude && local claude_can=1 || local claude_can=0
  agent_can_commit codex && local codex_can=1 || local codex_can=0
  assert_eq "1" "$claude_can" "claude can commit"
  assert_eq "1" "$codex_can" "codex can commit"
}

test_agent_is_known() {
  agent_is_known claude && local known_claude=1 || local known_claude=0
  agent_is_known codex && local known_codex=1 || local known_codex=0
  agent_is_known unknown_agent && local known_unknown=1 || local known_unknown=0
  assert_eq "1" "$known_claude" "claude is known agent"
  assert_eq "1" "$known_codex" "codex is known agent"
  assert_eq "0" "$known_unknown" "unknown_agent is not known"
}

test_agent_build_cmd() {
  local cmd="$(agent_build_cmd claude --json --yolo)"
  assert_contains "$cmd" "claude" "built command contains claude"
  assert_contains "$cmd" "json" "built command contains json flag"
  assert_contains "$cmd" "skip-permissions" "built command contains yolo flag"
}

test_agent_developer_cmd() {
  RALPH_DEVELOPER_AGENT=claude
  local cmd="$(agent_developer_cmd)"
  assert_contains "$cmd" "claude" "developer cmd uses claude"
  assert_contains "$cmd" "json" "developer cmd has json flag"
}

test_agent_reviewer_cmd() {
  RALPH_REVIEWER_AGENT=codex
  local cmd="$(agent_reviewer_cmd)"
  assert_contains "$cmd" "codex" "reviewer cmd uses codex"
  assert_contains "$cmd" "json" "reviewer cmd has json flag"
}

test_detect_agent_from_cmd() {
  local detected_claude="$(detect_agent_from_cmd 'claude -p --json')"
  local detected_codex="$(detect_agent_from_cmd 'codex exec --json')"
  local detected_unknown="$(detect_agent_from_cmd 'some_other_cmd')"
  assert_eq "claude" "$detected_claude" "detects claude from command"
  assert_eq "codex" "$detected_codex" "detects codex from command"
  assert_eq "unknown" "$detected_unknown" "returns unknown for unrecognized"
}

test_register_agent() {
  register_agent "testbot" "testbot run" "--output-json" "stream_parse_generic" "1" "--auto"
  local cmd="$(agent_get_cmd testbot)"
  assert_eq "testbot run" "$cmd" "registered agent has correct command"
  agent_is_known testbot && local known=1 || local known=0
  assert_eq "1" "$known" "registered agent is known"
}

run_test "Agent - get_cmd claude" test_agent_get_cmd_claude
run_test "Agent - get_cmd codex" test_agent_get_cmd_codex
run_test "Agent - get_json_flag" test_agent_get_json_flag
run_test "Agent - get_parser" test_agent_get_parser
run_test "Agent - can_commit" test_agent_can_commit
run_test "Agent - is_known" test_agent_is_known
run_test "Agent - build_cmd" test_agent_build_cmd
run_test "Agent - developer_cmd" test_agent_developer_cmd
run_test "Agent - reviewer_cmd" test_agent_reviewer_cmd
run_test "Agent - detect_from_cmd" test_detect_agent_from_cmd
run_test "Agent - register_agent" test_register_agent

############################################
# Integration tests for P0/P1/P2 issues
############################################

test_print_progress_zero_total() {
  # P1 fix: print_progress should not crash with total=0
  local result="$(print_progress 0 0 "Test")"
  ((TESTS_RUN++))
  if [[ -n "$result" ]]; then
    test_pass "print_progress handles total=0 gracefully"
  else
    test_fail "print_progress handles total=0 gracefully"
  fi
  assert_contains "$result" "no progress data" "print_progress shows 'no progress data' for zero total"
}

test_hook_uses_absolute_path() {
  # P2 fix: Hook should use absolute path for orig backup
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  local hooks_dir="${tmpdir}/.git/hooks"
  mkdir -p "$hooks_dir"

  # Create an existing hook to trigger backup
  echo "#!/bin/bash" > "${hooks_dir}/pre-commit"
  echo "exit 0" >> "${hooks_dir}/pre-commit"
  chmod +x "${hooks_dir}/pre-commit"

  install_hook "TestHook" "${hooks_dir}/pre-commit"

  # Read the installed hook content
  local hook_content="$(cat ${hooks_dir}/pre-commit)"

  ((TESTS_RUN++))
  # The orig= line should contain an absolute path (starts with /)
  if [[ "$hook_content" == *'orig="/'* ]]; then
    test_pass "install_hook uses absolute path for orig backup"
  else
    test_fail "install_hook uses absolute path for orig backup" "Expected absolute path in hook"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_hook_works_from_subdirectory() {
  # P2 fix: Hook should work when committing from a subdirectory
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q
  git config user.email "test@test.com"
  git config user.name "Test"

  # Create an existing hook that we'll backup
  local hooks_dir="${tmpdir}/.git/hooks"
  mkdir -p "$hooks_dir"
  echo '#!/bin/bash' > "${hooks_dir}/pre-commit"
  echo 'echo "Original hook ran"' >> "${hooks_dir}/pre-commit"
  chmod +x "${hooks_dir}/pre-commit"

  # Install Ralph hook (which should backup the original with absolute path)
  install_hook "Commit" "${hooks_dir}/pre-commit"

  # Create a subdirectory and a file
  mkdir -p subdir
  echo "test" > subdir/file.txt
  git add subdir/file.txt

  # Commit from the root (should work - no .no_agent_commit)
  ((TESTS_RUN++))
  if git commit -m "test commit" 2>/dev/null; then
    test_pass "hook allows commit when .no_agent_commit absent"
  else
    test_fail "hook allows commit when .no_agent_commit absent"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

run_test "Progress bar - zero total" test_print_progress_zero_total
run_test "Hook - uses absolute path" test_hook_uses_absolute_path
run_test "Hook - works from subdirectory" test_hook_works_from_subdirectory

############################################
# Hook uninstall tests
############################################

test_uninstall_hook_restores_original() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  local hooks_dir="${tmpdir}/.git/hooks"
  mkdir -p "$hooks_dir"

  # Create an existing hook
  echo '#!/bin/bash' > "${hooks_dir}/pre-commit"
  echo 'echo "Original hook"' >> "${hooks_dir}/pre-commit"
  chmod +x "${hooks_dir}/pre-commit"

  # Install Ralph hook (backs up original)
  install_hook "Commit" "${hooks_dir}/pre-commit"

  # Verify Ralph hook is installed
  ((TESTS_RUN++))
  if file_contains_marker "${hooks_dir}/pre-commit" "$HOOK_MARKER"; then
    test_pass "Ralph hook installed"
  else
    test_fail "Ralph hook installed"
  fi

  # Uninstall hook
  uninstall_hook "${hooks_dir}/pre-commit" >/dev/null 2>&1

  # Verify original is restored
  ((TESTS_RUN++))
  if [[ -f "${hooks_dir}/pre-commit" ]]; then
    local content=$(cat "${hooks_dir}/pre-commit")
    if [[ "$content" == *"Original hook"* ]]; then
      test_pass "uninstall_hook restores original"
    else
      test_fail "uninstall_hook restores original" "content doesn't match original"
    fi
  else
    test_fail "uninstall_hook restores original" "hook file missing"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_uninstall_hook_removes_when_no_original() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  local hooks_dir="${tmpdir}/.git/hooks"
  mkdir -p "$hooks_dir"

  # Install Ralph hook (no original)
  install_hook "Commit" "${hooks_dir}/pre-commit"

  # Uninstall hook
  uninstall_hook "${hooks_dir}/pre-commit" >/dev/null 2>&1

  # Verify hook is removed
  ((TESTS_RUN++))
  if [[ ! -f "${hooks_dir}/pre-commit" ]]; then
    test_pass "uninstall_hook removes hook when no original"
  else
    test_fail "uninstall_hook removes hook when no original"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_cleanup_orphaned_marker() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  # Create orphaned marker
  touch .no_agent_commit

  # Clean up
  cleanup_orphaned_marker >/dev/null 2>&1

  # Verify marker is removed
  ((TESTS_RUN++))
  if [[ ! -f ".no_agent_commit" ]]; then
    test_pass "cleanup_orphaned_marker removes marker"
  else
    test_fail "cleanup_orphaned_marker removes marker"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_cleanup_orphaned_marker_when_missing() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q

  # Ensure no marker exists
  rm -f .no_agent_commit

  # Clean up (should succeed even when no marker)
  ((TESTS_RUN++))
  if cleanup_orphaned_marker >/dev/null 2>&1; then
    test_pass "cleanup_orphaned_marker succeeds when no marker"
  else
    test_fail "cleanup_orphaned_marker succeeds when no marker"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

run_test "Hook - uninstall restores original" test_uninstall_hook_restores_original
run_test "Hook - uninstall removes when no original" test_uninstall_hook_removes_when_no_original
run_test "Cleanup - removes orphaned marker" test_cleanup_orphaned_marker
run_test "Cleanup - succeeds when no marker" test_cleanup_orphaned_marker_when_missing

############################################
# Commit verification tests
############################################

test_commit_verification_detects_new_commit() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q
  git config user.email "test@test.com"
  git config user.name "Test"

  # Create initial commit
  echo "initial" > file.txt
  git add file.txt
  git commit -m "initial" -q

  # Capture HEAD before
  local head_before="$(git rev-parse HEAD)"

  # Make a new commit
  echo "change" >> file.txt
  git add file.txt
  git commit -m "second commit" -q

  # Capture HEAD after
  local head_after="$(git rev-parse HEAD)"

  # Verify detection
  ((TESTS_RUN++))
  if [[ "$head_before" != "$head_after" ]]; then
    test_pass "commit verification detects new commit (HEAD changed)"
  else
    test_fail "commit verification detects new commit (HEAD changed)"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

test_commit_verification_detects_no_commit() {
  local tmpdir="$(mktemp -d)"
  cd "$tmpdir"
  git init -q
  git config user.email "test@test.com"
  git config user.name "Test"

  # Create initial commit
  echo "initial" > file.txt
  git add file.txt
  git commit -m "initial" -q

  # Capture HEAD before (no new commit)
  local head_before="$(git rev-parse HEAD)"
  local head_after="$(git rev-parse HEAD)"

  # Verify no change detected
  ((TESTS_RUN++))
  if [[ "$head_before" == "$head_after" ]]; then
    test_pass "commit verification detects no new commit (HEAD unchanged)"
  else
    test_fail "commit verification detects no new commit (HEAD unchanged)"
  fi

  # Cleanup
  cd /
  rm -rf "$tmpdir"
}

run_test "Commit verification - detects new commit" test_commit_verification_detects_new_commit
run_test "Commit verification - detects no commit" test_commit_verification_detects_no_commit

############################################
# Summary
############################################
print ""
print "${T_BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${T_RESET}"
print "${T_BOLD}  Results${T_RESET}"
print "${T_BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${T_RESET}"
print ""
print "  Total:  $TESTS_RUN"
print "  ${T_GREEN}Passed: $TESTS_PASSED${T_RESET}"
print "  ${T_RED}Failed: $TESTS_FAILED${T_RESET}"
print ""

if [[ $TESTS_FAILED -gt 0 ]]; then
  print "${T_RED}${T_BOLD}TESTS FAILED${T_RESET}"
  exit 1
else
  print "${T_GREEN}${T_BOLD}ALL TESTS PASSED${T_RESET}"
  exit 0
fi
