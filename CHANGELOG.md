# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
### Added
- **Reducer-based pipeline architecture** (RFC-004): Complete refactoring from procedural control flow to event-sourced state machine
  - Pure reducer function for deterministic state transitions
  - Comprehensive event types for all pipeline state changes
  - Effect handler architecture isolating side effects
  - Fault-tolerant agent executor catching all panics and errors
  - State machine tracks agent fallback chains, rebase progress, and commit generation
  - Event loop orchestrates all pipeline phases
  - Improved checkpoint/resume via state serialization
  - 33+ integration tests for reducer state machine

### Changed
- Agent failures (including segfaults SIGSEGV=139, panics, I/O errors) now never crash the pipeline
- Pipeline automatically retries on transient failures (Network, RateLimit, Timeout, ModelUnavailable)
- Pipeline automatically falls back to next agent/model on permanent failures (Authentication, FileSystem, InternalError)
- All state transitions are explicit and testable via unit tests

### Fixed
- Removed dead code in app/mod.rs (unused development phase functions: run_development, run_review_and_fix)
- Eliminated clippy allow/expect attributes in reducer code
- Improved code quality to meet AGENTS.md compliance (clippy clean)

### Testing
- Added 31+ unit tests for reducer state machine (state_reduction.rs, state.rs)
- Added 33+ integration tests for reducer state machine transitions (reducer_state_machine.rs)
- Verified all existing integration tests pass with new architecture (1680+ tests total)
- All AGENTS.md compliance checks passing (no allow/expect attributes, no test flags in production code)
