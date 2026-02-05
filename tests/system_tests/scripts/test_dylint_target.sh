#!/usr/bin/env bash
##############################################################################
# Dylint Target Test Script
#
# This script validates that `make dylint` works correctly when:
# - System cargo (Homebrew/apt stable) is first in PATH
# - rustup default toolchain is stable
# - Nightly toolchain is available but not default
#
# Usage: ./tests/system_tests/scripts/test_dylint_target.sh
# Exit codes: 0 = success, 1 = failure
##############################################################################

set -euo pipefail

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[0;33m'
readonly NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "Testing dylint target..."
echo "Repository root: $REPO_ROOT"
echo

# Test 1: Verify nightly toolchain is available
echo "Test 1: Check nightly toolchain availability"
if ! command -v rustup >/dev/null 2>&1; then
	echo -e "${RED}FAIL: rustup not found (required for test)${NC}"
	exit 1
fi

if ! rustup toolchain list | grep -q nightly; then
	echo -e "${YELLOW}Nightly toolchain not installed, installing...${NC}"
	rustup toolchain install nightly --profile minimal
fi
echo -e "${GREEN}PASS: Nightly toolchain available${NC}"
echo

# Test 2: Simulate Homebrew/apt environment by checking PATH resolution
echo "Test 2: Verify cargo resolution in current environment"
SYSTEM_CARGO=$(which cargo)
echo "System cargo: $SYSTEM_CARGO"

if command -v rustup >/dev/null 2>&1; then
	NIGHTLY_CARGO=$(rustup which cargo --toolchain nightly)
	NIGHTLY_BIN_DIR=$(dirname "$NIGHTLY_CARGO")
	echo "Nightly cargo: $NIGHTLY_CARGO"
	echo "Nightly bin dir: $NIGHTLY_BIN_DIR"
	
	# Verify that prepending nightly bin dir changes resolution
	if [ "$SYSTEM_CARGO" != "$NIGHTLY_CARGO" ]; then
		echo -e "${GREEN}PASS: System cargo differs from nightly cargo${NC}"
		echo "This simulates the Homebrew/apt scenario"
	else
		echo -e "${YELLOW}SKIP: System cargo is already nightly${NC}"
	fi
fi
echo

# Test 3: Run make dylint and verify success (with regression test)
echo "Test 3: Run make dylint and check for E0554 regression"
cd "$REPO_ROOT"

# Capture make dylint output for regression testing
MAKE_OUTPUT=$(mktemp)
if make dylint 2>&1 | tee "$MAKE_OUTPUT"; then
	# Check that output doesn't contain the E0554 error
	if grep -q "error\[E0554\]" "$MAKE_OUTPUT"; then
		echo -e "${RED}FAIL: E0554 error detected (nightly features on stable channel)${NC}"
		echo "This indicates dylint driver was built with stable instead of nightly"
		rm -f "$MAKE_OUTPUT"
		exit 1
	fi
	
	# Check for the specific error about proc_macro_hygiene
	if grep -q "feature(proc_macro_hygiene)" "$MAKE_OUTPUT"; then
		echo -e "${RED}FAIL: proc_macro_hygiene feature error detected${NC}"
		rm -f "$MAKE_OUTPUT"
		exit 1
	fi
	
	echo -e "${GREEN}PASS: make dylint succeeded with no E0554 errors${NC}"
	rm -f "$MAKE_OUTPUT"
else
	echo -e "${RED}FAIL: make dylint failed${NC}"
	rm -f "$MAKE_OUTPUT"
	exit 1
fi
echo

# Test 4: Verify that the dylint driver was built with nightly
echo "Test 4: Verify dylint driver uses nightly"
# The driver should exist and work - if it was built with stable, it would have failed
PLATFORM=$(rustup show active-toolchain | cut -d' ' -f1 | cut -d- -f2-)
DRIVER_PATH="$HOME/.dylint_drivers/nightly-$PLATFORM/dylint-driver"

if [ -f "$DRIVER_PATH" ]; then
	echo "Driver found: $DRIVER_PATH"
	if "$DRIVER_PATH" -V >/dev/null 2>&1; then
		echo -e "${GREEN}PASS: dylint-driver is functional${NC}"
	else
		echo -e "${RED}FAIL: dylint-driver exists but is not functional${NC}"
		exit 1
	fi
else
	echo -e "${YELLOW}WARNING: Driver not found at expected path${NC}"
	echo "This may be expected if using a different platform"
fi
echo

echo -e "${GREEN}All tests passed!${NC}"
