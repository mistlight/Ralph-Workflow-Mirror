#!/usr/bin/env bash
# Memory safety verification script
#
# This script runs all memory safety tests to verify:
# - Bounded memory growth in execution history
# - Thread lifecycle cleanup
# - Arc reference pattern correctness
# - Channel backpressure behavior
# - Unsafe code correctness
#
# All tests must pass with NO OUTPUT (warnings or failures).
#
# Usage:
#   bash scripts/verify_memory_safety.sh

set -euo pipefail

echo "Running memory safety verification..."
echo ""

# Track if any test fails
FAILED=0

# Run memory safety integration tests
echo "→ Memory safety integration tests..."
if ! cargo test --test '*' memory_safety --quiet 2>&1; then
    echo "✗ Memory safety integration tests FAILED"
    FAILED=1
else
    echo "✓ Memory safety integration tests passed"
fi
echo ""

# Run bounded growth tests specifically
echo "→ Bounded growth tests..."
if ! cargo test --test '*' bounded_growth --quiet 2>&1; then
    echo "✗ Bounded growth tests FAILED"
    FAILED=1
else
    echo "✓ Bounded growth tests passed"
fi
echo ""

# Run thread lifecycle tests
echo "→ Thread lifecycle tests..."
if ! cargo test --test '*' thread_lifecycle --quiet 2>&1; then
    echo "✗ Thread lifecycle tests FAILED"
    FAILED=1
else
    echo "✓ Thread lifecycle tests passed"
fi
echo ""

# Run Arc pattern tests
echo "→ Arc circular reference prevention tests..."
if ! cargo test --test '*' arc_patterns --quiet 2>&1; then
    echo "✗ Arc pattern tests FAILED"
    FAILED=1
else
    echo "✓ Arc pattern tests passed"
fi
echo ""

# Run channel bounds tests
echo "→ Channel bounds and backpressure tests..."
if ! cargo test --test '*' channel_bounds --quiet 2>&1; then
    echo "✗ Channel bounds tests FAILED"
    FAILED=1
else
    echo "✓ Channel bounds tests passed"
fi
echo ""

# Run benchmark tests (informational only - capture output)
echo "→ Benchmark tests (informational)..."
if ! cargo test --lib benchmarks --quiet 2>&1; then
    echo "✗ Benchmark tests FAILED"
    FAILED=1
else
    echo "✓ Benchmark tests passed"
fi
echo ""

# Run unsafe code safety tests
echo "→ Unsafe code behavioral verification..."
if ! cargo test --lib executor::tests::safety --quiet 2>&1; then
    echo "✗ Unsafe code safety tests FAILED"
    FAILED=1
else
    echo "✓ Unsafe code safety tests passed"
fi
echo ""

# Summary
if [ $FAILED -eq 0 ]; then
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "✓ All memory safety verification passed"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 0
else
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "✗ Memory safety verification FAILED"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 1
fi
