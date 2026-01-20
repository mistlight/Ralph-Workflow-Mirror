#!/usr/bin/env bash
##############################################################################
# DO NOT MODIFY THIS SCRIPT UNLESS YOU BELIEVE IT IS FUNDAMENTALLY BROKEN
#
# AI AGENTS: This script enforces critical integration test style guide rules.
# DO NOT change the patterns, logic, or behavior of this script.
# DO NOT add exceptions or workarounds.
# DO NOT disable or bypass any checks.
#
# If this script fails, FIX THE PRODUCTION CODE, not this script.
#
# The ONLY valid reason to modify this script is if the script itself has a
# bug that causes false positives or false negatives. If you believe this is
# the case, document your reasoning thoroughly in the commit message.
##############################################################################
#
# No Test Flags in Production Code Checker
#
# This script validates that production code does NOT contain test-only
# conditional logic that violates the integration test style guide.
#
# See INTEGRATION_TESTS.md "Rule 2: No Test-Only Flags in Production Code"
#
# Forbidden patterns in production code:
#   1. cfg!(test) - Runtime test detection branches
#   2. test_mode: bool / is_test: bool - Test mode parameters
#   3. RUNNING_TESTS / TEST_ENV / IS_TESTING - Environment-based test detection
#   4. #[cfg(feature = "testing")] dual implementations
#
# Allowed patterns (not flagged):
#   - #[cfg(test)] mod tests { } - Test module declarations
#   - #[cfg(feature = "test-utils")] - Test utility exports
#   - Code in tests/ directory
#   - Comments and documentation
#
# Usage: ./tests/integration_tests/no_test_flags_check.sh
# Exit codes: 0 = clean, 1 = violations found, 2 = error
#
##############################################################################
# REMINDER: DO NOT MODIFY THIS SCRIPT. FIX THE PRODUCTION CODE INSTEAD.
##############################################################################

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[0;33m'
readonly NC='\033[0m' # No Color

# Get the repository root (two levels up from script location)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Production source directories to scan
PROD_DIR="$REPO_ROOT/ralph-workflow/src"

# Temporary files for results
TEMP_VIOLATIONS=$(mktemp)

cleanup() {
    rm -f "$TEMP_VIOLATIONS"
}
trap cleanup EXIT

echo "Running test flag compliance checks..."
echo "Repository root: $REPO_ROOT"
echo

# Verify production directory exists
if [ ! -d "$PROD_DIR" ]; then
    echo -e "${RED}Error: Production directory not found: $PROD_DIR${NC}"
    exit 2
fi

# Clear violations file
: > "$TEMP_VIOLATIONS"

# Use ripgrep for faster and more reliable pattern matching
# Fall back to grep if rg is not available
if command -v rg &> /dev/null; then
    SEARCH_CMD="rg"
else
    echo -e "${YELLOW}Warning: ripgrep (rg) not found, falling back to grep${NC}"
    SEARCH_CMD="grep"
fi

# Pattern 1: cfg!(test) - Runtime test detection
# This pattern creates different behavior at runtime based on test mode
echo "Checking for cfg!(test) runtime detection..."
if [ "$SEARCH_CMD" = "rg" ]; then
    # Use ripgrep - it handles the pattern matching more reliably
    # Exclude lines that are comments (start with // or *)
    rg -n --no-heading 'cfg!\s*\(\s*test\s*\)' "$PROD_DIR" --glob '*.rs' 2>/dev/null | \
        grep -v '^\s*//' | \
        grep -v '^\s*\*' | \
        grep -v '^\s*/\*' | \
        while IFS=: read -r file line_num content; do
            echo "cfg!(test):$file:$line_num:runtime test detection" >> "$TEMP_VIOLATIONS"
        done || true
else
    find "$PROD_DIR" -name "*.rs" -type f -exec grep -Hn 'cfg!\s*(\s*test\s*)' {} \; 2>/dev/null | \
        grep -v '^\s*//' | \
        while IFS=: read -r file line_num content; do
            echo "cfg!(test):$file:$line_num:runtime test detection" >> "$TEMP_VIOLATIONS"
        done || true
fi

# Pattern 2: test_mode: bool or is_test: bool parameters
# These parameters indicate test-conditional logic in function signatures
echo "Checking for test_mode/is_test boolean parameters..."
if [ "$SEARCH_CMD" = "rg" ]; then
    rg -n --no-heading '(test_mode|is_test)\s*:\s*bool' "$PROD_DIR" --glob '*.rs' 2>/dev/null | \
        grep -v '^\s*//' | \
        grep -v '^\s*\*' | \
        while IFS=: read -r file line_num content; do
            echo "test_param:$file:$line_num:test mode boolean parameter" >> "$TEMP_VIOLATIONS"
        done || true
else
    find "$PROD_DIR" -name "*.rs" -type f -exec grep -EHn '(test_mode|is_test)\s*:\s*bool' {} \; 2>/dev/null | \
        grep -v '^\s*//' | \
        while IFS=: read -r file line_num content; do
            echo "test_param:$file:$line_num:test mode boolean parameter" >> "$TEMP_VIOLATIONS"
        done || true
fi

# Pattern 3: Environment-based test detection
# Checking for RUNNING_TESTS, TEST_ENV, IS_TESTING environment variables
echo "Checking for test detection environment variables..."
if [ "$SEARCH_CMD" = "rg" ]; then
    rg -n --no-heading 'env::var\s*\(\s*"(RUNNING_TESTS|TEST_ENV|IS_TESTING)"' "$PROD_DIR" --glob '*.rs' 2>/dev/null | \
        grep -v '^\s*//' | \
        grep -v '^\s*\*' | \
        while IFS=: read -r file line_num content; do
            echo "test_env:$file:$line_num:test detection environment variable" >> "$TEMP_VIOLATIONS"
        done || true
else
    find "$PROD_DIR" -name "*.rs" -type f -exec grep -EHn 'env::var\s*\(\s*"(RUNNING_TESTS|TEST_ENV|IS_TESTING)"' {} \; 2>/dev/null | \
        grep -v '^\s*//' | \
        while IFS=: read -r file line_num content; do
            echo "test_env:$file:$line_num:test detection environment variable" >> "$TEMP_VIOLATIONS"
        done || true
fi

# Pattern 4: #[cfg(feature = "testing")] for dual implementations
# This is only a violation if it creates two different implementations
# We check for the pattern but exclude test-utils which is legitimate
echo "Checking for testing feature flag dual implementations..."
if [ "$SEARCH_CMD" = "rg" ]; then
    rg -n --no-heading '#\[cfg\(feature\s*=\s*"testing"\)\]' "$PROD_DIR" --glob '*.rs' 2>/dev/null | \
        grep -v '^\s*//' | \
        grep -v '^\s*\*' | \
        while IFS=: read -r file line_num content; do
            echo "testing_feature:$file:$line_num:testing feature flag (use test-utils instead)" >> "$TEMP_VIOLATIONS"
        done || true
else
    find "$PROD_DIR" -name "*.rs" -type f -exec grep -EHn '#\[cfg\(feature\s*=\s*"testing"\)\]' {} \; 2>/dev/null | \
        grep -v '^\s*//' | \
        while IFS=: read -r file line_num content; do
            echo "testing_feature:$file:$line_num:testing feature flag (use test-utils instead)" >> "$TEMP_VIOLATIONS"
        done || true
fi

# Check for violations
echo
if [ -s "$TEMP_VIOLATIONS" ]; then
    violation_count=$(wc -l < "$TEMP_VIOLATIONS" | tr -d ' ')
    echo -e "${RED}Found $violation_count violation(s) in production code${NC}"
    echo
    echo "Violations:"
    while IFS=: read -r type file line_num description; do
        # Make path relative for readability
        rel_path="${file#$REPO_ROOT/}"
        echo "  - $rel_path:$line_num: $description"
    done < "$TEMP_VIOLATIONS"
    echo
    echo -e "${YELLOW}To fix: Use dependency injection instead of test flags.${NC}"
    echo "See tests/INTEGRATION_TESTS.md \"Rule 2: No Test-Only Flags in Production Code\""
    echo
    echo "Examples of proper patterns:"
    echo "  - Accept trait objects for external dependencies"
    echo "  - Use TempDir for filesystem isolation in tests"
    echo "  - Pass configuration as parameters, not environment checks"
    exit 1
else
    echo -e "${GREEN}All production code complies with test flag rules${NC}"
    echo
    echo "Summary:"
    file_count=$(find "$PROD_DIR" -name "*.rs" -type f 2>/dev/null | wc -l | tr -d ' ')
    echo "  - Scanned $file_count Rust file(s) in production directories"
    echo "  - No forbidden test flags found"
    exit 0
fi
