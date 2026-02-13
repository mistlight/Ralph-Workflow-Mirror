#!/usr/bin/env bash
# CI performance regression verification
#
# This script runs performance-critical tests and verifies metrics
# remain within acceptable bounds. Fails CI if regression is detected.
#
# Thresholds are based on current measurements with 20% headroom.

set -euo pipefail

VERBOSE=0
if [[ "${1:-}" == "--verbose" ]]; then
  VERBOSE=1
  shift
fi

log() {
  if [[ "$VERBOSE" -eq 1 ]]; then
    printf '%s\n' "$*"
  fi
}

run() {
  local name="$1"
  shift

  local output
  if output=$("$@" 2>&1); then
    log "✓ ${name}"
    return 0
  fi

  printf '%s\n' "✗ ${name}" >&2
  if [[ "$VERBOSE" -eq 1 ]]; then
    printf '%s\n' "$output" >&2
  fi
  return 1
}

FAILED=0

# Test 1: Execution history bounded growth
run "Execution history bounded growth" \
  cargo test -p ralph-workflow-tests --test integration_tests \
    memory_safety::long_running_pipeline::test_10k_iterations_memory_remains_bounded \
    --quiet -- --nocapture \
  || FAILED=1

# Test 2: Checkpoint size remains reasonable
CHECKPOINT_OUTPUT=""
if CHECKPOINT_OUTPUT=$(cargo test -p ralph-workflow-tests --test integration_tests \
  memory_safety::long_running_pipeline::test_checkpoint_size_remains_reasonable_with_max_history \
  --quiet -- --nocapture 2>&1); then
  if [[ "$VERBOSE" -eq 1 ]]; then
    SIZE=$(printf '%s\n' "$CHECKPOINT_OUTPUT" | grep "Checkpoint size" | awk '{print $7}' || true)
    if [[ -n "$SIZE" ]]; then
      log "✓ Checkpoint size: ${SIZE} KB (threshold: 2048 KB)"
    else
      log "✓ Checkpoint size verified"
    fi
  else
    true
  fi
else
  printf '%s\n' "✗ Checkpoint size within limits" >&2
  if [[ "$VERBOSE" -eq 1 ]]; then
    printf '%s\n' "$CHECKPOINT_OUTPUT" >&2
  fi
  FAILED=1
fi

# Test 3: Memory benchmarks (informational only)
if [[ "$VERBOSE" -eq 1 ]]; then
  run "Memory usage benchmarks" \
    cargo test -p ralph-workflow --lib benchmarks::memory_usage --quiet -- --nocapture \
    || FAILED=1
else
  run "Memory usage benchmarks" \
    cargo test -p ralph-workflow --lib benchmarks::memory_usage --quiet \
    || FAILED=1
fi

# Test 4: Thread cleanup verification
run "Thread lifecycle" \
  cargo test -p ralph-workflow-tests --test integration_tests memory_safety::thread_lifecycle --quiet \
  || FAILED=1

if [[ "$FAILED" -eq 0 ]]; then
  exit 0
fi

exit 1
