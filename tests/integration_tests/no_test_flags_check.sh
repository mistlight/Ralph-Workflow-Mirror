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
# See docs/agents/testing-guide.md "Common Anti-Patterns" section
#
# Forbidden patterns in production code:
#   1. cfg!(test) - Runtime test detection branches
#   2. Test mode boolean parameters (test_mode, is_test, skip_*, mock_*, fake_*, etc.)
#   3. Test/mock/skip environment variable checks (any env var suggesting test bypass)
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

# By default, this script is silent on success so it can be used in "no-output"
# verification pipelines. Set NO_TEST_FLAGS_CHECK_QUIET=0 to enable informational output.
readonly NO_TEST_FLAGS_CHECK_QUIET="${NO_TEST_FLAGS_CHECK_QUIET:-1}"

log() {
    if [ "$NO_TEST_FLAGS_CHECK_QUIET" = "0" ]; then
        echo "$@"
    fi
}

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

log "Running test flag compliance checks..."
log "Repository root: $REPO_ROOT"
log

# Verify production directory exists
if [ ! -d "$PROD_DIR" ]; then
    echo -e "${RED}Error: Production directory not found: $PROD_DIR${NC}"
    exit 2
fi

# Clear violations file
: > "$TEMP_VIOLATIONS"

# Require ripgrep for reliable pattern matching
if ! command -v rg &> /dev/null; then
    echo -e "${RED}Error: ripgrep (rg) is required but not found${NC}"
    echo "Install with: brew install ripgrep (macOS) or cargo install ripgrep"
    exit 2
fi

# Helper function to search and record violations
# Args: $1 = pattern, $2 = description, $3 = violation_type
search_pattern() {
    local pattern="$1"
    local description="$2"
    local violation_type="$3"
    
    rg -n --no-heading "$pattern" "$PROD_DIR" --glob '*.rs' 2>/dev/null | \
        grep -v '^\s*//' | \
        grep -v '^\s*\*' | \
        grep -v '^\s*//!' | \
        grep -v '^\s*////' | \
        while IFS=: read -r file line_num content; do
            echo "$violation_type:$file:$line_num:$description" >> "$TEMP_VIOLATIONS"
        done || true
}

##############################################################################
# Pattern 1: cfg!(test) - Runtime test detection
# This pattern creates different behavior at runtime based on test mode
##############################################################################
log "Checking for cfg!(test) runtime detection..."
search_pattern 'cfg!\s*\(\s*test\s*\)' "runtime test detection via cfg!(test)" "cfg_test"

##############################################################################
# Pattern 2: Test mode boolean parameters
# These parameters indicate test-conditional logic in function signatures
# Catches: test_mode, is_test, is_testing, skip_validation, mock_mode, 
#          fake_mode, dry_run (when used for testing), stub_mode, etc.
##############################################################################
log "Checking for test/mock/skip boolean parameters..."

# Direct test mode flags
search_pattern '(test_mode|is_test|is_testing|testing_mode)\s*:\s*bool' \
    "test mode boolean parameter" "test_param"

# Skip/bypass flags that suggest test shortcuts
search_pattern '(skip_validation|skip_verify|skip_check|skip_auth|skip_api)\s*:\s*bool' \
    "skip/bypass boolean parameter (test shortcut)" "skip_param"

# Mock/fake/stub flags
search_pattern '(mock_mode|fake_mode|stub_mode|use_mock|use_fake|use_stub)\s*:\s*bool' \
    "mock/fake/stub boolean parameter" "mock_param"

# Disable flags that might be test shortcuts
search_pattern '(disable_[a-z_]+|no_[a-z_]+_check)\s*:\s*bool' \
    "disable/no-check boolean parameter" "disable_param"

##############################################################################
# Pattern 3: Environment-based test/mock detection
# Catches any environment variable that looks like a test bypass mechanism
# Uses broad patterns to catch variations like:
#   RUNNING_TESTS, TEST_MODE, IS_TESTING, SKIP_AUTH, MOCK_API, etc.
#
# ALLOWED standard env vars (not flagged):
#   - NO_COLOR, CLICOLOR, CLICOLOR_FORCE, TERM - Standard terminal config
#   - CI, GITHUB_ACTIONS, GITLAB_CI - CI detection (not bypass)
#   - HOME, USER, PATH, PWD, etc. - Standard system vars
#   - CARGO_*, RUSTFLAGS, etc. - Build configuration
##############################################################################
log "Checking for test/mock/skip environment variables..."

# Test-related env vars (TEST, TESTING, etc.)
# Excludes: CARGO_TEST_*, which is legitimate cargo config
search_pattern 'env::var\s*\(\s*"(RUNNING_TEST|IS_TEST|TEST_MODE|TESTING_MODE|IN_TEST)"' \
    "test-related environment variable" "test_env"

# Skip/bypass env vars for authentication, validation, verification
# These suggest shortcuts that bypass production behavior
search_pattern 'env::var\s*\(\s*"(SKIP_AUTH|SKIP_VALIDATION|SKIP_VERIFY|SKIP_CHECK|SKIP_API)"' \
    "skip/bypass environment variable" "skip_env"

# Mock/fake/stub env vars
search_pattern 'env::var\s*\(\s*"(MOCK_|FAKE_|STUB_|USE_MOCK|USE_FAKE)[A-Z_]*"' \
    "mock/fake/stub environment variable" "mock_env"

# Disable env vars that bypass security or validation
search_pattern 'env::var\s*\(\s*"(DISABLE_AUTH|DISABLE_VALIDATION|DISABLE_VERIFY|DISABLE_SSL|DISABLE_TLS)"' \
    "disable security/validation environment variable" "disable_env"

# CI-specific bypass vars (sometimes used incorrectly to skip things in CI)
search_pattern 'env::var\s*\(\s*"(CI_SKIP_|CI_NO_|CI_DISABLE_)[A-Z_]+"' \
    "CI-specific bypass environment variable" "ci_bypass_env"

##############################################################################
# Pattern 4: #[cfg(feature = "testing")] for dual implementations
# This is a violation because it creates two different code paths
# test-utils is allowed because it only exports test utilities
##############################################################################
log "Checking for testing feature flag dual implementations..."
search_pattern '#\[cfg\(feature\s*=\s*"testing"\)\]' \
    "testing feature flag (use test-utils instead)" "testing_feature"

##############################################################################
# Pattern 5: Conditional compilation that changes behavior for tests
# Catches patterns like #[cfg(not(test))] in production logic
##############################################################################
log "Checking for conditional test compilation in logic..."
search_pattern '#\[cfg\(not\(test\)\)\]' \
    "conditional compilation excluding tests (creates untested code path)" "cfg_not_test"

##############################################################################
# Results
##############################################################################
log
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
    echo "See docs/agents/testing-guide.md \"Common Anti-Patterns\" section"
    echo
    echo "Examples of proper patterns:"
    echo "  - Accept trait objects for external dependencies"
    echo "  - Use TempDir for filesystem isolation in tests"
    echo "  - Pass configuration as parameters, not environment checks"
    echo "  - Use the test-utils feature to expose test helpers, not to change behavior"
    exit 1
else
    log -e "${GREEN}All production code complies with test flag rules${NC}"
    log
    log "Summary:"
    file_count=$(find "$PROD_DIR" -name "*.rs" -type f 2>/dev/null | wc -l | tr -d ' ')
    log "  - Scanned $file_count Rust file(s) in production directories"
    log "  - No forbidden test flags found"
    exit 0
fi
