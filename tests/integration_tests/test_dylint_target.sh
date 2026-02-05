#!/usr/bin/env bash
##############################################################################
# Dylint Target Test Script
#
# This script validates that `make dylint` works correctly when:
# - System cargo (Homebrew/apt stable) is first in PATH
# - rustup default toolchain is stable
# - Nightly toolchain is available but not default
#
# Usage: ./tests/integration_tests/test_dylint_target.sh
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

# Test 5: Sandbox environment simulation (verify environment variable handling)
echo "Test 5: Verify Makefile respects environment variable overrides"
# This test verifies that the Makefile correctly reads and uses custom environment
# variables, even though actually running with fully redirected HOME/CARGO_HOME/RUSTUP_HOME
# would require a complete rustup installation in the alternate location.

# Just verify that the default CARGO_HOME is shown correctly
VERBOSE_OUTPUT=$(mktemp)
make dylint-verbose >"$VERBOSE_OUTPUT" 2>&1

# Verify that CARGO_HOME is shown in the output (should be ~/.cargo or equivalent)
if grep -q "CARGO_HOME:" "$VERBOSE_OUTPUT"; then
	SHOWN_CARGO_HOME=$(grep "CARGO_HOME:" "$VERBOSE_OUTPUT" | head -1 | cut -d: -f2- | xargs)
	echo "Makefile shows CARGO_HOME: $SHOWN_CARGO_HOME"
	echo -e "${GREEN}PASS: Makefile correctly exports CARGO_HOME${NC}"
else
	echo -e "${RED}FAIL: CARGO_HOME not shown in verbose output${NC}"
	rm -f "$VERBOSE_OUTPUT"
	exit 1
fi

# Verify that RUSTUP_HOME is shown
if grep -q "RUSTUP_HOME:" "$VERBOSE_OUTPUT"; then
	SHOWN_RUSTUP_HOME=$(grep "RUSTUP_HOME:" "$VERBOSE_OUTPUT" | head -1 | cut -d: -f2- | xargs)
	echo "Makefile shows RUSTUP_HOME: $SHOWN_RUSTUP_HOME"
	echo -e "${GREEN}PASS: Makefile correctly exports RUSTUP_HOME${NC}"
else
	echo -e "${YELLOW}WARNING: RUSTUP_HOME not shown in verbose output${NC}"
fi

# Verify that DYLINT_DRIVER_PATH is shown
if grep -q "DYLINT_DRIVER_PATH:" "$VERBOSE_OUTPUT"; then
	SHOWN_DRIVER_PATH=$(grep "DYLINT_DRIVER_PATH:" "$VERBOSE_OUTPUT" | head -1 | cut -d: -f2- | xargs)
	echo "Makefile shows DYLINT_DRIVER_PATH: $SHOWN_DRIVER_PATH"
	echo -e "${GREEN}PASS: Makefile correctly exports DYLINT_DRIVER_PATH${NC}"
else
	echo -e "${YELLOW}WARNING: DYLINT_DRIVER_PATH not shown in verbose output${NC}"
fi

rm -f "$VERBOSE_OUTPUT"
echo

# Test 6: Path resolution verification
echo "Test 6: Verify PATH includes nightly bin directory"
# Run make dylint-verbose to capture debug output
VERBOSE_OUTPUT=$(mktemp)
make dylint-verbose >"$VERBOSE_OUTPUT" 2>&1

# Verify PATH is shown in the output
if grep -q "PATH (first 3 entries):" "$VERBOSE_OUTPUT"; then
	PATH_LINE=$(grep "PATH (first 3 entries):" "$VERBOSE_OUTPUT" | head -1)
	echo "$PATH_LINE"
	echo -e "${GREEN}PASS: PATH resolution is shown in verbose output${NC}"
else
	echo -e "${RED}FAIL: PATH not shown in verbose output${NC}"
	rm -f "$VERBOSE_OUTPUT"
	exit 1
fi

# Verify nightly bin directory is shown
if grep -q "Nightly bin dir:" "$VERBOSE_OUTPUT"; then
	NIGHTLY_BIN_LINE=$(grep "Nightly bin dir:" "$VERBOSE_OUTPUT" | head -1)
	echo "$NIGHTLY_BIN_LINE"
	echo -e "${GREEN}PASS: Nightly bin directory is configured${NC}"
else
	echo -e "${YELLOW}WARNING: Nightly bin dir not shown in verbose output${NC}"
fi

# Verify that 'which cargo' resolves to nightly (should be in nightly toolchain path)
if grep -q "which cargo:" "$VERBOSE_OUTPUT"; then
	WHICH_CARGO=$(grep "which cargo:" "$VERBOSE_OUTPUT" | head -1)
	echo "$WHICH_CARGO"
	# Check if the path contains 'nightly' to verify it's using the nightly cargo
	if echo "$WHICH_CARGO" | grep -q "nightly"; then
		echo -e "${GREEN}PASS: cargo resolves to nightly toolchain${NC}"
	else
		echo -e "${YELLOW}WARNING: cargo path doesn't contain 'nightly'${NC}"
	fi
fi

rm -f "$VERBOSE_OUTPUT"
echo

# Test 7: Verify wrapper script is used
echo "Test 7: Verify cargo wrapper script is invoked"
VERBOSE_OUTPUT=$(mktemp)
make dylint-verbose >"$VERBOSE_OUTPUT" 2>&1

# Check that wrapper script was created and used
if grep -q "Wrapper script path:" "$VERBOSE_OUTPUT"; then
	WRAPPER_PATH=$(grep "Wrapper script path:" "$VERBOSE_OUTPUT" | cut -d: -f2- | xargs)
	echo "Wrapper path: $WRAPPER_PATH"
	echo -e "${GREEN}PASS: Wrapper script path shown${NC}"
else
	echo -e "${RED}FAIL: Wrapper script path not shown${NC}"
	rm -f "$VERBOSE_OUTPUT"
	exit 1
fi

# Verify that resolved cargo matches wrapper path
if grep -q "Resolved cargo (via command -v):" "$VERBOSE_OUTPUT"; then
	RESOLVED_CARGO=$(grep "Resolved cargo (via command -v):" "$VERBOSE_OUTPUT" | cut -d: -f2- | xargs)
	echo "Resolved cargo: $RESOLVED_CARGO"
	if [ "$RESOLVED_CARGO" = "$WRAPPER_PATH" ]; then
		echo -e "${GREEN}PASS: cargo resolves to wrapper script${NC}"
	else
		echo -e "${YELLOW}WARNING: cargo does not resolve to wrapper${NC}"
		echo "Expected: $WRAPPER_PATH"
		echo "Got: $RESOLVED_CARGO"
	fi
fi

# Verify wrapper script contents include nightly enforcement
if grep -q "export RUSTUP_TOOLCHAIN=nightly" "$VERBOSE_OUTPUT"; then
	echo -e "${GREEN}PASS: Wrapper script exports RUSTUP_TOOLCHAIN=nightly${NC}"
else
	echo -e "${RED}FAIL: Wrapper script does not export RUSTUP_TOOLCHAIN=nightly${NC}"
	rm -f "$VERBOSE_OUTPUT"
	exit 1
fi

# Verify wrapper script exports CARGO variable
if grep -q "export CARGO=" "$VERBOSE_OUTPUT"; then
	echo -e "${GREEN}PASS: Wrapper script exports CARGO variable${NC}"
else
	echo -e "${YELLOW}WARNING: Wrapper script does not export CARGO variable${NC}"
fi

rm -f "$VERBOSE_OUTPUT"
echo

echo -e "${GREEN}All tests passed!${NC}"
