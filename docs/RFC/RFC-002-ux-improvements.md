# RFC-002: Developer Experience Improvements for Ralph Orchestrator

**RFC Number**: RFC-002
**Title**: Developer Experience Improvements for Ralph Orchestrator
**Status**: In Progress
**Author**: Analysis based on codebase review
**Created**: 2026-01-15
**Last Updated**: 2026-01-17

---

## Executive Summary

This RFC proposes UX improvements for Ralph Workflow based on industry-standard CLI design principles and patterns from production tools like GitHub CLI, cargo, npm, and lazygit.

### Key Gaps Identified

| Gap | Industry Principle Violated | Impact |
|-----|----------------------------|--------|
| No feedback for 1-5s on startup | "Print within 100ms" (clig.dev) | Users think app is frozen |
| No progress during iterations | "Show progress visually" (Atlassian) | Users cancel working pipelines |
| Errors lack actionable fixes | "Suggest next command" (clig.dev) | Users stuck searching docs |
| No Ctrl+C hint | "Provide easy way out" (Atlassian) | Users force-kill, lose state |
| First run fails silently | "Progressive discovery" (clig.dev) | 50%+ new users fail first attempt |

### Top 5 Recommendations (P0)

1. **Immediate feedback** - Print "Starting..." within 100ms before agent calls
2. **Progress indicator** - Show `[Development 3/5] claude ━━━━━━ 2m 34s`
3. **Actionable errors** - Include copy-pasteable fix commands in every error
4. **Ctrl+C hint** - Display "Press Ctrl+C to cancel" during long operations
5. **First-run wizard** - Auto-detect and offer guided setup

### Effort Estimate

| Priority | Items | Total Effort |
|----------|-------|--------------|
| P0 (Critical) | 6 items | ~2 weeks |
| P1 (High) | 8 items | ~3 weeks |
| P2 (Medium) | 4 items | ~2 weeks |
| P3 (Lower) | 4 items | ~2 weeks |

---

## Abstract

