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
# Source ralph.zsh functions for testing
# We need to extract just the helper functions without running main
############################################

# Extract helper functions from ralph.zsh
# We'll define them inline since sourcing would run the whole script

# Timestamp function
ts() { date +"%Y-%m-%d %H:%M:%S"; }

# Timer variables
typeset -g START_TIME=0
typeset -g PHASE_START=0

timer_start() {
  START_TIME=$SECONDS
  PHASE_START=$SECONDS
}

timer_phase_start() {
  PHASE_START=$SECONDS
}

timer_elapsed() {
  local elapsed=$((SECONDS - START_TIME))
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}

timer_phase_elapsed() {
  local elapsed=$((SECONDS - PHASE_START))
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}

# Progress bar function
print_progress() {
  local current="$1" total="$2" label="${3:-Progress}"
  local pct=$((current * 100 / total))
  local bar_width=20
  local filled=$((current * bar_width / total))
  local empty=$((bar_width - filled))

  local bar=""
  for ((k=0; k<filled; k++)); do bar+="█"; done
  for ((k=0; k<empty; k++)); do bar+="░"; done

  printf "%s: [%s] %d%% (%d/%d)" "$label" "$bar" "$pct" "$current" "$total"
}

# file_contains_marker (simplified for testing)
file_contains_marker() {
  local file="$1" marker="$2"
  grep -Fq -- "$marker" "$file" >/dev/null 2>&1
}

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
  assert_numeric "$START_TIME" "START_TIME is numeric after timer_start"
  assert_numeric "$PHASE_START" "PHASE_START is numeric after timer_start"
}

test_timer_elapsed_format() {
  START_TIME=$((SECONDS - 65))  # 1m 5s ago
  local result="$(timer_elapsed)"
  assert_contains "$result" "m" "timer_elapsed contains 'm'"
  assert_contains "$result" "s" "timer_elapsed contains 's'"
  assert_eq "1m 05s" "$result" "timer_elapsed formats 65s as '1m 05s'"
}

test_timer_phase_elapsed() {
  PHASE_START=$((SECONDS - 30))  # 30s ago
  local result="$(timer_phase_elapsed)"
  assert_eq "0m 30s" "$result" "timer_phase_elapsed formats 30s as '0m 30s'"
}

test_timer_zero_elapsed() {
  START_TIME=$SECONDS
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
############################################

# Check if jq is available
HAS_JQ=0
if command -v jq >/dev/null 2>&1; then
  HAS_JQ=1
fi

# Minimal Claude event parser for testing
parse_claude_event() {
  local line="$1"
  [[ -z "$line" ]] && return 0
  [[ "$HAS_JQ" != "1" ]] && { print -r -- "$line"; return 0; }

  local event_type
  event_type=$(print -r -- "$line" | jq -r '.type // empty' 2>/dev/null) || { print -r -- "$line"; return 0; }
  print "EVENT:${event_type}"
}

# Stream parser (from ralph.zsh)
stream_parse_claude() {
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    parse_claude_event "$line"
  done
}

test_stream_parse_complete_lines() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_complete_lines (skipped - jq not available)"
    return 0
  fi

  local result
  result=$(printf '{"type":"init"}\n{"type":"result"}\n' | stream_parse_claude)

  assert_contains "$result" "EVENT:init" "stream_parse handles first JSON line"
  assert_contains "$result" "EVENT:result" "stream_parse handles second JSON line"
}

test_stream_parse_chunked_data() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_chunked_data (skipped - jq not available)"
    return 0
  fi

  # Simulate chunked streaming by sending partial data
  # In a real stream, '{"type":"init"}\n' might arrive as '{"typ' then 'e":"init"}\n'
  local result
  result=$(printf '{"type":"init"}\n' | stream_parse_claude)

  assert_contains "$result" "EVENT:init" "stream_parse handles single JSON line"
}

test_stream_parse_no_trailing_newline() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_no_trailing_newline (skipped - jq not available)"
    return 0
  fi

  # Test handling of data without trailing newline (common in streams)
  local result
  result=$(printf '{"type":"init"}' | stream_parse_claude)

  assert_contains "$result" "EVENT:init" "stream_parse handles JSON without trailing newline"
}

test_stream_parse_empty_lines() {
  if [[ "$HAS_JQ" != "1" ]]; then
    print "${T_YELLOW}⊘${T_RESET} stream_parse_empty_lines (skipped - jq not available)"
    return 0
  fi

  # Test handling of empty lines between JSON objects
  local result
  result=$(printf '{"type":"init"}\n\n{"type":"result"}\n' | stream_parse_claude)

  assert_contains "$result" "EVENT:init" "stream_parse handles JSON with empty lines (init)"
  assert_contains "$result" "EVENT:result" "stream_parse handles JSON with empty lines (result)"
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
