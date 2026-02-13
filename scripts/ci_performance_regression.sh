#!/usr/bin/env bash
# CI performance regression verification
#
# This script runs performance-critical tests and verifies metrics
# remain within acceptable bounds. Fails CI if regression is detected.
#
# Thresholds are based on current measurements with 20% headroom.

set -euo pipefail

echo "=== CI Performance Regression Verification ==="
echo ""

FAILED=0

# Test 1: Execution history bounded growth
echo "→ Testing execution history remains bounded..."
OUTPUT=$(cargo test -p ralph-workflow-tests --test integration_tests \
    memory_safety::long_running_pipeline::test_10k_iterations_memory_remains_bounded \
    --quiet -- --nocapture 2>&1)

if echo "$OUTPUT" | grep -q "FAILED"; then
    echo "✗ Bounded growth test FAILED"
    FAILED=1
else
    echo "✓ Bounded growth verified"
fi
echo ""

# Test 2: Checkpoint size remains reasonable
echo "→ Testing checkpoint serialization size..."
OUTPUT=$(cargo test -p ralph-workflow-tests --test integration_tests \
    memory_safety::long_running_pipeline::test_checkpoint_size_remains_reasonable_with_max_history \
    --quiet -- --nocapture 2>&1)

if echo "$OUTPUT" | grep -q "FAILED"; then
    echo "✗ Checkpoint size test FAILED"
    FAILED=1
else
    # Extract size from output
    SIZE=$(echo "$OUTPUT" | grep "Checkpoint size" | awk '{print $7}' || echo "N/A")
    echo "✓ Checkpoint size: ${SIZE} KB (threshold: 2048 KB)"
fi
echo ""

# Test 3: Memory benchmarks (informational - capture for trending)
echo "→ Running memory usage benchmarks..."
cargo test -p ralph-workflow --lib benchmarks::memory_usage --quiet -- --nocapture 2>&1 | \
    grep -E "(Execution History Growth|Total entries|Growth per iteration)" || true
echo ""

# Test 4: Thread cleanup verification
echo "→ Verifying thread cleanup..."
OUTPUT=$(cargo test -p ralph-workflow-tests --test integration_tests \
    memory_safety::thread_lifecycle --quiet 2>&1)

if echo "$OUTPUT" | grep -q "FAILED"; then
    echo "✗ Thread lifecycle tests FAILED"
    FAILED=1
else
    echo "✓ Thread cleanup verified"
fi
echo ""

# Summary
if [ $FAILED -eq 0 ]; then
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "✓ Performance regression verification PASSED"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 0
else
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "✗ Performance regression detected - FAILED"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 1
fi