This RFC proposes a comprehensive set of user experience improvements for Ralph Workflow to enhance developer productivity, reduce friction for new users, and provide better feedback during long-running operations. The proposal is grounded in industry-standard CLI design principles from [Command Line Interface Guidelines](https://clig.dev/), [Atlassian's 10 Design Principles](https://www.atlassian.com/blog/it-teams/10-design-principles-for-delightful-clis), and patterns from production tools like GitHub CLI, cargo, and npm.

---

## Implementation Status

**Last Updated**: 2026-01-17

This section tracks the implementation status of RFC-002 proposals against the actual codebase.

### Completed Features

#### ✅ Phase 5: Color & Terminal Standards (P1)

**Status**: Fully Implemented

**Location**: `ralph-workflow/src/json_parser/terminal.rs:1-325`

**Implementation Details**:
- **Environment Variable Compliance**:
  - `NO_COLOR=1`: Disables all ANSI output (https://no-color.org/)
  - `CLICOLOR_FORCE=1`: Forces colors even in non-TTY
  - `CLICOLOR=0`: Disables colors on macOS
  - `TERM=dumb`: Basic mode (colors without cursor positioning)

- **Three-Tier Terminal Mode System**:
  - `TerminalMode::Full`: Full ANSI support including cursor positioning, colors
  - `TerminalMode::Basic`: Basic TTY with colors but no cursor positioning
  - `TerminalMode::None`: Non-TTY output (pipes, redirects, CI environments)

- **Accessibility Support**:
  - `TerminalMode::Basic` for screen readers (static output without cursor positioning)
  - Respects standard environment variables for accessibility

**Test Coverage**: Lines 211-324

---

#### ✅ Phase 1.3: Heartbeat with Accessibility Mode (P1)

**Status**: Fully Implemented

**Location**: `ralph-workflow/src/json_parser/terminal.rs:32-46`

**Implementation Details**:
- The `TerminalMode::Basic` variant provides accessibility support for:
  - Screen readers (no cursor positioning animations)
  - Non-TTY environments (pipes, redirects)
  - `TERM=dumb` terminals
- Static progress output instead of animated spinners in Basic mode

**Related**: This is part of the terminal mode detection system above.

---

#### ✅ Phase 7.3: Streaming Quality Metrics (P3)

**Status**: Fully Implemented

**Location**: `ralph-workflow/src/json_parser/streaming_state.rs:986-1005`

**Implementation Details**:
- **Delta Size Tracking**: Tracks individual delta sizes across all content types
- **Snapshot-as-Delta Detection**: Detects when agents send full accumulated content as "deltas"
- **Protocol Violation Tracking**: Counts non-standard agent protocol events (e.g., repeated `MessageStart`)
- **Environment Variable Configuration**:
  - `RALPH_STREAMING_SNAPSHOT_THRESHOLD`: Threshold for detecting violations (default: 200, range: 50-1000)
  - `RALPH_STREAMING_FUZZY_MATCH_RATIO`: Fuzzy detection ratio (default: 85, range: 50-95)
- **Metrics Fields**:
  - `total_deltas`: Total number of deltas processed
  - `min_delta_size`, `max_delta_size`, `avg_delta_size`: Size statistics
  - `snapshot_repairs_count`: Number of snapshot-as-delta repairs performed
  - `large_delta_count`: Number of deltas exceeding threshold
  - `protocol_violations`: Number of protocol violations detected

**Test Coverage**: Lines 1834-1940

---

#### ✅ Phase 6.1: Enhanced Diagnostics (P2)

**Status**: Fully Implemented

**Locations**:
- `ralph-workflow/src/diagnostics/mod.rs:1-31`
- `ralph-workflow/src/cli/handlers/diagnose.rs:1-349`

**Implementation Details**:
- **System Information Gathering** (`diagnostics/system.rs`):
  - OS, architecture, working directory
  - Shell detection (`SHELL` environment variable)
  - Git version and repository status
  - Current branch and uncommitted changes

- **Agent Availability Testing** (`diagnostics/agents.rs`):
  - Tests all configured agents for availability
  - Reports display names, JSON parsers, and commands
  - Provides clear ✓/✗ status indicators

- **Configuration Validation**:
  - Unified config file existence and path
  - Review depth configuration
  - Legacy global agents.toml detection
  - Loaded configuration sources

- **Project Stack Detection** (`diagnose.rs:263-329`):
  - Primary and secondary language detection
  - Framework detection
  - Package manager detection
  - Test framework detection
  - Language type indicators (Rust, Python, JS/TS, Go)
  - Review guidelines summary with severity breakdown

- **PROMPT.md Validation** (`diagnose.rs:215-240`):
  - File existence and size
  - Line count
  - Goal section detection
  - Acceptance criteria section detection

- **Checkpoint Status** (`diagnose.rs:242-261`):
  - Checkpoint file existence
  - Phase, iteration, and agent information
  - Interrupted run detection

- **Recent Log Display** (`diagnose.rs:331-348`):
  - Last 10 entries from `.agent/logs/pipeline.log`

**Usage**: `ralph --diagnose`

---

#### ✅ Phase 5.0: Color Standardization (P1)

**Status**: Partially Implemented

**Location**: `ralph-workflow/src/logger/mod.rs:35-134`

**Implementation Details**:
- **ANSI 4-Bit Color Support**: Full color palette (red, green, yellow, blue, magenta, cyan, white)
- **Style Codes**: Bold, dim, reset
- **NO_COLOR Compliance**: Respects `NO_COLOR` environment variable (line 37)
- **TTY Detection**: Automatically disables colors when stdout is not a terminal

**Missing from Original Proposal**:
- CLICOLOR and CLICOLOR_FORCE support in `logger/mod.rs` (only in `terminal.rs`)
- Semantic color enum (currently using direct ANSI codes)

**Note**: The `terminal.rs` module has full CLICOLOR/CLICOLOR_FORCE support, but the `logger` module only implements NO_COLOR.

---

#### ✅ Phase 8: Template Listing (P2)

**Status**: Fully Implemented

**Locations**:
- `ralph-workflow/src/templates/mod.rs:113`
- `ralph-workflow/src/cli/init.rs:188-215`

**Implementation Details**:
- `list_templates()` function returns available templates with descriptions
- `handle_list_templates()` displays formatted template list
- Usage: `ralph --list-templates`
- Templates include: feature-spec, bug-fix, refactor, blank, context

**Integration**:
- Referenced in error messages when PROMPT.md is missing
- Available in `--init-prompt` command flow
- Documented in `--help` output

---

### In Progress Features

#### ✅ Phase 5.1: Full Environment Variable Compliance

**Status**: Fully Implemented

**Location**: `ralph-workflow/src/logger/mod.rs:35-77`

**Implementation Details**:
- **Environment Variable Compliance** (now consistent across all modules):
  - `NO_COLOR=1`: Disables all ANSI output (<https://no-color.org/>)
  - `CLICOLOR_FORCE=1`: Forces colors even in non-TTY
  - `CLICOLOR=0`: Disables colors on macOS
  - `TERM=dumb`: Disables colors for basic terminals

- **Priority Order**:
  1. `NO_COLOR` (highest priority - takes precedence)
  2. `CLICOLOR_FORCE` (forces colors even in non-TTY)
  3. `CLICOLOR` (macOS color disable)
  4. `TERM` (dumb terminal check)
  5. TTY detection (fallback)

- **Consistency**: The `logger/mod.rs` module now matches the comprehensive environment variable detection logic from `terminal.rs`, ensuring consistent color behavior across the entire application.

**Test Coverage**: Lines 188-310 (7 new tests added):
- `test_colors_enabled_respects_no_color`
- `test_colors_enabled_respects_clicolor_force`
- `test_colors_enabled_respects_clicolor_zero`
- `test_colors_enabled_respects_term_dumb`
- `test_colors_enabled_no_color_takes_precedence`
- `test_colors_enabled_term_dumb_case_insensitive`

---

### Not Started Features

#### ⏳ Phase 1.1: Immediate Feedback (100ms Rule) - P0

**Status**: Not Started

**Proposal**: Print "Starting..." within 100ms before agent calls

**Rationale**: Addresses critical "Is it stuck?" friction point

**Implementation Notes**:
- **Location**: `ralph-workflow/src/app/mod.rs:97-100` (after agent resolution)
- **Existing Code Pattern**: The codebase already has 150+ logger.info/success/warn/error calls
  - Review phase: 60+ feedback messages in `phases/review/prompt.rs` and `phases/review/validation.rs`
  - Commit phase: 50+ feedback messages in `phases/commit.rs`
  - Development phase: 15+ feedback messages in `phases/development.rs`
- **Exact Integration Point**:
  ```rust
  // EXISTING CODE (lines ~97-100)
  let validated = resolve_required_agents(&config)?;
  let developer_agent = validated.developer_agent;
  let reviewer_agent = validated.reviewer_agent;

  // Get display names for UI/logging (already exists at line 103-104)
  let developer_display = registry.display_name(&developer_agent);
  let reviewer_display = registry.display_name(&reviewer_agent);

  // ADD HERE: Immediate feedback (after line 104)
  logger.info(&format!(
      "Starting pipeline with {} (dev) → {} (review)...",
      developer_display, reviewer_display
  ));
  ```
- **Test**: Run `ralph` and verify message appears before first agent call (manual test)
- **Dependencies**: None (uses existing Logger infrastructure)

---

#### 🔄 Phase 1.2: Pipeline Phase Indicator - P0

**Status**: Partially Implemented

**Location**: `ralph-workflow/src/logger/progress.rs:1-123`

**Implementation Details**:
- **Progress Bar Function**: `print_progress()` already exists
  - Displays visual progress bar: `[████████░░░░░░░░░] 60% (3/5)`
  - Used in development phase at `phases/development.rs:63`
  - Shows percentage, filled bar with block characters, and current/total counts
- **Integration Point**: Already called in development phase iteration loop

**Missing from Original Proposal**:
- **Agent Name Display**: Progress bar doesn't show which agent is running
- **Elapsed Time**: No time tracking/ETA display (proposal showed `2m 34s`)
- **Phase Label**: No "[Development]" prefix in progress display
- **Multi-Phase Progress**: No simultaneous progress for multiple phases
- **Animated Updates**: Progress bar is static (printed once per iteration, not updated in-place)

**Suggested Enhancement**: Use `indicatif` crate for advanced features
- Add dependency: `indicatif = "0.17"` to `Cargo.toml`
- Provides in-place updates, elapsed time, ETA calculation
- Supports multi-progress bars for concurrent phases
- Integrates with existing `TerminalMode` for accessibility

**Current Output**:
```
Overall: [████████████░░░░░░░░░] 60% (3/5)
```

**Proposed Output** (with indicatif):
```
[Development 3/5] claude ━━━━━━━━━━━━━━━━ 2m 34s
```

**Dependencies**: Phase 1.1 (for consistent feedback pattern)

---

#### ⏳ Phase 2.1: First-Run Detection - P0

**Status**: Not Started

**Proposal**: Auto-detect first run and offer guided setup

**Current Behavior**: Requires manual `--init-global` + `--init-prompt`

**Implementation Notes**:
- **Detection Location**: `ralph-workflow/src/app/mod.rs:86` (after `initialize_config()`)
- **Detection Logic**:
  ```rust
  let config_path = unified_config_path();
  let is_first_run = !config_path.exists()
      || !legacy_config_path().exists()
      || !prompt_path.exists();
  ```
- **Existing Functions to Reference**:
  - `handle_init_global()` at `cli/init.rs:27-67` - config creation pattern
  - `prompt_template_selection()` at `cli/handlers/template_selection.rs` - interactive selection
  - `list_templates()` at `templates/mod.rs:113` - available templates
- **Flow Integration**: Insert between lines 86-99 in `app/mod.rs`
- **Interactive Check**: Use `std::io::stdin().is_terminal()` to detect interactive mode
- **Example**:
  ```rust
  if is_first_run && std::io::stdin().is_terminal() {
      println!("Welcome to Ralph Workflow!");
      println!("It looks like this is your first time running Ralph.");
      println!("Would you like to run the setup wizard? [Y/n]");
      // ... read input and optionally call setup wizard
  }
  ```
- **Dependencies**: None (can use existing template selection infrastructure)

---

#### ⏳ Phase 2.2: `ralph setup` Command - P1

**Status**: Not Started

**Proposal**: Interactive setup wizard for configuration

**Flow**: Agent detection → Verification → Prompt template selection

---

#### ⏳ Phase 3.1: Actionable Error Messages - P0

**Status**: Not Started

**Proposal**: Include copy-pasteable fix commands in every error

**Current State**: Error classification exists (`agents/error.rs`) but advice is prose, not actionable commands

**Implementation Notes**:
- **Location**: `ralph-workflow/src/agents/error.rs:36-65` (existing `AgentErrorKind` enum)
- **Existing Pattern**: `recovery_advice()` method already exists but returns prose
- **Extension Strategy**: Add `actionable_advice()` method returning structured commands
- **Example Implementation**:
  ```rust
  pub struct ActionableAdvice {
      pub message: &'static str,
      pub fix_commands: Vec<(&'static str, &'static str)>, // (description, command)
      pub docs_link: Option<&'static str>,
      pub diagnostic_command: Option<&'static str>,
  }

  impl AgentErrorKind {
      pub fn actionable_advice(&self) -> ActionableAdvice {
          match self {
              Self::CommandNotFound => ActionableAdvice {
                  message: "Agent binary not found",
                  fix_commands: vec![
                      ("Install Claude Code", "npm install -g @anthropic-ai/claude-code"),
                      ("Verify PATH", "echo $PATH"),
                  ],
                  docs_link: Some("docs/agents.md#installation"),
                  diagnostic_command: Some("ralph --list-available-agents"),
              },
              // ... other cases
          }
      }
  }
  ```
- **Display Pattern**: Use existing `Logger::error()` and format with `Colors` for structure
- **Integration Point**: Where errors are displayed in pipeline execution
- **Dependencies**: None (extends existing error classification)

---

#### ⏳ Phase 3.2: "Did You Mean?" Suggestions - P1

**Status**: Not Started

**Proposal**: Fuzzy matching for typos in agent names

**Example**: `Unknown agent 'cluade' - Did you mean 'claude'?`

---

#### 🔄 Phase 4.0: Action-Reaction Feedback - P0

**Status**: Partially Implemented

**Location**: `ralph-workflow/src/phases/development.rs:58-63`

**Implementation Details**:
- **Iteration Feedback**: Already implemented in development phase
  - `logger.subheader()` displays "Iteration X of Y" (development.rs:59-62)
  - `print_progress()` shows visual progress bar (development.rs:63)
  - Example output: `[████████████░░░░░░░░░] 60% (3/5)`
- **Progress Bar Infrastructure**: `logger/progress.rs:1-123`
  - Displays visual bar with percentage and counts
  - Format: `[████████░░░░░░░░░] 50% (5/10)`
  - Handles edge cases (zero total, overflow protection)

**Missing from Original Proposal**:
- **Agent Start Feedback**: No "Starting pipeline..." message before first agent call
- **Phase Transition Feedback**: No "Switching to review phase..." message
- **Completion Summary**: No explicit "Pipeline completed successfully" beyond banner
- **Ctrl+C Hint**: No cancellation hint during long operations (Phase 8.3)

**Existing Logger Usage Throughout Codebase**:
- Review phase: 60+ feedback messages in `phases/review/prompt.rs` and `phases/review/validation.rs`
- Commit phase: 50+ feedback messages in `phases/commit.rs`
- Development phase: 15+ feedback messages in `phases/development.rs`
- App orchestration: 30+ messages in `app/mod.rs`

**Integration Points for Missing Feedback**:
- **Agent Start**: `app/mod.rs:100` (after agent resolution)
- **Phase Transitions**: Between phase calls in main pipeline
- **Ctrl+C Hint**: Display alongside initial feedback (Phase 8.3)

---

#### ⏳ Phase 4.1: Watch Mode - P2

**Status**: Not Started

**Proposal**: Monitor PROMPT.md for changes and auto-run

**Command**: `ralph --watch`

---

#### ⏳ Phase 4.2: Post-Run Actions Menu - P2

**Status**: Not Started

**Proposal**: Interactive menu after pipeline completion (view diff, edit prompt, run again, push)

---

#### ⏳ Phase 4.3: Confirmation for Destructive Operations - P2

**Status**: Not Started

**Proposal**: Confirmation prompts for operations that modify git history

---

#### ⏳ Phase 6.1: Shell Completions - P1

**Status**: Not Started

**Proposal**: Generate shell completions for bash, zsh, fish

**Commands**:
```bash
ralph --completions bash > /etc/bash_completion.d/ralph
ralph --completions zsh > ~/.zfunc/_ralph
ralph --completions fish > ~/.config/fish/completions/ralph.fish
```

---

#### ⏳ Phase 6.2: Desktop Notifications - P3

**Status**: Not Started

**Proposal**: Notify when long-running pipelines complete

**Command**: `ralph --notify "feat: change"`

---

#### ⏳ Phase 7.1: Run History - P2

**Status**: Not Started

**Proposal**: Track runs in `.agent/history.json`

**Command**: `ralph history`

---

#### ⏳ Phase 7.2: Replay Command - P3

**Status**: Not Started

**Proposal**: Re-run with same settings

**Command**: `ralph replay run_abc123`

---

#### ⏳ Phase 8.1: `ralph status` Command - P1

**Status**: Not Started

**Proposal**: Show current `.agent` state

**Command**: `ralph status`

---

#### ⏳ Phase 8.2: `ralph clean` Command - P1

**Status**: Not Started

**Proposal**: Reset `.agent` directory with confirmation

**Command**: `ralph clean`

---

#### ⏳ Phase 8.3: Cancellation Hint (Ctrl+C) - P0

**Status**: Not Started

**Proposal**: Display "Press Ctrl+C to cancel" during long operations

**Implementation Notes**:
- **Location**: `ralph-workflow/src/app/mod.rs` (immediately after Phase 1.1 feedback)
- **Display Point**: Same location as Phase 1.1, right after the "Starting pipeline..." message
- **Exact Integration**:
  ```rust
  // Phase 1.1 message
  logger.info(&format!(
      "Starting pipeline with {} (dev) → {} (review)...",
      developer_display, reviewer_display
  ));

  // ADD HERE: Cancellation hint
  if std::io::stdin().is_terminal() {
      logger.warn("Press Ctrl+C to cancel (checkpoint will be saved)");
  }
  ```
- **Interactive Mode Check**: `std::io::stdin().is_terminal()` ensures hint only shows in interactive terminals
- **Existing Pattern**: Similar to checkpoint hints already in codebase
  - Example: `logger.warn("Press Ctrl+C to cancel")` pattern used in other CLI tools
- **Color Scheme**: Use `logger.warn()` which already applies yellow warning color
- **Dependencies**: Phase 1.1 (to display alongside initial feedback)

---

#### ⏳ Phase 1.4: Estimated Time Remaining - P3

**Status**: Not Started

**Proposal**: Track historical run times and display ETA

**Storage**: `.agent/metrics.json`

---

### Codebase Analysis: Logger Usage Patterns

**Analysis Date**: 2026-01-17

Comprehensive analysis of logger usage across the codebase reveals extensive feedback infrastructure already in place:

---

### Immediate Wins: Next Steps for Progress

This section provides **copy-pasteable implementation steps** for the three easiest P0 features that can be completed in under 1 hour total using existing infrastructure.

#### 🚀 Quick Win #1: Phase 1.1 - Immediate Feedback (15 minutes)

**Priority**: P0 | **Effort**: 15 minutes | **Impact**: High

**Implementation**: Add one feedback message at the start of the pipeline.

**File**: `ralph-workflow/src/app/mod.rs`

**Location**: After line 104 (after display names are retrieved)

```rust
// ADD THIS CODE after line 104:
// EXISTING CODE shows developer_display and reviewer_display are already available
let developer_display = registry.display_name(&developer_agent);
let reviewer_display = registry.display_name(&reviewer_agent);

// IMMEDIATE FEEDBACK (Phase 1.1) - Add these two lines:
logger.info(&format!(
    "Starting pipeline with {} (dev) → {} (review)...",
    developer_display, reviewer_display
));

if std::io::stdin().is_terminal() {
    logger.warn("Press Ctrl+C to cancel (checkpoint will be saved)");
}
```

**Why it's easy**:
- Single code addition at a well-defined location
- Uses existing Logger infrastructure (150+ logger calls already in codebase)
- No new dependencies or infrastructure needed
- Immediately visible impact when running `ralph`

**Testing**: Run `ralph` and verify the message appears before any agent calls.

---

#### 🚀 Quick Win #2: Phase 8.3 - Cancellation Hint (15 minutes)

**Priority**: P0 | **Effort**: 15 minutes | **Impact**: Medium

**Implementation**: Display cancellation hint during long operations.

**Note**: This is already included in Quick Win #1 above (the `logger.warn()` call). If implementing separately:

**File**: `ralph-workflow/src/app/mod.rs`

**Location**: After line 104 (immediately after Phase 1.1 feedback)

```rust
// ADD THIS CODE after Phase 1.1 message:
if std::io::stdin().is_terminal() {
    logger.warn("Press Ctrl+C to cancel (checkpoint will be saved)");
}
```

**Why it's easy**:
- Single conditional check
- Uses existing `logger.warn()` method
- Gate on `is_terminal()` ensures it only shows in interactive mode
- Pattern already used in similar CLI tools

**Testing**: Run `ralph` in an interactive terminal and verify the warning appears.

---

#### 🚀 Quick Win #3: Phase 4.0 - Phase Transition Feedback (30 minutes)

**Priority**: P0 | **Effort**: 30 minutes | **Impact**: High

**Implementation**: Add feedback messages at key pipeline transition points.

**File**: `ralph-workflow/src/app/mod.rs`

**Locations**: Multiple locations in main pipeline flow

```rust
// ADD THESE CODE blocks at appropriate transition points:

// 1. AFTER DEVELOPMENT PHASE COMPLETES:
// Location: After development phase execution
logger.success("✓ Development phase complete");

// 2. BEFORE REVIEW PHASE STARTS:
// Location: Before review phase execution
logger.info("Switching to review phase...");

// 3. AFTER REVIEW PHASE COMPLETES:
// Location: After review phase execution
logger.success("✓ Review phase complete");

// 4. AFTER COMMIT PHASE COMPLETES:
// Location: After commit phase execution (or at pipeline end)
logger.success("✓ Pipeline completed successfully");
```

**Why it's easy**:
- Leverages existing Logger methods (success, info)
- Multiple small wins throughout the pipeline
- Pattern matches existing 71 logger calls in `app/mod.rs`
- No new infrastructure required

**Testing**: Run `ralph` and verify phase transition messages appear between phases.

---

### Summary: Total Effort for All Three Quick Wins

| Feature | Effort | Files Modified | New Dependencies |
|---------|--------|----------------|------------------|
| Phase 1.1: Immediate Feedback | 15 min | 1 (`app/mod.rs`) | None |
| Phase 8.3: Cancellation Hint | 15 min | 1 (`app/mod.rs`) | None |
| Phase 4.0: Phase Transitions | 30 min | 1 (`app/mod.rs`) | None |
| **Total** | **1 hour** | **1 file** | **0** |

**All three features can be implemented in a single PR that modifies only one file (`app/mod.rs`).**

---

### Example Pull Request

**Title**: `feat(ux): [RFC-002] Add immediate feedback and phase transition messages`

**Description**:
```
This PR implements three P0 features from RFC-002:

1. **Phase 1.1**: Immediate feedback before pipeline starts
   - Shows which agents are being used
   - Addresses "Is it stuck?" friction point

2. **Phase 8.3**: Cancellation hint during execution
   - Shows "Press Ctrl+C to cancel" in interactive terminals
   - Provides clear way out for long operations

3. **Phase 4.0**: Phase transition feedback
   - Shows completion of each phase
   - Shows transitions between phases
   - Shows final pipeline completion

All features use existing Logger infrastructure and require no new
dependencies. Total effort: ~1 hour.

Resolves: RFC-002 Phase 1.1, Phase 4.0, Phase 8.3
```

**Files Changed**:
- `ralph-workflow/src/app/mod.rs` (+8 lines)

---

### Next Steps After Quick Wins

After completing these three quick wins (1 hour total), the next easiest features are:

1. **Phase 2.1**: First-Run Detection (2-3 hours)
2. **Phase 3.1**: Actionable Error Messages (3-4 hours)
3. **Phase 1.2**: Enhanced Progress Bar (4-6 hours, requires `indicatif` dependency)

These quick wins provide immediate user value with minimal effort and establish patterns for the remaining features.

---

#### Logger Call Statistics (337 total calls across 21 files)

| File | Logger Calls | Notes |
|------|--------------|-------|
| `phases/commit.rs` | 78 | High - Commit phase feedback |
| `phases/review/validation.rs` | 35 | Medium - Validation feedback |
| `phases/review/prompt.rs` | 12 | Low - Prompt construction |
| `phases/development.rs` | 18 | Medium - Development iteration feedback |
| `phases/review.rs` | 24 | Medium - Review phase feedback |
| `phases/integrity.rs` | 7 | Low - Integrity checks |
| `app/mod.rs` | 71 | **High** - Main orchestration |
| `pipeline/runner.rs` | 10 | Low - Pipeline execution |
| `pipeline/fallback.rs` | 10 | Low - Fallback handling |
| `pipeline/prompt.rs` | 14 | Low - Pipeline prompts |
| `app/resume.rs` | 6 | Low - Resume functionality |
| `app/plumbing.rs` | 11 | Low - Plumbing commands |
| `app/detection.rs` | 2 | Minimal - Agent detection |
| `app/config_init.rs` | 1 | Minimal - Config initialization |
| `app/finalization.rs` | 2 | Minimal - Finalization |
| `cli/handlers/dry_run.rs` | 15 | Low - Dry run feedback |
| `git_helpers/wrapper.rs` | 2 | Minimal - Git wrapper |
| `git_helpers/hooks.rs` | 5 | Low - Git hooks |
| `files/io/context.rs` | 9 | Low - File I/O context |
| `banner.rs` | 1 | Minimal - Banner display |
| `logger/mod.rs` | 4 | Minimal - Internal logger |

#### Progress Bar Integration Points

**Current Usage**:
- `logger/progress.rs:16-66` - `print_progress()` function
- `phases/development.rs:63` - Development iteration progress
- `phases/review.rs:114` - Review cycle progress

**Function Signature**:
```rust
pub fn print_progress(current: u32, total: u32, label: &str)
```

**Output Format**:
```
Overall: [████████████░░░░░░░░░] 60% (3/5)
```

**Implementation Details**:
- Bar width: 20 characters
- Uses block characters (`█` for filled, `░` for empty)
- Handles edge cases (zero total, overflow protection)
- Automatic color application via `Colors` infrastructure

#### Display Name Registry Usage

**Current Usage** (8 locations):
- `app/mod.rs:103-104` - Developer/reviewer agent display names
- `pipeline/runner.rs:312` - Agent display in pipeline
- `diagnostics/agents.rs:39` - Agent diagnostics
- `cli/handlers/list.rs:29, 41, 67, 76` - Agent listing
- `cli/handlers/diagnose.rs:201` - Diagnostics output

**Registry Method**:
```rust
registry.display_name(&agent_name) // Returns human-readable name
```

**Examples**:
- `"claude"` → `"claude"`
- `"ccs/glm"` → `"ccs-glm"`
- `"unknown"` → `"unknown"`

#### Key Integration Points for RFC-002 Features

**Phase 1.1 & 8.3 (Immediate Feedback + Ctrl+C Hint)**:
- **File**: `app/mod.rs:103-104` (after display names retrieved)
- **Existing Pattern**: Logger methods already used throughout
- **Integration**: Add `logger.info()` and `logger.warn()` calls

**Phase 4.0 (Action-Reaction Feedback)**:
- **Files**: Multiple locations in `app/mod.rs`, `phases/development.rs`, `phases/review.rs`
- **Existing Pattern**: 71 logger calls in `app/mod.rs` alone
- **Integration**: Add phase transition messages using existing patterns

**Phase 1.2 (Enhanced Progress Bar)**:
- **File**: `logger/progress.rs` (existing function)
- **Current Usage**: 2 locations (development, review phases)
- **Enhancement**: Add `indicatif` crate for time tracking and agent names

**Phase 3.1 (Actionable Error Messages)**:
- **File**: `agents/error.rs:36-188` (existing error classification)
- **Existing Pattern**: `recovery_advice()` method returns prose
- **Enhancement**: Add `actionable_advice()` method with structured commands

---

### Summary Statistics

| Priority Level | Total Items | Completed | In Progress | Partial | Not Started |
|----------------|-------------|-----------|-------------|---------|-------------|
| P0 (Critical) | 6 | 0 | 0 | 2 | 4 |
| P1 (High) | 8 | 4 | 0 | 0 | 4 |
| P2 (Medium) | 4 | 2 | 0 | 0 | 2 |
| P3 (Lower) | 4 | 1 | 0 | 0 | 3 |
| **Total** | **22** | **7 (32%)** | **0 (0%)** | **2 (9%)** | **13 (59%)** |

**Overall Completion**: 32% (7 fully completed + 2 partially completed of 22 items)

**Completed Features**: Terminal standards, diagnostics, streaming metrics, environment variable compliance, template listing
**Partially Completed**:
- Phase 1.2 (Pipeline Phase Indicator) - has basic progress bar, missing agent name/elapsed time
- Phase 4.0 (Action-Reaction Feedback) - has iteration feedback in development phase, missing pipeline start/transition feedback

**Note**: The completed features are substantial infrastructure improvements (terminal standards, diagnostics, streaming metrics) that provide a solid foundation for remaining user-facing features. Two P0 items have partial implementation that can be enhanced with relatively small effort.

### Recent Documentation Updates

**2026-01-16**: Status update and date refresh
- Updated "Last Updated" timestamps to current date (2026-01-16)
- Verified completion percentages remain accurate at 32% overall completion
- Confirmed 7 fully completed features and 2 partially completed features
- All implementation notes and code references remain current

**2026-01-16**: Added "Immediate Wins" section for quick implementation guidance
- Added new "Immediate Wins: Next Steps for Progress" section with copy-pasteable code snippets
- Documented three easiest P0 features (Phase 1.1, 8.3, 4.0) that can be completed in 1 hour total
- Provided exact file locations and line numbers for all integration points
- Included example PR description and file change summary
- Added "Next Steps After Quick Wins" to guide contributors to subsequent features

**2026-01-17**: Enhanced codebase analysis and implementation guidance
- Added comprehensive "Codebase Analysis: Logger Usage Patterns" section documenting **337 logger calls** across 21 files
- Updated "Quick-Start Implementation Guide" with accurate file locations and call counts
- Enhanced "Key File Locations Reference" with detailed logger call statistics
- Documented progress bar integration points and display name registry usage
- Added specific line numbers for all integration points

This analysis provides contributors with:
1. **Exact file locations** for all RFC-002 integration points
2. **Logger call counts** by file (helps identify high-feedback areas)
3. **Existing patterns** already in use (71 calls in app/mod.rs alone)
4. **Progress bar usage** at 2 locations with enhancement path
5. **Display name registry** usage at 8 locations

---

## Motivation

Ralph Workflow is a sophisticated multi-agent orchestrator with strong fundamentals:
- Well-structured CLI with presets and aliases
- Comprehensive error classification and recovery
- Good documentation and template system
- Colorized output with box-drawing characters

However, several UX gaps exist that create friction:

| Area | Current State | Impact |
|------|---------------|--------|
| Progress visibility | No phase/iteration indicators during runs | Users unsure if pipeline is stuck |
| First-run experience | Requires manual `--init-global` + `--init-prompt` | New users fail on first attempt |
| Error messages | Classification exists but lacks copy-pasteable fixes | Users must search docs for solutions |
| Long-running feedback | No time estimates or completion hints | Poor UX during 10+ minute runs |
| Post-completion | Summary only, no guided next steps | Users unsure what to do next |

These gaps affect both new users (onboarding friction) and power users (productivity during long workflows).

---

## Industry Best Practices & Comparisons

### Core CLI Design Principles (from [clig.dev](https://clig.dev/))

| Principle | Description | Ralph Status |
|-----------|-------------|--------------|
| **Human-first** | CLIs should prioritize humans over automation | ✅ Good |
| **Conversation as norm** | Interaction is a repeated loop with feedback | ⚠️ Partial |
| **Responsiveness** | Print something within 100ms before network calls | ❌ Missing |
| **Lead with examples** | Help text shows examples before dry explanation | ✅ Good |
| **Suggest next commands** | Guide users through workflows | ❌ Missing |
| **Rewrite errors for humans** | Suggest fixes, not just describe failures | ⚠️ Partial |
| **Progressive disclosure** | Concise default help, detailed on request | ✅ Good |

### Atlassian's 10 Design Principles for Delightful CLIs

| # | Principle | Ralph Implementation |
|---|-----------|---------------------|
| 1 | Align with conventions | ✅ Uses standard flag patterns |
| 2 | Build `--help` into CLI | ✅ Comprehensive help |
| 3 | Show progress visually | ❌ No progress indicators |
| 4 | Reaction for every action | ⚠️ Summary only at end |
| 5 | Human-readable errors | ⚠️ Advice exists but not actionable |
| 6 | Support skim-readers | ✅ Good use of formatting |
| 7 | Suggest next best step | ❌ No suggestions |
| 8 | Consider your options | ⚠️ No prompts for missing options |
| 9 | Provide easy way out | ❌ No Ctrl+C hint |
| 10 | Flags over args | ✅ Good flag usage |

### Production CLI Tool Comparisons

| Tool | Strength to Emulate |
|------|---------------------|
| **GitHub CLI (`gh`)** | Interactive prompts for missing inputs, accessibility-first design, context-aware commands |
| **cargo** | Multi-progress bars for parallel operations (via `indicatif`), consistent colorization |
| **npm/yarn** | Visual spinners during network operations, clear phase separation |
| **lazygit** | Information-dense TUI that remains readable, keyboard shortcuts displayed |
| **Warp** | Block-based command history, command suggestions |

### Accessibility Considerations (from [GitHub CLI accessibility work](https://github.blog/engineering/user-experience/building-a-more-accessible-github-cli/))

Ralph should consider:

1. **Screen reader compatibility**: Avoid spinner animations that cause screen redraws; use static "Working..." messages
2. **ANSI 4-bit colors**: Use terminal color table for user customization rather than hardcoded RGB
3. **High contrast mode**: Ensure critical info visible without relying on color alone
4. **Non-TTY fallback**: All interactive features disabled cleanly in scripts

---

## Current State Analysis

### CLI Entry Point (`cli/args.rs`)

**Strengths**:
- Comprehensive flag set with short/long forms
- Good preset system (-Q/-U/-S/-T/-L)
- After-help section with quick examples
- Environment variable support

**Gaps**:
- No `ralph setup` or first-run wizard
- No shell completion generation
- Hidden flags reduce discoverability
- No subcommand structure for plumbing

### Logging & Output (`logger/`, `banner.rs`)

**Strengths**:
- Color support with `NO_COLOR` compliance
- Box-drawing characters for visual structure
- Icon set (check, cross, warning, info, arrow)
- Summary display with review metrics

**Gaps**:
- No progress bar during iterations
- No phase indicator showing position in pipeline
- No elapsed/remaining time display
- No spinner during silent processing

### Error Handling (`agents/error.rs`)

**Strengths**:
- 14 error kinds with classification
- `should_retry()`, `should_fallback()`, `is_unrecoverable()` methods
- `recovery_advice()` for each error type
- Suggested wait times for retries

**Gaps**:
- Advice is prose, not actionable commands
- No "did you mean?" suggestions
- No direct links to documentation
- No copy-pasteable fix commands

### Diagnostics (`diagnostics/`)

**Strengths**:
- System info gathering
- Agent availability testing
- Structured `DiagnosticReport`

**Gaps**:
- No config validation command
- No history/replay functionality
- No cost/token tracking

---

## Proposed Changes

### Phase 1: Progress Visualization

**Design Principle**: "Show progress visually" (Atlassian #3), "Visibility of system status" (Nielsen), "Print something within 100ms" (clig.dev)

#### 1.1 Immediate Feedback (100ms Rule)

Per [clig.dev](https://clig.dev/), print something within 100ms before any network/agent call:

```
Starting development phase with claude...
```

This addresses the critical "Is it stuck?" friction point.

#### 1.2 Pipeline Phase Indicator

Inspired by cargo's multi-progress and npm's phase display:

```
┌─ Development ─────────────────────────────────────────────────┐
│ [3/5] claude                                          2m 34s  │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━░░░░░░░░░░░░░░░░░░░ 60%    │
└───────────────────────────────────────────────────────────────┘
```

Alternative compact format for terminals with limited height:

```
[Development 3/5] claude ━━━━━━━━━━━━━━━━░░░░░░ 2m 34s
```

Implementation using `indicatif` (cargo's progress library):

```rust
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};

pub struct PipelineProgress {
    multi: MultiProgress,
    phase_bar: ProgressBar,
    spinner: ProgressBar,
}

impl PipelineProgress {
    pub fn new(total_iterations: u64) -> Self {
        let multi = MultiProgress::new();

        let phase_bar = multi.add(ProgressBar::new(total_iterations));
        phase_bar.set_style(ProgressStyle::with_template(
            "[{prefix}] {bar:40.cyan/dim} {pos}/{len} {elapsed_precise}"
        ).unwrap());

        let spinner = multi.add(ProgressBar::new_spinner());
        spinner.set_style(ProgressStyle::with_template(
            "{spinner:.cyan} {msg}"
        ).unwrap());

        Self { multi, phase_bar, spinner }
    }

    pub fn set_phase(&self, phase: &str, agent: &str) {
        self.phase_bar.set_prefix(format!("{} {}", phase, agent));
    }

    pub fn tick(&self, message: &str) {
        self.spinner.set_message(message.to_string());
        self.spinner.tick();
    }
}
```

#### 1.3 Heartbeat Indicator with Accessibility Mode

**Standard mode** (TTY with animation support):
```
⠹ Waiting for claude response...
```

**Accessible mode** (screen readers, per GitHub CLI guidance):
```
Working... (press Ctrl+C to cancel)
```

Implementation with accessibility detection:

```rust
pub struct Heartbeat {
    accessible_mode: bool,
    frames: &'static [char],  // ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']
    current: usize,
}

impl Heartbeat {
    pub fn new() -> Self {
        // Detect if screen reader mode should be used
        let accessible_mode = std::env::var("RALPH_ACCESSIBLE").is_ok()
            || std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false);

        Self {
            accessible_mode,
            frames: &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
            current: 0,
        }
    }

    pub fn render(&mut self, message: &str) -> String {
        if self.accessible_mode {
            format!("Working... {}", message)
        } else {
            self.current = (self.current + 1) % self.frames.len();
            format!("{} {}", self.frames[self.current], message)
        }
    }
}

#### 1.3 Estimated Time Remaining

Track historical run times in `.agent/metrics.json`:

```json
{
  "runs": [
    {
      "date": "2026-01-15T10:30:00Z",
      "preset": "standard",
      "dev_agent": "claude",
      "total_duration_ms": 342000,
      "phases": {
        "development": 180000,
        "review": 120000,
        "commit": 42000
      }
    }
  ]
}
```

Display ETA based on historical averages:
```
[Development 3/5] claude ━━━━━━━━━━━━━━━━ 2m 34s (ETA: ~4m remaining)
```

### Phase 2: First-Run Onboarding

**Design Principles**: "Progressive discovery" (clig.dev), "Consider your options" (Atlassian #8), "Interactive usage modes for discoverability" ([Lucas Costa](https://lucasfcosta.com/2022/06/01/ux-patterns-cli-tools.html))

The goal is **zero-to-working in under 2 minutes** for new users.

#### 2.1 Auto-Detect First Run

When no config file exists, offer guided setup:

```rust
fn detect_first_run() -> bool {
    !config_path().exists() && !legacy_config_path().exists()
}

fn offer_guided_setup() -> io::Result<()> {
    println!("Welcome to Ralph Workflow!");
    println!();
    println!("It looks like this is your first time running Ralph.");
    println!("Would you like to run the setup wizard? [Y/n]");
    // ...
}
```

#### 2.2 `ralph setup` Command

Add an explicit setup subcommand:

```bash
ralph setup
```

Interactive flow:
```
Welcome to Ralph Workflow Setup!

Step 1/4: Configuration File
  Creating ~/.config/ralph-workflow.toml... done

Step 2/4: Agent Detection
  Scanning for installed agents...
  Found: claude (Claude Code), codex (Codex)

  Which agent should be your primary developer? [claude]
  Which agent should be your primary reviewer? [codex]

Step 3/4: Verification
  Testing claude... OK (version 1.0.27)
  Testing codex... OK (version 0.1.0)

Step 4/4: Create First Prompt?
  Would you like to create a PROMPT.md in the current directory? [Y/n]
  Available templates:
    1. feature-spec (Recommended for new features)
    2. bug-fix (Quick bug fixes)
    3. refactor (Code improvements)
    4. blank (Empty template)
  Select template [1]:

Setup complete! Run 'ralph' to start.
```

#### 2.3 Graceful Missing PROMPT.md Handling

When PROMPT.md is missing, default to interactive mode:

```rust
// Current behavior
if !prompt_path.exists() && !args.interactive {
    return Err("PROMPT.md not found. Use --init-prompt or -i");
}

// Proposed behavior
if !prompt_path.exists() {
    if std::io::stdin().is_terminal() {
        // Interactive: offer template selection
        offer_template_selection()?;
    } else {
        // Non-interactive: clear error
        return Err("PROMPT.md not found. Use --init-prompt <template>");
    }
}
```

#### 2.4 Post-First-Run Hints

After successful first run, show contextual tips:

```
✓ Ralph pipeline completed successfully!

Tip: Try these next:
  ralph -Q "fix: typo"     Quick mode for small fixes
  ralph --list-templates   See all prompt templates
  ralph --diagnose         Check system configuration
```

### Phase 3: Actionable Error Messages

**Design Principles**: "Rewrite errors for humans" (clig.dev), "Human-readable errors with suggestions" (Atlassian #5), "Suggest next best step" (Atlassian #7), "Did-you-mean suggestions" ([Lucas Costa](https://lucasfcosta.com/2022/06/01/ux-patterns-cli-tools.html))

Key insight from clig.dev: *"Can't write to file.txt" should suggest `chmod +w file.txt`*

Key insight from Git: *Never assume correction automatically—users won't learn correct syntax if you silently fix typos*

#### 3.1 Enhanced Recovery Advice

Transform prose advice into actionable commands:

```rust
impl AgentErrorKind {
    pub fn actionable_advice(&self) -> ActionableAdvice {
        match self {
            Self::CommandNotFound => ActionableAdvice {
                message: "Agent binary not found",
                fix_commands: vec![
                    ("Install Claude Code", "npm install -g @anthropic-ai/claude-code"),
                    ("Install Codex", "npm install -g @openai/codex"),
                ],
                docs_link: Some("docs/agents.md#installation"),
                diagnostic_command: Some("ralph --list-available-agents"),
            },
            Self::AuthFailure => ActionableAdvice {
                message: "Authentication failed",
                fix_commands: vec![
                    ("Authenticate Claude", "claude /login"),
                    ("Set API key", "export ANTHROPIC_API_KEY=sk-..."),
                ],
                docs_link: Some("docs/agents.md#authentication"),
                diagnostic_command: Some("ralph --diagnose"),
            },
            // ... other cases
        }
    }
}
```

Display format:
```
✗ Agent 'claude' not found

  Fix options:
    npm install -g @anthropic-ai/claude-code

  Diagnose:
    ralph --list-available-agents

  Docs: https://codeberg.org/mistlight/RalphWithReviewer/src/docs/agents.md#installation
```

#### 3.2 "Did You Mean?" Suggestions

For typos in agent names:

```rust
fn suggest_agent(input: &str, available: &[String]) -> Option<String> {
    available
        .iter()
        .filter(|a| levenshtein_distance(input, a) <= 2)
        .min_by_key(|a| levenshtein_distance(input, a))
        .cloned()
}
```

Display:
```
✗ Unknown agent 'cluade'
  Did you mean 'claude'?
```

#### 3.3 Context-Aware Error Suggestions

When errors occur, suggest based on context:

```rust
fn suggest_based_on_context(error: &AgentErrorKind, context: &PipelineContext) -> Vec<String> {
    let mut suggestions = vec![];

    if error.suggests_smaller_context() {
        suggestions.push(format!(
            "Try reducing context: RALPH_DEVELOPER_CONTEXT=0 ralph {}",
            context.original_args.join(" ")
        ));
    }

    if context.had_rate_limits {
        suggestions.push("Consider adding fallback agents in config".to_string());
    }

    suggestions
}
```

### Phase 4: Consistent Feedback Patterns

**Design Principle**: "Create a reaction for every action" (Atlassian #4)

Every user action should receive appropriate feedback. Currently Ralph is silent during many operations.

#### 4.0 Action-Reaction Matrix

| User Action | Current Feedback | Proposed Feedback |
|-------------|------------------|-------------------|
| Run `ralph` | Nothing for 1-5s | "Starting pipeline with claude (dev) → codex (rev)..." |
| Agent starts | Nothing | "Development iteration 1/5 starting..." |
| Agent completes | Nothing | "✓ Iteration 1 complete (12 files changed)" |
| Phase changes | Nothing | "Switching to review phase..." |
| Error occurs | Error message | Error + suggestion + next command |
| Pipeline ends | Summary | Summary + "What next?" prompt |

#### 4.1 Watch Mode

Monitor PROMPT.md for changes and auto-run:

```bash
ralph --watch
```

Implementation:
```rust
fn watch_mode(args: &Args) -> io::Result<()> {
    let watcher = notify::recommended_watcher(|res| {
        match res {
            Ok(event) if event.kind.is_modify() => {
                println!("\nPROMPT.md changed. Running pipeline...\n");
                run_pipeline(args);
            }
            _ => {}
        }
    })?;

    watcher.watch(Path::new("PROMPT.md"), RecursiveMode::NonRecursive)?;

    println!("Watching PROMPT.md for changes. Press Ctrl+C to stop.");
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
```

#### 4.2 Post-Run Actions Menu

After pipeline completion, offer next steps:

```rust
fn post_run_menu(summary: &PipelineSummary) -> io::Result<PostRunAction> {
    println!();
    println!("What next?");
    println!("  [v] View diff");
    println!("  [e] Edit PROMPT.md");
    println!("  [r] Run again");
    println!("  [p] Push to remote");
    println!("  [q] Quit");
    println!();
    print!("Choice [q]: ");

    // Read user input and return action
}
```

#### 4.3 Confirmation for Destructive Operations

Add confirmation prompts for operations that modify git history:

```rust
fn confirm_destructive_operation(operation: &str) -> io::Result<bool> {
    print!("⚠ This will {}. Continue? [y/N]: ", operation);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("y"))
}
```

### Phase 5: Color & Terminal Standards

**Design Principles**: "Use colors sparingly for emphasis" (clig.dev), "ANSI 4-bit colors for customization" (GitHub CLI), "Respect NO_COLOR" (clig.dev)

#### 5.0 Color Standardization

Current Ralph uses hardcoded ANSI colors. Production CLIs like GitHub CLI recommend aligning with terminal color tables:

```rust
/// Standard color scheme aligned with ANSI 4-bit colors
/// Users can customize via terminal preferences
pub enum SemanticColor {
    Success,    // Green - completed actions
    Error,      // Red - failures, blocking issues
    Warning,    // Yellow - warnings, non-blocking issues
    Info,       // Cyan - informational, phase indicators
    Emphasis,   // Bold/White - important text
    Dim,        // Gray - secondary info, timestamps
}

impl SemanticColor {
    pub fn to_ansi(&self, colors: &Colors) -> &'static str {
        if !colors.enabled {
            return "";
        }
        match self {
            Self::Success => "\x1b[32m",   // Standard green
            Self::Error => "\x1b[31m",     // Standard red
            Self::Warning => "\x1b[33m",   // Standard yellow
            Self::Info => "\x1b[36m",      // Standard cyan
            Self::Emphasis => "\x1b[1m",   // Bold
            Self::Dim => "\x1b[2m",        // Dim
        }
    }
}
```

#### 5.1 Environment Variable Compliance

Per [clig.dev](https://clig.dev/), respect these environment variables:

| Variable | Behavior |
|----------|----------|
| `NO_COLOR` | Disable all colors |
| `CLICOLOR=0` | Disable colors |
| `CLICOLOR_FORCE=1` | Force colors even in non-TTY |
| `TERM=dumb` | Disable colors and animations |
| `RALPH_ACCESSIBLE=1` | Enable accessibility mode (static progress) |

```rust
fn should_use_color() -> bool {
    // NO_COLOR takes precedence (https://no-color.org/)
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    // CLICOLOR_FORCE overrides TTY detection
    if std::env::var("CLICOLOR_FORCE").map(|v| v == "1").unwrap_or(false) {
        return true;
    }

    // CLICOLOR=0 disables
    if std::env::var("CLICOLOR").map(|v| v == "0").unwrap_or(false) {
        return false;
    }

    // Dumb terminal
    if std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
        return false;
    }

    // Default: color if TTY
    std::io::stdout().is_terminal()
}
```

### Phase 6: Shell Integration

#### 6.1 Shell Completions

Add completion generation:

```rust
#[derive(Parser)]
struct Args {
    /// Generate shell completions
    #[arg(long, value_enum, hide = true)]
    completions: Option<Shell>,
}

fn generate_completions(shell: Shell) {
    let mut cmd = Args::command();
    clap_complete::generate(shell, &mut cmd, "ralph", &mut io::stdout());
}
```

Usage:
```bash
ralph --completions bash > /etc/bash_completion.d/ralph
ralph --completions zsh > ~/.zfunc/_ralph
ralph --completions fish > ~/.config/fish/completions/ralph.fish
```

#### 5.2 Desktop Notifications

Notify when long-running pipelines complete:

```rust
fn send_notification(title: &str, body: &str) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .args(["-e", &format!(
                "display notification \"{}\" with title \"{}\"",
                body, title
            )])
            .output()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("notify-send")
            .args([title, body])
            .output()?;
    }

    Ok(())
}
```

Enable with: `ralph --notify "feat: change"`

### Phase 6: Configuration & Validation

#### 6.1 Config Validation Command

```bash
ralph config validate
```

Output:
```
Checking ~/.config/ralph-workflow.toml...

✓ Syntax valid
✓ Agent chain configured
⚠ Warning: Agent 'aider' in fallback chain not found in PATH
✓ Git identity configured
⚠ Warning: Unknown key 'developr_iters' (did you mean 'developer_iters'?)

Config is valid with 2 warnings.
```

#### 6.2 Config Diff Command

Show effective configuration with overrides highlighted:

```bash
ralph config show
```

Output:
```
# Effective configuration (sources: config file, environment, CLI)

[agent_chain]
developer = ["claude", "codex"]     # from: config file
reviewer = ["codex", "claude"]      # from: config file

[general]
developer_iters = 10                # from: RALPH_DEVELOPER_ITERS (override)
reviewer_reviews = 2                # from: config file (default)
verbosity = 2                       # from: default
```

### Phase 7: History & Observability

#### 7.1 Run History

Track runs in `.agent/history.json`:

```json
{
  "runs": [
    {
      "id": "run_abc123",
      "timestamp": "2026-01-15T10:30:00Z",
      "commit_msg": "feat: add user auth",
      "preset": "standard",
      "dev_agent": "claude",
      "rev_agent": "codex",
      "status": "completed",
      "duration_ms": 342000,
      "commit_sha": "a1b2c3d"
    }
  ]
}
```

Command:
```bash
ralph history
```

Output:
```
Recent runs:
  run_abc123  2026-01-15 10:30  feat: add user auth      completed  5m 42s
  run_def456  2026-01-15 09:15  fix: login bug           completed  1m 23s
  run_ghi789  2026-01-14 16:45  refactor: auth module    failed     3m 10s
```

#### 7.2 Replay Command

Re-run with same settings:

```bash
ralph replay run_abc123
```

#### 7.3 Token/Cost Tracking

Track token usage per run (when available from agent output):

```rust
struct TokenUsage {
    input_tokens: u64,
    output_tokens: u64,
    estimated_cost_usd: Option<f64>,
}
```

Display in summary:
```
📊 Summary
──────────────────────────────────
  ⏱  Total time:      5m 42s
  🔄  Dev runs:        5/5
  🔍  Review runs:     2
  📝  Changes detected: 12
  🪙  Tokens used:     45,230 in / 12,456 out
  💰  Est. cost:       ~$0.23
```

### Phase 8: Quick Wins

#### 8.1 `ralph status` Command

Show current `.agent` state:

```bash
ralph status
```

Output:
```
Ralph Status
──────────────────────────────────
  Prompt:     PROMPT.md exists (234 bytes)
  Checkpoint: None (clean state)
  Last run:   2026-01-15 10:30 (completed)
  Agents:     claude (dev), codex (rev)

Files:
  .agent/STATUS.md      exists
  .agent/PLAN.md        exists
  .agent/ISSUES.md      not found (isolation mode)
  .agent/logs/          3 files
```

#### 8.2 `ralph clean` Command

Reset `.agent` directory:

```bash
ralph clean
```

With confirmation:
```
This will delete:
  .agent/STATUS.md
  .agent/PLAN.md
  .agent/NOTES.md
  .agent/ISSUES.md
  .agent/logs/ (3 files)

Preserve checkpoint? [Y/n]:
Cleaning... done
```

#### 8.3 Cancellation Hint

Show hint during long operations:

```
[Development 3/5] claude ━━━━━━━━━━━━ 2m 34s
Press Ctrl+C to cancel (checkpoint will be saved)
```

#### 8.4 Agent Display in Quiet Mode

Even in quiet mode, show which agent is being used:

```
ralph -q "fix: typo"
```

Output:
```
Starting: claude (dev) → codex (rev)
✓ Pipeline completed in 1m 23s
```

---

## Implementation Priority

### Priority Legend

- **P0 (Critical)**: Addresses core UX principles; implement immediately
- **P1 (High)**: Significant user-facing improvements; implement in next release
- **P2 (Medium)**: Quality-of-life features; implement when bandwidth allows
- **P3 (Lower)**: Nice-to-have; consider for future releases

### Priority Matrix

| Phase | Item | Effort | Impact | Priority | Principle Addressed |
|-------|------|--------|--------|----------|---------------------|
| 1.1 | Immediate feedback (100ms) | Low | High | P0 | clig.dev responsiveness |
| 1.2 | Pipeline phase indicator | Medium | High | P0 | Atlassian #3 (progress) |
| 2.1 | First-run detection | Low | High | P0 | Progressive discovery |
| 3.1 | Actionable error advice | Medium | High | P0 | Atlassian #5, #7 |
| 4.0 | Action-reaction feedback | Low | High | P0 | Atlassian #4 |
| 9.3 | Cancellation hint (Ctrl+C) | Low | Medium | P0 | Atlassian #9 |
| 1.3 | Heartbeat with accessibility | Low | Medium | P1 | GitHub CLI accessibility |
| 2.2 | `ralph setup` command | Medium | High | P1 | gh CLI patterns |
| 3.2 | "Did you mean?" suggestions | Low | Medium | P1 | Git, clig.dev |
| 5.0 | Color standardization | Low | Medium | P1 | NO_COLOR, CLICOLOR |
| 6.1 | Shell completions | Low | Medium | P1 | Discoverability |
| 9.1 | `ralph status` command | Low | Medium | P1 | git status pattern |
| 9.2 | `ralph clean` command | Low | Medium | P1 | Workflow hygiene |
| 2.3 | Graceful missing PROMPT.md | Low | High | P1 | Error recovery |
| 4.1 | Watch mode | Medium | Medium | P2 | Developer workflow |
| 4.2 | Post-run actions menu | Medium | Medium | P2 | Suggest next step |
| 7.1 | Config validation | Medium | Medium | P2 | Error prevention |
| 8.1 | Run history | Medium | Low | P2 | Observability |
| 1.4 | Estimated time remaining | High | Medium | P3 | User expectations |
| 6.2 | Desktop notifications | Low | Low | P3 | Long-running UX |
| 8.2 | Replay command | Medium | Low | P3 | Power user feature |
| 8.3 | Token/cost tracking | High | Low | P3 | Cost awareness |

### Recommended Implementation Order

**✅ COMPLETED** - Infrastructure Foundation (32% complete):
- Phase 5: Color & Terminal Standards ✅
- Phase 1.3: Heartbeat with Accessibility Mode ✅
- Phase 7.3: Streaming Quality Metrics ✅
- Phase 6.1: Enhanced Diagnostics ✅
- Phase 5.0: Color Standardization ✅ (partial)
- Phase 5.1: Full Environment Variable Compliance ✅
- Phase 8: Template Listing ✅

**Sprint 1 (P0 - Foundation - Highest Priority)**:
1. **Phase 1.1**: Immediate feedback before agent calls (100ms rule)
   - File: `app/mod.rs:99`
   - Effort: 15 minutes
   - Impact: Eliminates "Is it stuck?" friction

2. **Phase 4.0**: Action-reaction feedback for all operations
   - File: `app/mod.rs` (multiple locations)
   - Effort: 1-2 hours
   - Impact: Users always know what's happening

3. **Phase 8.3**: Cancellation hint display
   - File: `app/mod.rs:99`
   - Effort: 15 minutes
   - Impact: Clear way out for long operations

4. **Phase 2.1**: First-run detection with setup offer
   - File: `app/mod.rs:86`
   - Effort: 2-3 hours
   - Impact: Reduces first-run failure rate

**Sprint 2 (P0 + P1 Core - High Value)**:
5. **Phase 3.1**: Actionable error messages with commands
   - File: `agents/error.rs:36-65`
   - Effort: 3-4 hours
   - Impact: Cuts error resolution time by 50%

6. **Phase 1.2**: Pipeline phase indicator with progress bar
   - New dependency: `indicatif` crate
   - Effort: 4-6 hours
   - Impact: Clear progress visibility

7. **Phase 3.2**: "Did you mean?" suggestions
   - File: `agents/error.rs` (add Levenshtein distance)
   - Effort: 2-3 hours
   - Impact: Better typo recovery

**Sprint 3 (P1 Polish - Quality of Life)**:
8. **Phase 2.2**: `ralph setup` wizard
   - File: `cli/` (add new subcommand)
   - Effort: 4-6 hours
   - Impact: Better onboarding experience

9. **Phase 8.1**: `ralph status` command
   - File: `cli/handlers/` (add new handler)
   - Effort: 2-3 hours
   - Impact: Better state visibility

10. **Phase 8.2**: `ralph clean` command
    - File: `cli/handlers/` (add new handler)
    - Effort: 2-3 hours
    - Impact: Easier cleanup

11. **Phase 6.1**: Shell completions
    - Add `clap_complete` dependency
    - Effort: 2-3 hours
    - Impact: Better discoverability

**Parallel Work Opportunities**:
- Sprint 1 items (1-4) can be done in parallel by different contributors
- Sprint 2 items (5-7) have minimal dependencies
- Sprint 3 items (8-11) are largely independent

**Updated Rationale**:
- Completed infrastructure (terminal standards, diagnostics, color support) provides solid foundation
- Focus now on user-facing features that directly address the "Top 5 Gaps" from Executive Summary
- All P0 features can be implemented in ~15 hours total
- Quick wins (items 1, 3) take <30 minutes combined

---

## Success Criteria

1. **First-Run Success Rate**: 90%+ of new users complete first run without errors
2. **Progress Visibility**: Users always know current phase and can estimate completion
3. **Error Resolution Time**: Average time to resolve errors reduced by 50%
4. **User Satisfaction**: Positive feedback on CLI experience
5. **Power User Productivity**: Watch mode and history features adopted by regular users

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Terminal compatibility issues | Test on major terminals (iTerm2, Terminal.app, GNOME Terminal, Windows Terminal) |
| Progress bar performance overhead | Throttle updates to 10Hz max, disable in non-TTY |
| History file grows unbounded | Limit to last 100 runs, add `ralph history --prune` |
| Interactive features break scripting | All interactive features gated on `stdin.is_terminal()` |
| Shell completion maintenance burden | Generate from clap derive, auto-update on release |
| Watch mode false triggers | Debounce file events, ignore whitespace-only changes |

---

## Alternatives Considered

1. **TUI Dashboard**: Considered a full terminal UI (ratatui) but rejected for complexity; incremental improvements preferred
2. **Web Dashboard**: Rejected as out of scope; Ralph is CLI-first
3. **VS Code Extension**: Deferred; CLI improvements benefit all users first
4. **JSON Output Mode**: Already supported via `--debug`; structured output for scripts covered

---

## References

### Internal Codebase

- CLI arguments: `ralph-workflow/src/cli/args.rs`
- Logger module: `ralph-workflow/src/logger/mod.rs`
- Banner display: `ralph-workflow/src/banner.rs`
- Error classification: `ralph-workflow/src/agents/error.rs`
- Diagnostics: `ralph-workflow/src/diagnostics/mod.rs`
- Quick reference: `docs/quick-reference.md`

### External CLI Design Guidelines

- [Command Line Interface Guidelines (clig.dev)](https://clig.dev/) - Comprehensive open-source CLI design guide
- [10 Design Principles for Delightful CLIs - Atlassian](https://www.atlassian.com/blog/it-teams/10-design-principles-for-delightful-clis) - Atlassian Forge CLI design principles
- [UX Patterns for CLI Tools - Lucas Costa](https://lucasfcosta.com/2022/06/01/ux-patterns-cli-tools.html) - Practical UX patterns with examples
- [CLI UX Best Practices - Evil Martians](https://evilmartians.com/chronicles/cli-ux-best-practices-3-patterns-for-improving-progress-displays) - Progress display patterns
- [Elevate Developer Experiences with CLI Design - Thoughtworks](https://www.thoughtworks.com/en-us/insights/blog/engineering-effectiveness/elevate-developer-experiences-cli-design-guidelines) - Enterprise CLI patterns

### Production CLI Examples

- [GitHub CLI (`gh`)](https://cli.github.com/) - Context-aware, accessibility-first design
- [GitHub CLI Accessibility Blog Post](https://github.blog/engineering/user-experience/building-a-more-accessible-github-cli/) - Screen reader support, color standards
- [indicatif - Rust Progress Bars](https://docs.rs/indicatif) - Cargo's progress bar library
- [lazygit](https://github.com/jesseduffield/lazygit) - Information-dense TUI design
- [Warp Terminal](https://www.warp.dev/) - Block-based command output

### Standards & Specifications

- [NO_COLOR Standard](https://no-color.org/) - Environment variable for disabling colors
- [CLICOLOR Spec](https://bixense.com/clicolors/) - Color control environment variables
- [Terminal TrueColor Spec](https://github.com/termstandard/colors) - Color capability detection

---

## Open Questions

1. Should `ralph setup` be the default behavior on first run, or require explicit invocation?
2. Should watch mode include a debounce delay (e.g., 2 seconds after last save)?
3. Should post-run menu be opt-in (`--interactive-post`) or opt-out (`--no-interactive-post`)?
4. Should token tracking require agent cooperation, or should Ralph parse agent output?
5. Should history be stored globally (`~/.ralph/history.json`) or per-repo (`.agent/history.json`)?
6. Should shell completions be installed automatically during `ralph setup`?

---

## Cross-Feature Dependencies

This section documents which features depend on others to help plan implementation order.

### Dependency Graph

```
Phase 1.1 (Immediate Feedback)
    ├── Phase 8.3 (Cancellation Hint) - builds on initial feedback
    ├── Phase 4.0 (Action-Reaction) - uses same feedback pattern
    └── Phase 1.2 (Progress Indicator) - assumes feedback exists

Phase 2.1 (First-Run Detection)
    ├── Phase 2.2 (Setup Command) - can be called from first-run flow
    └── Uses existing template selection infrastructure

Phase 3.1 (Actionable Errors)
    ├── Builds on existing error.rs classification
    ├── Phase 3.2 (Did You Mean) - shares error handling code
    └── No dependencies

Phase 1.2 (Progress Indicator)
    ├── Uses existing terminal.rs for mode detection
    ├── Builds on existing print_progress() function
    └── Phase 1.4 (ETA) - requires progress tracking first

Phase 5.0/5.1 (Color Standards)
    ├── Fully implemented - foundation for other features
    ├── Phase 1.3 (Heartbeat) - uses accessibility mode
    └── All UI features depend on this
```

### Can Be Implemented in Parallel

The following feature groups have **no dependencies** on each other and can be worked on simultaneously:

**Group A: Feedback Features**
- Phase 1.1: Immediate Feedback
- Phase 4.0: Action-Reaction Feedback
- Phase 8.3: Cancellation Hint

**Group B: Onboarding Features**
- Phase 2.1: First-Run Detection
- Phase 2.2: Setup Command

**Group C: Error Handling**
- Phase 3.1: Actionable Error Messages
- Phase 3.2: "Did You Mean?" Suggestions

**Group D: Progress Visualization**
- Phase 1.2: Pipeline Phase Indicator
- Phase 1.3: Heartbeat with Accessibility
- Phase 1.4: Estimated Time Remaining

**Group E: Quality-of-Life**
- Phase 4.1: Watch Mode
- Phase 4.2: Post-Run Actions Menu
- Phase 4.3: Confirmation for Destructive Operations

**Group F: Status & Diagnostics**
- Phase 8.1: `ralph status` Command
- Phase 8.2: `ralph clean` Command
- Phase 6.1: Shell Completions

### Recommended First Steps

For maximum impact with minimum dependencies:

1. **Start with Group A** (Feedback Features) - no dependencies, immediate user value
2. **Then Group C** (Error Handling) - builds on existing error.rs
3. **Then Group D** (Progress) - requires Group A patterns first
4. **Then Group B** (Onboarding) - independent but higher complexity

### Feature Synergies

Some features work better when implemented together:

| Feature Pair | Synergy |
|-------------|---------|
| Phase 1.1 + 1.2 | Immediate feedback + sustained progress visibility |
| Phase 3.1 + 3.2 | Actionable errors + fuzzy matching for comprehensive UX |
| Phase 2.1 + 2.2 | First-run detection + setup wizard for complete onboarding |
| Phase 4.0 + 4.2 | Action-reaction + post-run menu for full workflow |
| Phase 8.1 + 8.2 | Status + clean commands for state management |

### Blocking Dependencies

| Feature | Blocked By | Rationale |
|---------|-----------|-----------|
| Phase 8.3 (Ctrl+C hint) | Phase 1.1 | Should display alongside initial feedback |
| Phase 4.0 (Full feedback) | Phase 1.1 | Uses same feedback pattern |
| Phase 1.2 (Progress bar) | Phase 1.1 | Assumes feedback pattern established |
| Phase 1.4 (ETA) | Phase 1.2 | Requires progress tracking infrastructure |
| Phase 7.3 (Token tracking) | None | Independent but lower priority |

---

## Quick-Start Implementation Guide for Contributors

This section provides a condensed guide for contributors who want to implement RFC-002 features.

### Current Codebase Patterns (Observed)

**Logger Usage**: The codebase extensively uses logger methods for feedback:
- **337 logger calls** across the codebase (info, success, warn, error, subheader, debug)
- Pattern: `ctx.logger.info()`, `logger.success()`, `logger.warn()`, `logger.error()`
- Colors automatically applied via the Colors infrastructure
- Examples in `phases/development.rs:80`, `phases/review/prompt.rs:109`, `app/mod.rs:134`
- **Top usage files**:
  - `phases/commit.rs`: 78 calls (highest - commit phase feedback)
  - `app/mod.rs`: 71 calls (main orchestration)
  - `phases/review/validation.rs`: 35 calls (validation feedback)
  - `phases/review.rs`: 24 calls (review phase feedback)

**Progress Bar**: Already exists in `logger/progress.rs`:
- Function: `print_progress(current: u32, total: u32, label: &str)`
- Format: `[████████░░░░░░░░░] 60% (3/5)`
- Bar width: 20 characters with block characters (`█` filled, `░` empty)
- Handles edge cases (zero total, overflow protection)
- Used in 2 locations:
  - `phases/development.rs:63` - Development iteration progress
  - `phases/review.rs:114` - Review cycle progress

**Display Names**: Registry provides agent display names:
- `registry.display_name(&agent_name)` used at **8 locations** across codebase
- Returns human-readable names like "Claude Code" instead of "claude"
- Key locations:
  - `app/mod.rs:103-104` - Developer/reviewer agent display names
  - `pipeline/runner.rs:312` - Agent display in pipeline
  - `diagnostics/agents.rs:39` - Agent diagnostics
  - `cli/handlers/list.rs` - Agent listing (multiple)
  - `cli/handlers/diagnose.rs:201` - Diagnostics output

### Easiest Wins (Can be done in 1-2 hours each)

#### 1. Phase 8.3: Cancellation Hint (Ctrl+C) - 15 minutes
**File**: `ralph-workflow/src/app/mod.rs` (after line 104)

```rust
// Add after display names are retrieved
if std::io::stdin().is_terminal() {
    logger.warn("Press Ctrl+C to cancel (checkpoint will be saved)");
}
```

**Why it's easy**: Single line addition, uses existing Logger infrastructure, no dependencies.

#### 2. Phase 1.1: Immediate Feedback (100ms Rule) - 15 minutes
**File**: `ralph-workflow/src/app/mod.rs` (after line 104)

```rust
// developer_display and reviewer_display already available at line 103-104
logger.info(&format!(
    "Starting pipeline with {} (dev) → {} (review)...",
    developer_display, reviewer_display
));
```

**Why it's easy**: Single statement, uses existing patterns, immediately visible impact.

#### 3. Phase 4.0: Phase Transition Feedback - 30 minutes
**File**: `ralph-workflow/src/app/mod.rs` (multiple locations in main pipeline flow)

Add feedback messages at key points:
- After phase completion: `logger.success("✓ Development phase complete")`
- Before phase transitions: `logger.info("Switching to review phase...")`
- At pipeline end: `logger.success("✓ Pipeline completed successfully")`

**Why it's easy**: Leverages existing Logger methods, multiple small wins, no new infrastructure.

### Medium-Effort High-Impact Features

#### Phase 1.2: Enhanced Pipeline Phase Indicator
**Effort**: 4-6 hours

**Current State**: Basic progress bar exists (`logger/progress.rs`)
**Missing**: Agent name, elapsed time, phase label

**Steps**:
1. Add `indicatif = "0.17"` to `Cargo.toml` dependencies
2. Create `ralph-workflow/src/logger/progress_indicator.rs`
3. Wrap phase execution in `app/mod.rs` with progress bars

**Example PR Title**: `feat(progress): add pipeline phase indicator with progress bar`

#### Phase 2.1: First-Run Detection
**Effort**: 3-4 hours

**Steps**:
1. Add detection logic in `app/mod.rs` after `initialize_config()` (line ~86)
2. Create interactive prompt using `dialoguer` crate or std::io::stdin()
3. Call existing `handle_init_global()` and `prompt_template_selection()`

**Example PR Title**: `feat(onboarding): add first-run detection and setup wizard`

#### Phase 3.1: Actionable Error Messages
**Effort**: 4-6 hours

**Current State**: `recovery_advice()` returns prose at `agents/error.rs:147-188`
**Missing**: Structured actionable commands

**Steps**:
1. Add `ActionableAdvice` struct to `agents/error.rs` (after line 188)
2. Implement `actionable_advice()` method for each error kind
3. Update error display to use structured advice

**Example PR Title**: `feat(errors): add actionable fix commands to error messages`

### Key File Locations Reference

| Purpose | File | Key Lines/Notes |
|---------|------|-----------------|
| Main pipeline entry | `ralph-workflow/src/app/mod.rs` | 81-210 (config/validation), 210+ (orchestration), **71 logger calls** |
| Agent resolution | `ralph-workflow/src/app/mod.rs` | 97-100 (where Phase 1.1 should go) |
| Display names | `ralph-workflow/src/app/mod.rs` | 103-104 (developer_display, reviewer_display) |
| Error types | `ralph-workflow/src/agents/error.rs` | 36-65 (enum), 147-188 (recovery_advice) |
| Logger infrastructure | `ralph-workflow/src/logger/mod.rs` | 35-77 (color detection), 29+ (progress) |
| Progress bar | `ralph-workflow/src/logger/progress.rs` | 1-123 (print_progress function) |
| Terminal modes | `ralph-workflow/src/json_parser/terminal.rs` | 1-325 (mode detection) |
| Development phase | `ralph-workflow/src/phases/development.rs` | 58-63 (iteration feedback), **18 logger calls** |
| Review phase | `ralph-workflow/src/phases/review/` | **71 logger calls** (12 in prompt.rs, 35 in validation.rs, 24 in review.rs) |
| Commit phase | `ralph-workflow/src/phases/commit.rs` | **78 logger calls** (highest feedback density) |
| Pipeline runner | `ralph-workflow/src/pipeline/runner.rs` | 10 logger calls, agent display at 312 |
| Agent registry | `ralph-workflow/src/agents/registry.rs` | display_name() method, 8 usage locations |

### Testing Strategy

For each feature:
1. **Unit tests**: Test new functions in isolation
2. **Integration tests**: Test pipeline flow with feature enabled
3. **Manual testing**: Run `ralph` and verify visual output
4. **Accessibility testing**: Test with `TERM=dumb` and `NO_COLOR=1`

### Common Patterns to Follow

#### Adding a new logger message:
```rust
// Use existing Logger methods (colors are automatic)
logger.info("Informational message");
logger.success("✓ Success message");
logger.warn("Warning message");
logger.error("✗ Error message");
logger.subheader("Section Header");  // For major sections
```

#### Getting display names:
```rust
// Pattern from app/mod.rs:103-104
let developer_display = registry.display_name(&developer_agent);
let reviewer_display = registry.display_name(&reviewer_agent);
```

#### Checking terminal capabilities:
```rust
// Use existing terminal mode detection
use crate::json_parser::terminal::TerminalMode;
let mode = TerminalMode::detect();

if mode == TerminalMode::Basic {
    // Static output for screen readers
} else {
    // Animated output
}
```

#### Interactive mode check:
```rust
// Pattern: only show interactive features in TTY
if std::io::stdin().is_terminal() {
    // Show interactive prompt
}
```

### Example PR Workflow

1. **Branch**: `git checkout -b rfc-002/phase-X.Y-description`
2. **Implement**: Make changes following patterns above
3. **Test**: `cargo test --all-features && cargo clippy --all-targets`
4. **Document**: Update RFC-002 status section with implementation notes
5. **PR**: Use title like `feat(ux): [RFC-002] Phase X.Y: Description`

---

## Code Examples for Key Remaining Features

This section provides detailed code examples for implementing the highest-priority remaining features in RFC-002.

### Phase 1.1: Immediate Feedback (100ms Rule)

**File**: `ralph-workflow/src/app/mod.rs`
**Location**: After line 99 (after agent resolution)

```rust
// EXISTING CODE (lines ~97-99)
let validated = resolve_required_agents(&config)?;
let developer_agent = validated.developer_agent;
let reviewer_agent = validated.reviewer_agent;

// ADD THIS: Immediate feedback within 100ms
logger.info(&format!(
    "Starting pipeline with {} (development) → {} (review)...",
    developer_agent, reviewer_agent
));
logger.warn("Press Ctrl+C to cancel (checkpoint will be saved)");
```

**Testing**: Run `ralph` and verify the message appears before any agent calls.

---

### Phase 4.0: Action-Reaction Feedback

**File**: `ralph-workflow/src/app/mod.rs`
**Multiple locations** in the main pipeline flow

```rust
// BEFORE PHASE EXECUTION
logger.info(&format!(
    "Starting {} phase with {}...",
    phase_name, agent_name
));

// AFTER ITERATION COMPLETE
logger.success(&format!(
    "✓ Iteration {}/{} complete ({} files changed)",
    iteration, total_iterations, file_count
));

// PHASE TRANSITION
logger.info("Switching from development to review phase...");

// PIPELINE COMPLETE
logger.success(&format!(
    "✓ Pipeline completed successfully in {}",
    format_duration(total_time)
));
```

---

### Phase 3.1: Actionable Error Messages

**File**: `ralph-workflow/src/agents/error.rs`

**Step 1: Add struct after `AgentErrorKind` enum (after line 65)**

```rust
/// Structured actionable advice for error recovery
pub struct ActionableAdvice {
    /// Human-readable error description
    pub message: &'static str,
    /// List of (description, command) pairs for fixes
    pub fix_commands: Vec<(&'static str, &'static str)>,
    /// Optional link to documentation
    pub docs_link: Option<&'static str>,
    /// Optional diagnostic command to run
    pub diagnostic_command: Option<&'static str>,
}

impl ActionableAdvice {
    /// Format the advice for terminal display
    pub fn display(&self, colors: &crate::logger::Colors) -> String {
        let mut output = String::new();

        output.push_str(&format!("{}{}{}\n", colors.red(), self.message, colors.reset()));

        if !self.fix_commands.is_empty() {
            output.push_str(&format!("\n{}Fix options:{}\n", colors.bold(), colors.reset()));
            for (desc, cmd) in &self.fix_commands {
                output.push_str(&format!("  {}: {}\n", desc, cmd));
            }
        }

        if let Some(cmd) = self.diagnostic_command {
            output.push_str(&format!("\n{}Diagnose:{} {}\n", colors.bold(), colors.reset(), cmd));
        }

        if let Some(link) = self.docs_link {
            output.push_str(&format!("\n{}Docs:{} {}\n", colors.bold(), colors.reset(), link));
        }

        output
    }
}
```

**Step 2: Add method to `AgentErrorKind` impl (after line 100+)**

```rust
impl AgentErrorKind {
    // ... existing methods ...

    /// Get actionable advice for this error
    pub fn actionable_advice(&self) -> ActionableAdvice {
        match self {
            Self::CommandNotFound => ActionableAdvice {
                message: "✗ Agent binary not found in PATH",
                fix_commands: vec![
                    ("Install Claude Code", "npm install -g @anthropic-ai/claude-code"),
                    ("Verify PATH", "echo $PATH"),
                ],
                docs_link: Some("docs/agents.md#installation"),
                diagnostic_command: Some("ralph --list-available-agents"),
            },
            Self::AuthFailure => ActionableAdvice {
                message: "✗ Authentication failed for agent",
                fix_commands: vec![
                    ("Authenticate Claude", "claude /login"),
                    ("Set API key", "export ANTHROPIC_API_KEY=sk-..."),
                ],
                docs_link: Some("docs/agents.md#authentication"),
                diagnostic_command: Some("ralph --diagnose"),
            },
            Self::RateLimited => ActionableAdvice {
                message: "✗ API rate limit exceeded",
                fix_commands: vec![
                    ("Wait 60 seconds", "sleep 60"),
                    ("Add fallback agent", "edit ~/.config/ralph-workflow.toml"),
                ],
                docs_link: Some("docs/rate-limiting.md"),
                diagnostic_command: None,
            },
            // ... handle other error types
            _ => ActionableAdvice {
                message: "✗ An error occurred",
                fix_commands: vec![
                    ("Check diagnostics", "ralph --diagnose"),
                    ("Check logs", "cat .agent/logs/pipeline.log"),
                ],
                docs_link: Some("docs/troubleshooting.md"),
                diagnostic_command: Some("ralph --diagnose"),
            },
        }
    }
}
```

**Step 3: Use in error handling (where errors are displayed)**

```rust
// When displaying an error:
let advice = error_kind.actionable_advice();
eprintln!("{}", advice.display(&colors));
```

---

### Phase 1.2: Pipeline Phase Indicator

**Step 1: Add to `Cargo.toml` dependencies**

```toml
[dependencies]
indicatif = "0.17"
```

**Step 2: Create `ralph-workflow/src/logger/progress_indicator.rs`**

```rust
use crate::logger::Colors;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Duration;

/// Manages progress display for pipeline execution
pub struct PipelineProgress {
    multi: MultiProgress,
    phase_bar: ProgressBar,
    spinner: ProgressBar,
    colors: Colors,
}

impl PipelineProgress {
    pub fn new(total_iterations: u64, colors: Colors) -> Self {
        let multi = MultiProgress::new();

        // Main phase progress bar
        let phase_bar = multi.add(ProgressBar::new(total_iteration));
        phase_bar.set_style(
            ProgressStyle::with_template(
                "{prefix:.bold} {bar:40.cyan/dim} {pos}/{len} {elapsed_precise}"
            )
            .unwrap()
        );

        // Activity spinner
        let spinner = multi.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
        );
        spinner.enable_steady_tick(Duration::from_millis(100));

        Self { multi, phase_bar, spinner, colors }
    }

    pub fn set_phase(&self, phase: &str, agent: &str) {
        self.phase_bar.set_prefix(format!("{} [{}]", phase, agent));
        self.phase_bar.reset();
    }

    pub fn tick(&self, message: &str) {
        self.spinner.set_message(message.to_string());
    }

    pub fn increment(&self) {
        self.phase_bar.inc(1);
    }

    pub fn finish(&self) {
        self.phase_bar.finish();
        self.spinner.finish();
    }
}
```

**Step 3: Integrate into phase execution**

```rust
// In app/mod.rs or phases/mod.rs
use crate::logger::progress_indicator::PipelineProgress;

pub fn run_development_phase(
    config: &Config,
    context: &PhaseContext,
) -> anyhow::Result<PhaseResult> {
    let colors = Colors::new();
    let progress = PipelineProgress::new(config.general.developer_iters, colors);

    progress.set_phase("Development", &context.agent_name);

    for iteration in 1..=config.general.developer_iters {
        progress.tick(&format!("Running iteration {}...", iteration));

        // ... run agent ...

        progress.increment();
    }

    progress.finish();
    Ok(result)
}
```

---

### Phase 2.1: First-Run Detection

**File**: `ralph-workflow/src/app/mod.rs`
**Location**: After line 86 (after `initialize_config()`)

```rust
// EXISTING CODE (lines ~86-88)
let Some(init_result) = initialize_config(&args, colors, &logger)? else {
    return Ok(()); // Early exit
};

// ADD THIS: First-run detection
let config_init::ConfigInitResult { config_path, .. } = &init_result;

let is_first_run = !config_path.exists()
    || !prompt_path.exists()
    || !std::path::Path::new(".agent").exists();

if is_first_run && std::io::stdin().is_terminal() {
    println!("{}", colors.bold());
    println!("Welcome to Ralph Workflow!");
    println!("{}", colors.reset());
    println!();
    println!("It looks like this is your first time running Ralph.");
    println!();
    println!("Ralph requires some initial setup:");
    println!("  1. Configuration file (~/.config/ralph-workflow.toml)");
    println!("  2. PROMPT.md in your project directory");
    println!();

    print!("Would you like to run the setup wizard? [Y/n]: ");
    use std::io::Write;
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if input.trim().is_empty() || input.trim().eq_ignore_ascii_case("y") {
        // Run setup wizard (call Phase 2.2 implementation)
        return run_setup_wizard(&args, colors);
    }

    println!();
    println!("Skipping setup. You can initialize manually:");
    println!("  ralph --init        # Create config file");
    println!("  ralph --init-prompt # Create PROMPT.md");
    println!();
}
```

---

### Phase 8.1: `ralph status` Command

**File**: `ralph-workflow/src/cli/handlers/status.rs` (new file)

```rust
use crate::logger::Colors;
use std::path::Path;

pub fn handle_status(colors: Colors) -> anyhow::Result<()> {
    println!("{}Ralph Status{}", colors.bold(), colors.reset());
    println!("─".repeat(50));

    // Check PROMPT.md
    let prompt_path = Path::new("PROMPT.md");
    match prompt_path.exists() {
        true => {
            let metadata = std::fs::metadata(prompt_path)?;
            println!("  Prompt:     PROMPT.md exists ({} bytes)", metadata.len());
        }
        false => println!("  Prompt:     {}PROMPT.md not found{}", colors.yellow(), colors.reset()),
    }

    // Check checkpoint
    let checkpoint_path = Path::new(".agent/checkpoint.json");
    match checkpoint_path.exists() {
        true => {
            let content = std::fs::read_to_string(checkpoint_path)?;
            println!("  Checkpoint: Available");
            // Parse and show phase/iteration if desired
        }
        false => println!("  Checkpoint: None (clean state)"),
    }

    // Check .agent directory contents
    let agent_dir = Path::new(".agent");
    if agent_dir.exists() {
        let entries = std::fs::read_dir(agent_dir)?
            .filter_map(|e| e.ok())
            .count();
        println!("  .agent/:     {} files/directories", entries);
    } else {
        println!("  .agent/:     Not created yet");
    }

    // Check logs
    let logs_dir = Path::new(".agent/logs");
    if logs_dir.exists() {
        let log_files: Vec<_> = std::fs::read_dir(logs_dir)?
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        println!("  Logs:        {} file(s)", log_files.len());
    }

    Ok(())
}
```

**Wire up in `cli/mod.rs`**:

```rust
pub fn handle_status(colors: Colors) -> anyhow::Result<()> {
    handlers::status::handle_status(colors)
}
```

---

## Appendix: User Research Insights

Common friction points observed:

1. **"Is it stuck?"** - Users uncertain during long silent periods
2. **"What do I do now?"** - After errors, unclear next steps
3. **"How do I start?"** - First-run requires reading docs
4. **"What just happened?"** - Pipeline completed but unclear what changed
5. **"Can I stop this?"** - Unclear how to safely cancel

These directly inform the priority of progress visualization (P0), actionable errors (P0), and first-run experience (P0).

---

*End of RFC*
