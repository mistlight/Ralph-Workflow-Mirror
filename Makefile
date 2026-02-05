# Ralph Makefile
# Build and installation for the Ralph multi-agent orchestrator

# Configuration
BINARY_NAME := ralph
INSTALL_ROOT ?= /usr/local
INSTALL_BIN := $(INSTALL_ROOT)/bin

# Rust build configuration
CARGO := cargo
CARGO_FLAGS :=
RELEASE_FLAGS := --release

# Detect platform
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    PLATFORM := macos
else ifeq ($(UNAME_S),Linux)
    PLATFORM := linux
else
    PLATFORM := unknown
endif

.PHONY: all build release test clean install uninstall check fmt lint dylint help

# Default target
all: build

# Build debug version
build:
	$(CARGO) build $(CARGO_FLAGS)
	echo "Debug build complete: target/debug/$(BINARY_NAME)"

# Build release version (optimized)
release:
	$(CARGO) build $(RELEASE_FLAGS)
	echo "Release build complete: target/release/$(BINARY_NAME)"

# Run all tests
test:
	$(CARGO) test $(CARGO_FLAGS)
	echo "All tests passed"

# Run tests with output
test-verbose:
	$(CARGO) test $(CARGO_FLAGS) -- --nocapture

# Clean build artifacts
clean:
	$(CARGO) clean
	echo "Build artifacts cleaned"

# Install the binary (requires sudo for system directories)
install: release
	echo "Installing $(BINARY_NAME) to $(INSTALL_BIN)..."
	mkdir -p $(INSTALL_BIN)
	install -m 755 target/release/$(BINARY_NAME) $(INSTALL_BIN)/$(BINARY_NAME)
	echo "Installed: $(INSTALL_BIN)/$(BINARY_NAME)"
	echo ""
	echo "Installation complete! Run 'ralph --help' to get started."

# Install to user's local bin (no sudo needed)
install-local:
	$(MAKE) install INSTALL_ROOT=$(HOME)/.local

# Uninstall the binary
uninstall:
	echo "Removing $(INSTALL_BIN)/$(BINARY_NAME)..."
	rm -f $(INSTALL_BIN)/$(BINARY_NAME)
	echo "Uninstalled"

# Type checking and linting
check:
	$(CARGO) check $(CARGO_FLAGS)
	echo "Type check passed"

# Format code
fmt:
	$(CARGO) fmt
	echo "Code formatted"

# Check formatting without modifying
fmt-check:
	$(CARGO) fmt -- --check
	echo "Format check passed"

# Run clippy lints
lint:
	$(CARGO) clippy $(CARGO_FLAGS) --all-targets -- -D warnings
	echo "Lint check passed"

