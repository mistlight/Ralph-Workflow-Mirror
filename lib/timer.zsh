#!/usr/bin/env zsh
############################################
# Timer Utilities Module
#
# Provides timing functions for tracking execution duration.
#
# Usage:
#   source lib/timer.zsh
#   timer_start
#   # ... do work ...
#   print "Elapsed: $(timer_elapsed)"
############################################

# Guard against multiple sourcing
[[ -n "${_RALPH_TIMER_LOADED:-}" ]] && return 0
typeset -g _RALPH_TIMER_LOADED=1

# Timer state variables
typeset -g RALPH_START_TIME=0
typeset -g RALPH_PHASE_START=0

# Start the main timer
timer_start() {
  RALPH_START_TIME=$SECONDS
  RALPH_PHASE_START=$SECONDS
}

# Start a phase timer (for tracking sub-operations)
timer_phase_start() {
  RALPH_PHASE_START=$SECONDS
}

# Get elapsed time since timer_start in "Xm YYs" format
timer_elapsed() {
  local elapsed=$((SECONDS - RALPH_START_TIME))
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}

# Get elapsed time since timer_phase_start in "Xm YYs" format
timer_phase_elapsed() {
  local elapsed=$((SECONDS - RALPH_PHASE_START))
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}

# Format a duration in seconds to "Xm YYs" format
format_duration() {
  local elapsed="${1:-0}"
  local mins=$((elapsed / 60))
  local secs=$((elapsed % 60))
  printf "%dm %02ds" "$mins" "$secs"
}
