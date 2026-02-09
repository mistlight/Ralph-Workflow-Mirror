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
    for file in $parser_files; do
        # Check file itself or parent mod.rs
        dir=$(dirname "$file")
        if ! grep -q "TestPrinter\|VirtualTerminal" "$file" && ! grep -q "TestPrinter\|VirtualTerminal" "$dir/mod.rs" 2>/dev/null; then
            echo "WARNING: $file may not use TestPrinter or VirtualTerminal"
        fi
    done
else
    echo "No parser test files found"
fi

echo -e "\n=== Audit complete ==="
