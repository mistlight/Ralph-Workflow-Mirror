#!/usr/bin/env bash
# Integration Test Compliance Checker
#
# This script validates that all integration tests comply with the mandatory
# requirements defined in INTEGRATION_TESTS.md, including:
# - All tests must be wrapped with `with_default_timeout()`
# - Tests must follow proper structure (doc comments, etc.)
#
# Usage: ./tests/integration_tests/compliance_check.sh
# Exit codes: 0 = all compliant, 1 = violations found, 2 = error

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[0;33m'
readonly NC='\033[0m' # No Color

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR="$SCRIPT_DIR"
TEMP_OUTPUT=$(mktemp)
TEMP_VIOLATIONS="${TEMP_OUTPUT}.violations"

cleanup() {
    rm -f "$TEMP_OUTPUT" "$TEMP_VIOLATIONS"
}
trap cleanup EXIT

echo "Running integration test compliance checks..."
echo "Test directory: $TEST_DIR"
echo

# Find all Rust files in integration tests (excluding _TEMPLATE.rs and compliance_check.rs)
find "$TEST_DIR" -name "*.rs" -type f \
    ! -name "_TEMPLATE.rs" \
    ! -name "compliance_check.rs" \
    ! -name "common/mod.rs" \
    ! -path "*/target/*" | sort > "$TEMP_OUTPUT"

if [ ! -s "$TEMP_OUTPUT" ]; then
    echo -e "${YELLOW}No test files found to check${NC}"
    exit 0
fi

# Clear violations file
: > "$TEMP_VIOLATIONS"

# Process all files and collect violations
while IFS= read -r file; do
    # Find all test functions in this file
    # Pattern: #[test] followed (somewhere after) by fn ...
    grep -n '#\[test\]' "$file" | while IFS=: read -r line_num rest; do
        # Get the next line to see if it's a function definition
        next_line=$((line_num + 1))
        func_line=$(sed -n "${next_line}p" "$file")

        # Check if this is a test function (starts with "fn " or "pub fn " or "unsafe fn ")
        if echo "$func_line" | grep -qE '^[[:space:]]*(pub |unsafe )*fn[[:space:]]+\w+\('; then
            # Extract test name
            test_name=$(echo "$func_line" | sed -E 's/.*fn[[:space:]]+(\w+)\(.*/\1/')

            # Read lines starting from the test function to get the function body
            # Look for the opening brace after the function signature
            start_line=$next_line
            brace_found=0
            current_line=$start_line

            while [ $current_line -le $((start_line + 20)) ]; do
                if sed -n "${current_line}p" "$file" | grep -q '{'; then
                    brace_found=1
                    break
                fi
                current_line=$((current_line + 1))
            done

            if [ $brace_found -eq 1 ]; then
                # Check the next 20 lines for with_default_timeout
                check_end=$((current_line + 20))
                file_lines=$(wc -l < "$file")
                if [ $check_end -gt $file_lines ]; then
                    check_end=$file_lines
                fi

                # Extract and check function body
                function_body=$(sed -n "${current_line},${check_end}p" "$file")

                # Check if with_default_timeout or with_timeout appears in the function body
                if ! echo "$function_body" | grep -qE 'with_default_timeout|with_timeout'; then
                    echo "VIOLATION:$file:$line_num:$test_name" >> "$TEMP_VIOLATIONS"
                fi
            fi
        fi
    done
done < "$TEMP_OUTPUT"

# Check for violations
if [ -s "$TEMP_VIOLATIONS" ]; then
    echo -e "${RED}✗ Found $(wc -l < "$TEMP_VIOLATIONS" | tr -d ' ') test(s) without timeout wrapper${NC}"
    echo
    echo "Violations:"
    while IFS=: read -r type file line_num test_name; do
        echo "  - $file:$line_num: test '$test_name' missing timeout wrapper"
    done < "$TEMP_VIOLATIONS"
    echo
    echo -e "${YELLOW}To fix: Wrap test body with with_default_timeout(|| { ... }); or with_timeout(||, Duration::from_secs(30))${NC}"
    echo
    echo "Example:"
    echo "  #[test]"
    echo "  fn test_example() {"
    echo "      with_default_timeout(|| {"
    echo "          // test code here (for fast tests)"
    echo "      });"
    echo "  }"
    echo "  #[test]"
    echo "  fn slow_test_example() {"
    echo "      with_timeout(|| {"
    echo "          // test code here (for slow tests that need longer timeout)"
    echo "      }, std::time::Duration::from_secs(30));"
    echo "  }"
    exit 1
else
    echo -e "${GREEN}✓ All integration tests comply with timeout wrapper requirement${NC}"
    echo
fi

# ============================================================================
# Check for process spawning violations
# ============================================================================

echo "Checking for external process spawning in tests..."

# Pattern: std::process::Command::new or assert_cmd::Command::new with git/ls/cargo/ralph commands
# Skip test_timeout.rs as it only documents the rules
# Skip _TEMPLATE.rs as it only contains template examples
PROCESS_SPAWN_VIOLATIONS=$(rg -n --no-heading \
    'std::process::Command::new|assert_cmd::Command::new|Command::new\("git"|Command::new\("ls"|Command::new\("cargo"|Command::new\("ralph"|\.spawn\(\)' \
    "$TEST_DIR" --glob '*.rs' \
    -g '!test_timeout.rs' \
    -g '!_TEMPLATE.rs' \
    | grep -v '^\s*//' | grep -v '^\s*\*' || true)

if [ -n "$PROCESS_SPAWN_VIOLATIONS" ]; then
    violation_count=$(echo "$PROCESS_SPAWN_VIOLATIONS" | wc -l | tr -d ' ')
    echo -e "${RED}Found $violation_count process spawning violation(s)${NC}"
    echo
    echo "Violations:"
    echo "$PROCESS_SPAWN_VIOLATIONS" | while IFS=: read -r type file line_num rest; do
        if [ -n "$line_num" ]; then
            echo "  - $file:$line_num: process spawning detected ($rest)"
        fi
    done
    echo
    echo -e "${YELLOW}Process spawning is FORBIDDEN in integration tests.${NC}"
    echo "Use git2 library or MockGit/GitOps trait instead."
    echo "For CLI testing, use run_ralph_cli() which calls app::run() directly."
    echo "See tests/INTEGRATION_TESTS.md 'Rule 1.5: NO Process Spawning'"
    exit 1
else
    echo -e "${GREEN}No process spawning violations found${NC}"
    echo
fi

# ============================================================================
# Summary
# ============================================================================

 echo "Summary:"
 file_count=$(wc -l < "$TEMP_OUTPUT" | tr -d ' ')
 echo "  - Checked $file_count test file(s)"
 echo "  - All tests properly wrapped with timeout wrapper (with_default_timeout or with_timeout)"
 echo "  - No process spawning violations detected"
 exit 0
fi
