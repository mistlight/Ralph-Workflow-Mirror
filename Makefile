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
		if ! command -v cargo >/dev/null 2>&1; then \
			echo "error: cargo not found in PATH" >&2; \
			exit 1; \
		fi; \
		if ! cargo dylint --version >/dev/null 2>&1; then \
			echo "Installing cargo-dylint (and dylint-link)..." >&2; \
			cargo install cargo-dylint dylint-link; \
		fi; \
		if ! command -v rustup >/dev/null 2>&1; then \
			echo "rustup not found; installing rustup to $$HOME/.cargo (required for nightly + rustc-dev)." >&2; \
			if command -v curl >/dev/null 2>&1; then \
				curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path; \
			elif command -v wget >/dev/null 2>&1; then \
				wget -qO- https://sh.rustup.rs | sh -s -- -y --no-modify-path; \
			else \
				echo "error: need curl or wget to install rustup automatically" >&2; \
				exit 1; \
			fi; \
			export PATH="$$HOME/.cargo/bin:$$PATH"; \
		fi; \
		rustup toolchain install nightly --profile minimal --component rustc-dev --component llvm-tools-preview; \
		rustup component add rustc-dev llvm-tools-preview --toolchain nightly || true; \
		rustup run nightly cargo dylint -p ralph-workflow --lib file_too_long -- --lib; \
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
