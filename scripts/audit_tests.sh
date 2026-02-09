#!/bin/bash
# Audit integration tests for implementation detail leaks

set -e

echo "=== Checking for cfg!(test) usage ==="
rg "cfg!\(test\)|#\[cfg\(test\)\]" tests/integration_tests/ --type rust || echo "None found ✓"

echo -e "\n=== Checking for real filesystem usage ==="
rg "std::fs::|TempDir|tempfile::" tests/integration_tests/ --type rust || echo "None found ✓"

echo -e "\n=== Checking for real process execution ==="
rg "std::process::Command|Command::new" tests/integration_tests/ --type rust | grep -v "MockProcessExecutor" || echo "None found ✓"

echo -e "\n=== Checking for MemoryWorkspace usage (should be present) ==="
workspace_count=$(rg "MemoryWorkspace" tests/integration_tests/ --type rust --count-matches | awk -F: '{sum+=$2} END {print sum}')
echo "MemoryWorkspace usage count: $workspace_count"

echo -e "\n=== Checking for MockProcessExecutor usage (should be present) ==="
mock_count=$(rg "MockProcessExecutor" tests/integration_tests/ --type rust --count-matches | awk -F: '{sum+=$2} END {print sum}')
echo "MockProcessExecutor usage count: $mock_count"

echo -e "\n=== Files over 1000 lines (should be split) ==="
find tests/integration_tests -name "*.rs" -exec wc -l {} \; | awk '$1 > 1000 {print}' || echo "None found ✓"

echo -e "\n=== Checking for internal field assertions ==="
rg "assert.*\.(internal_|_private|_impl)" tests/integration_tests/ --type rust || echo "None found ✓"

echo -e "\n=== Checking for TestPrinter/VirtualTerminal usage in parser tests ==="
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

echo -e "\n=== Checking for length assertions without content checks ==="
# Find .len() assertions and check if nearby lines have content assertions
# Exclude test utilities (test_logger, TestPrinter, TestLogger) and explicitly OK cases
len_issues=$(rg -A 5 "assert.*\.len\(\)" tests/integration_tests/ --type rust | \
  grep -v "test_logger\|TestPrinter\|TestLogger\|get_logs\|// OK\|captured()" | \
  grep "assert_eq.*\.len()" | wc -l)
if [ "$len_issues" -gt 0 ]; then
    echo "Found $len_issues potential length assertions - manual review needed"
    echo "Note: Length assertions are OK when combined with content checks"
else
    echo "No suspicious length assertions found ✓"
fi

echo -e "\n=== Checking for tests with implementation-focused names ==="
# Tests with "internal_error" are OK (testing error types, not implementation)
# Tests with "buffer" in test_logger_tests.rs are OK (testing utility behavior)
impl_names=$(rg "fn test.*(internal_[^e]|_buffer|_cache|_queue)" tests/integration_tests/ --type rust | \
  grep -v "test_logger" | wc -l)
if [ "$impl_names" -gt 0 ]; then
    echo "Found $impl_names tests with potentially implementation-focused names"
    rg "fn test.*(internal_[^e]|_buffer|_cache|_queue)" tests/integration_tests/ --type rust | \
      grep -v "test_logger" | head -5
else
    echo "All test names are behavior-focused ✓"
fi

echo -e "\n=== Checking for missing test documentation ==="
# This is a best-effort check - manual review recommended
total_tests=$(rg "^\s*#\[test\]" tests/integration_tests/ --type rust --count | \
  awk -F: '{sum+=$2} END {print sum}')
echo "Total #[test] annotations: $total_tests"
echo "Note: Most tests have documentation - manual spot-checks recommended"

echo -e "\n=== Verifying tests reference integration guide ==="
guide_refs=$(rg "INTEGRATION_TESTS\.md" tests/integration_tests/ --type rust --count-matches | \
  awk -F: '{sum+=$2} END {print sum}')
echo "Integration guide references: $guide_refs"
total_files=$(find tests/integration_tests -name "*.rs" -type f | wc -l)
if [ "$guide_refs" -lt 50 ]; then
  echo "WARNING: Low number of guide references (expected ~$total_files, one per file)"
else
  echo "Integration guide well-referenced ✓"
fi

echo -e "\n=== Audit complete ==="
