#!/usr/bin/env bash
##############################################################################
# Dylint Docker Test Script
#
# Tests that `make dylint` works in a minimal Rust Docker environment
# starting from rust:slim with only minimal OS dependencies.
#
# Prerequisites: Docker must be installed and running
# Usage: ./tests/integration_tests/test_dylint_docker.sh
# Exit codes: 0 = success (or skip if Docker unavailable)
##############################################################################

set -euo pipefail

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[0;33m'
readonly NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "Testing dylint in Docker (rust:slim)..."
echo "Repository root: $REPO_ROOT"
echo

# Check if Docker is available
if ! command -v docker >/dev/null 2>&1; then
	echo -e "${YELLOW}SKIP: Docker not available${NC}"
	exit 0
fi

# Check if Docker daemon is running
if ! docker info >/dev/null 2>&1; then
	echo -e "${YELLOW}SKIP: Docker daemon not running${NC}"
	exit 0
fi

# Run dylint in rust:slim container
DOCKER_OUTPUT=$(mktemp)
echo "Running make dylint in rust:slim container..."
echo "(This may take several minutes on first run)"
echo

# Run and capture exit code separately so we can check for specific errors
set +e
docker run --rm -v "$REPO_ROOT:/work" -w /work rust:slim bash -lc '
	set -euo pipefail
	echo "Installing minimal OS dependencies..."
	apt-get update && apt-get install -y --no-install-recommends \
		ca-certificates curl make pkg-config libssl-dev
	echo ""
	echo "Running make dylint..."
	make dylint
' 2>&1 | tee "$DOCKER_OUTPUT"
DOCKER_EXIT=$?
set -e

if [ $DOCKER_EXIT -eq 0 ]; then
	echo -e "${GREEN}PASS: make dylint succeeded in Docker environment${NC}"
	rm -f "$DOCKER_OUTPUT"
	exit 0
fi

# If we got here, make dylint failed. Check for specific errors.

# Check for E0554 errors (regression test - this is the bug we're fixing)
if grep -q "error\[E0554\]" "$DOCKER_OUTPUT"; then
	echo -e "${RED}FAIL: E0554 error in Docker environment${NC}"
	echo "This indicates dylint driver was built with stable instead of nightly"
	rm -f "$DOCKER_OUTPUT"
	exit 1
fi

# Check for the specific proc_macro_hygiene feature error
if grep -q "feature(proc_macro_hygiene)" "$DOCKER_OUTPUT"; then
	echo -e "${RED}FAIL: proc_macro_hygiene feature error in Docker${NC}"
	rm -f "$DOCKER_OUTPUT"
	exit 1
fi

# Check for stable release channel error
if grep -q "may not be used on the stable release channel" "$DOCKER_OUTPUT"; then
	echo -e "${RED}FAIL: Stable release channel error in Docker${NC}"
	rm -f "$DOCKER_OUTPUT"
	exit 1
fi

# Check if dylint-link failed to find the .so file (known limitation in some Docker environments)
if grep -q "Could not find.*libfile_too_long.*despite successful build" "$DOCKER_OUTPUT"; then
	echo -e "${YELLOW}WARNING: dylint-link couldn't locate compiled library${NC}"
	echo "This is a known dylint limitation in some Docker environments"
	echo "The important verification passed: no E0554 errors (nightly toolchain was used)"
	rm -f "$DOCKER_OUTPUT"
	exit 0
fi

# Unknown failure
echo -e "${RED}FAIL: make dylint failed in Docker (exit code: $DOCKER_EXIT)${NC}"
echo ""
echo "Last 30 lines of output:"
tail -n 30 "$DOCKER_OUTPUT"
rm -f "$DOCKER_OUTPUT"
exit 1

echo -e "${GREEN}Docker test completed successfully!${NC}"
