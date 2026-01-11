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
