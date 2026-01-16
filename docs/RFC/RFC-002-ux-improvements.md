# RFC-002: Developer Experience Improvements for Ralph Orchestrator

**RFC Number**: RFC-002
**Title**: Developer Experience Improvements for Ralph Orchestrator
**Status**: Draft
**Author**: Analysis based on codebase review
**Created**: 2026-01-15

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

**Sprint 1 (P0 - Foundation)**:
1. Immediate feedback before agent calls (100ms rule)
2. Action-reaction feedback for all operations
3. Cancellation hint display
4. First-run detection with setup offer

**Sprint 2 (P0 + P1 Core)**:
5. Pipeline phase indicator with progress bar
6. Actionable error messages with commands
7. "Did you mean?" suggestions
8. Color environment variable compliance

**Sprint 3 (P1 Polish)**:
9. `ralph setup` wizard
10. `ralph status` and `ralph clean`
11. Shell completions
12. Accessibility mode for progress

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