# Run custom dylint lints (safe default: lib only)
dylint:
	@bash -euo pipefail -c '\
		HOME_DIR="$${HOME:-}"; \
		CARGO_HOME_DIR="$${CARGO_HOME:-}"; \
		RUSTUP_HOME_DIR="$${RUSTUP_HOME:-}"; \
		DYLINT_DRIVER_DIR="$${DYLINT_DRIVER_PATH:-}"; \
		\
		if [ -z "$$CARGO_HOME_DIR" ]; then \
			if [ -n "$$HOME_DIR" ]; then \
				CARGO_HOME_DIR="$$HOME_DIR/.cargo"; \
			else \
				echo "error: HOME is not set and CARGO_HOME is not set." >&2; \
				echo "Set HOME, or set CARGO_HOME and RUSTUP_HOME to writable locations." >&2; \
				exit 1; \
			fi; \
		fi; \
		if [ -z "$$RUSTUP_HOME_DIR" ]; then \
			if [ -n "$$HOME_DIR" ]; then \
				RUSTUP_HOME_DIR="$$HOME_DIR/.rustup"; \
			else \
				echo "error: HOME is not set and RUSTUP_HOME is not set." >&2; \
				echo "Set HOME, or set RUSTUP_HOME to a writable location." >&2; \
				exit 1; \
			fi; \
		fi; \
		if [ -z "$$DYLINT_DRIVER_DIR" ]; then \
			if [ -n "$$HOME_DIR" ]; then \
				DYLINT_DRIVER_DIR="$$HOME_DIR/.dylint_drivers"; \
			else \
				echo "error: HOME is not set and DYLINT_DRIVER_PATH is not set." >&2; \
				echo "Set HOME, or set DYLINT_DRIVER_PATH to a writable location." >&2; \
				exit 1; \
			fi; \
		fi; \
		\
		export CARGO_HOME="$$CARGO_HOME_DIR"; \
		export RUSTUP_HOME="$$RUSTUP_HOME_DIR"; \
		export DYLINT_DRIVER_PATH="$$DYLINT_DRIVER_DIR"; \
		export PATH="$$CARGO_HOME/bin:$$PATH"; \
		\
		for dir in "$$CARGO_HOME" "$$RUSTUP_HOME" "$$DYLINT_DRIVER_PATH"; do \
			if ! mkdir -p "$$dir" 2>/dev/null; then \
				echo "error: cannot create required directory: $$dir" >&2; \
				echo "Set CARGO_HOME/RUSTUP_HOME/DYLINT_DRIVER_PATH to writable locations." >&2; \
				exit 1; \
			fi; \
			if [ ! -w "$$dir" ]; then \
				echo "error: required directory is not writable: $$dir" >&2; \
				echo "Set CARGO_HOME/RUSTUP_HOME/DYLINT_DRIVER_PATH to writable locations." >&2; \
				exit 1; \
			fi; \
		done; \
		\
		if ! command -v rustup >/dev/null 2>&1; then \
			echo "error: rustup not found (required for nightly + rustc-dev)." >&2; \
			echo "Install rustup manually (see https://rustup.rs) and re-run make dylint." >&2; \
			echo "Automated rustup installation is intentionally disabled in this Makefile." >&2; \
			exit 1; \
		fi; \
		\
		if ! command -v cargo >/dev/null 2>&1; then \
			echo "error: cargo not found. Ensure $$CARGO_HOME/bin is on PATH." >&2; \
			exit 1; \
		fi; \
		\
		if ! rustup toolchain list | grep -qE "^nightly"; then \
			echo "error: Rust nightly toolchain is not installed (required for dylint driver builds)." >&2; \
			echo "Install it manually: rustup toolchain install nightly --profile minimal" >&2; \
			exit 1; \
		fi; \
		\
		INSTALLED_COMPONENTS="$$(rustup component list --toolchain nightly --installed 2>/dev/null || true)"; \
		MISSING=""; \
		echo "$$INSTALLED_COMPONENTS" | grep -q "^rustc-dev" || MISSING="$$MISSING rustc-dev"; \
		echo "$$INSTALLED_COMPONENTS" | grep -qE "^llvm-tools(-preview)?" || MISSING="$$MISSING llvm-tools-preview"; \
		if [ -n "$$MISSING" ]; then \
			echo "error: missing required nightly components:$$MISSING" >&2; \
			echo "Install them manually: rustup component add rustc-dev llvm-tools-preview llvm-tools --toolchain nightly" >&2; \
			exit 1; \
		fi; \
		\
		NIGHTLY_CARGO="$$(rustup which cargo --toolchain nightly)"; \
		NIGHTLY_RUSTC="$$(rustup which rustc --toolchain nightly)"; \
		NIGHTLY_BIN_DIR="$$(dirname "$$NIGHTLY_CARGO")"; \
		WRAPPER_DIR="$$(mktemp -d)"; \
		trap "rm -rf $$WRAPPER_DIR" EXIT; \
		printf "%s\n" \
			"#!/usr/bin/env bash" \
			"export RUSTUP_TOOLCHAIN=nightly" \
			"exec \"$$NIGHTLY_CARGO\" \"\$$@\"" \
			> "$$WRAPPER_DIR/cargo"; \
		chmod +x "$$WRAPPER_DIR/cargo"; \
		export PATH="$$WRAPPER_DIR:$$NIGHTLY_BIN_DIR:$$PATH"; \
		export RUSTUP_TOOLCHAIN=nightly; \
		# Do not export RUSTC here: cargo-dylint invokes the driver with `env -u RUSTC`, \
		# and exporting it can cause toolchain/stdlib resolution issues in some environments. \
		# The PATH wrapper ensures nightly is used for all subprocesses. \
		\
		if ! cargo dylint --version >/dev/null 2>&1; then \
			echo "error: cargo-dylint is not installed (and dylint-link may be missing)." >&2; \
			echo "Install it manually, or opt in via ALLOW_CARGO_INSTALL=1." >&2; \
			if [ "$${ALLOW_CARGO_INSTALL:-0}" = "1" ]; then \
				echo "ALLOW_CARGO_INSTALL=1 set; installing cargo-dylint (and dylint-link)..." >&2; \
				if ! cargo install cargo-dylint dylint-link; then \
					echo "error: failed to install cargo-dylint." >&2; \
					exit 1; \
				fi; \
			else \
				exit 1; \
			fi; \
		fi; \
		\
		cargo dylint -p ralph-workflow --lib file_too_long -- --lib; \
	'

# Run all checks (format, lint, test)
ci: fmt-check lint test
	echo "All CI checks passed"

# Build documentation
doc:
	$(CARGO) doc --no-deps --open

# Print version info
version:
	echo "Ralph build configuration:"
	echo "  Binary: $(BINARY_NAME)"
	echo "  Platform: $(PLATFORM)"
	echo "  Install path: $(INSTALL_BIN)/$(BINARY_NAME)"
	$(CARGO) --version
	rustc --version

# Help
help:
	echo "Ralph Makefile targets:"
	echo ""
	echo "  build         Build debug version"
	echo "  release       Build optimized release version"
	echo "  test          Run all tests"
	echo "  test-verbose  Run tests with output"
	echo "  clean         Remove build artifacts"
	echo "  install       Install to $(INSTALL_BIN) (may need sudo)"
	echo "  install-local Install to ~/.local/bin (no sudo needed)"
	echo "  uninstall     Remove installed binary"
	echo "  check         Run type checks"
	echo "  fmt           Format source code"
	echo "  lint          Run clippy lints"
	echo "  dylint        Run custom dylint lints (lib only)"
	echo "  ci            Run all CI checks"
	echo "  doc           Build and open documentation"
	echo "  version       Print version information"
	echo "  help          Show this help"
	echo ""
	echo "Environment variables:"
	echo "  INSTALL_ROOT  Installation prefix (default: /usr/local)"
	echo ""
	echo "Examples:"
	echo "  make release && sudo make install"
	echo "  make install-local"
	echo "  INSTALL_ROOT=/opt make install"
