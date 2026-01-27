# AGENTS.md

ALWAYS USE test-driven-development!

This repository welcomes automated code assistants (“agents”) and human contributors.
Follow these rules so changes stay safe, consistent, and easy to review.

## Extraneous Files
GET RID OF ALL TEMPORARY .md FILES IN THE ROOT or DOC folder, DO NOT WRITE THEM, DO NOT ATTEMPT TO WRITE THEM, ONLY USE PERMANENT DOCUMENTATIONS


## Scope & priorities

Agents should optimize for, in order:

1. **Correctness** (tests pass; behavior matches intent)
2. **Maintainability** (clear code; minimal magic)
3. **Consistency** (follow existing patterns; rustfmt/clippy clean)
4. **Small, reviewable diffs** (avoid drive-by refactors)

If any instruction below conflicts with another file (e.g., `CONTRIBUTING.md`), follow the stricter rule.

For design principles, testing philosophy, and dead code policy, see **[CODE_STYLE.md](CODE_STYLE.md)**.

Do not assume anything about external dependency, if you need to interact with an external API, you must use context7, if that fails, research the official documentation by going to the website through playwright.

Do not create ANY files in the root directory or documentation directory unless prompt is about documentation creation. You have to update outdated documentation though.

---

## Agent Execution Requirements

### YOLO Mode (File Write Permissions)

**CRITICAL:** All agents in the Ralph pipeline MUST run with YOLO mode enabled (e.g., `--dangerously-skip-permissions` for Claude CLI, `--yes` for other agents).

**Why this is mandatory:**

1. **Automated pipeline context**: Ralph is NOT an interactive tool. It's a fully automated CI/CD pipeline that orchestrates multiple agent invocations without human intervention.

2. **XML output requirement**: ALL agent roles (Developer, Reviewer, Commit) must write structured XML files to `.agent/tmp/`:
   - Developers write `development-result.xml`
   - Reviewers write `issues.xml`
   - Commit agents write `commit-message.xml`

3. **XSD retry mechanism**: When XML validation fails, agents must be able to rewrite corrected XML files. Without file write permissions, the retry mechanism fails entirely.

4. **Security is non-issue**: Agents run in an isolated `.agent/` directory with no access to sensitive files. The pipeline explicitly provides safe file paths for agent operations.

**Implementation location:** `ralph-workflow/src/pipeline/runner.rs:95-98`

```rust
// Enable yolo for ALL roles - this is an automated pipeline, not interactive.
// All agents need file write access to output their XML results.
let yolo = true;
```

**Historical bug (fixed 2026-01-23):** Prior to commit `14f3783`, YOLO was conditionally enabled only for Developer role and fix-mode operations. This caused Reviewers and Commit agents to fail writing XML on initial attempts, breaking the entire pipeline.

**Agent configuration:** Every agent must have a `yolo_flag` configured in `agents.toml`:
- Claude CLI: `yolo_flag = "--dangerously-skip-permissions"`
- Aider: `yolo_flag = "--yes"`
- OpenCode agents: Usually no flag needed (non-interactive by default)

**Never disable YOLO mode** in the automated pipeline. If an agent doesn't support autonomous operation, it cannot be used in Ralph's workflow.

---

## Integration Tests

**CRITICAL FOR AI AGENTS:** When working with integration tests, you **MUST** follow the integration test style guide.

- **Read first:** Before modifying, adding, or debugging integration tests, read **[tests/INTEGRATION_TESTS.md](tests/INTEGRATION_TESTS.md)**
- **This is mandatory:** The guide defines non-negotiable rules for behavior-based testing, mocking strategy, and when to update tests
- **Key principles:**
  - Test **observable behavior**, not implementation details
  - Mock only at **architectural boundaries** (filesystem, network, external APIs)
  - NEVER use `cfg!(test)` branches or test-only flags in production code
  - When a test fails, fix the implementation unless the expected behavior changed intentionally

**Common agent mistakes to avoid:**
- ❌ Mocking internal functions or helpers - Only mock external dependencies
- ❌ Testing private implementation details - Test through public APIs
- ❌ Adding `#[cfg(test)]` branches in production code - Refactor for dependency injection instead
- ❌ Updating tests because implementation changed - Only update if expected behavior changed
- ❌ Making real API calls to external services - Always mock external dependencies

**Required patterns:**
- Parser tests → Use `TestPrinter` from `ralph_workflow::json_parser::printer`
- File operations → Use `MemoryWorkspace` (NO `TempDir`, NO `std::fs::*`)
- Process execution → Use `MockProcessExecutor` (NO real process spawning)

---

## Workspace Dependency Injection (Filesystem Abstraction)

**CRITICAL:** ALL filesystem operations in production code MUST go through the `Workspace` trait. Direct `std::fs::*` calls are FORBIDDEN.

### Why This Matters

1. **Testability**: Code using `Workspace` can be tested with `MemoryWorkspace` - no real filesystem needed
2. **Isolation**: Tests run in containers with no filesystem access
3. **Explicit paths**: All paths are relative to workspace root - no CWD dependencies
4. **Consistency**: Single abstraction for all file I/O

### The Rule

| FORBIDDEN | REQUIRED |
|-----------|----------|
| `std::fs::read_to_string(path)` | `workspace.read(path)` |
| `std::fs::write(path, content)` | `workspace.write(path, content)` |
| `std::fs::create_dir_all(path)` | `workspace.create_dir_all(path)` |
| `std::fs::read_dir(path)` | `workspace.read_dir(path)` |
| `std::fs::exists(path)` / `path.exists()` | `workspace.exists(path)` |
| `std::fs::remove_file(path)` | `workspace.remove(path)` |
| `std::fs::metadata(path)` | Use `DirEntry` from `workspace.read_dir()` |

### Implementation

```rust
// Production code - accepts workspace via dependency injection
fn my_function(workspace: &dyn Workspace) -> Result<()> {
    let content = workspace.read(Path::new(".agent/config.toml"))?;
    workspace.write(Path::new(".agent/output.txt"), "result")?;
    Ok(())
}

// Tests - use MemoryWorkspace
#[test]
fn test_my_function() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/config.toml", "key = value");
    
    my_function(&workspace).unwrap();
    
    assert!(workspace.was_written(".agent/output.txt"));
}
```

### Exceptions

The ONLY acceptable uses of `std::fs` are:
1. Inside `WorkspaceFs` implementation itself (the production `Workspace` impl)
2. Bootstrap code that discovers the repo root before `Workspace` is created

### Documented Exceptions

The following specific uses of `std::fs` are acceptable and do not need refactoring:

| Location | Reason |
|----------|--------|
| `workspace.rs` (`WorkspaceFs`) | This IS the production filesystem implementation |
| `app/effect_handler.rs` (`RealAppEffectHandler`) | This IS the production effect handler |
| `config/path_resolver.rs` (`RealConfigEnvironment`) | Production config environment implementation |
| `agents/opencode_api/cache.rs` (`RealCacheEnvironment`) | Production cache implementation |
| `git_helpers/rebase.rs` | Operating on `.git/` directory internals |
| `git_helpers/hooks.rs` | Bootstrap operation on `.git/hooks/` (see module docs) |
| `files/protection/monitoring.rs` | Atomic file open for TOCTOU security |
| `files/io/agent_files.rs` (CWD functions) | CLI plumbing commands before workspace available |
| `checkpoint/file_state.rs` (deprecated functions) | Legacy support with workspace alternatives available |

All other production code MUST use the Workspace trait.

**When you see `std::fs` in production code outside these exceptions, it MUST be refactored to use `Workspace`.**

---

## Absolute rule: no `#[allow(dead_code)]`

This repository **does not permit** suppressing dead code warnings. The same goes for deprecated code

You must **never** introduce `#[allow(dead_code)]`, and you must remove any existing
occurrences if encountered.

Dead code must be handled by one of the following:
- Making it used
- Implement the feature that you will use it on, but just implement it **now** (Remember you have no time constraints or time limit, implement everything fully)
- Gating it behind a feature flag
- Moving it to `examples/` or `benches/`
- Deleting it

Do **not** replace it with other blanket `allow(...)` attributes unless explicitly instructed.

---


# DO NOT OVERRIDE UNLESS THE PROMPT IS ABOUT CLIPPY
## Build & test expectations

Dead code must either be removed or you implement the feature that it needs the dead code

Ensure you run git rebase on the main branch if working on a feature branch and resolve any merge conflicts AND:

Before opening a PR (or marking work “done”), run:

```bash
# THIS IS VERY IMPORTANT!!!! THIS COMMANDS MUST NOT PRODUCE ANY OUTPUT!!! NOTHING AT ALL SHOULD DISPLAY WITH THIS COMMAND
rg -n -U --pcre2 '
(?x)
\#\s*!?\[\s*
(?:
  (allow|expect)
|
  cfg_attr\s*\(
    [^()]*? , \s*
    (allow|expect)
)
\s*\(
  [^()\]]*
  (?:\([^()\]]*\)[^()\]]*)*
\)
\s*\]' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME

# Check integration test compliance (timeout wrappers, doc comments, etc.)
./tests/integration_tests/compliance_check.sh
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME

# Check for forbidden test flags in production code (cfg!(test), test_mode params, etc.)
# DO NOT MODIFY THIS SCRIPT. If it fails, FIX THE PRODUCTION CODE, not the script.
./tests/integration_tests/no_test_flags_check.sh
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME

# DO NOT CHANGE ANY OF THE COMMANDS BELOW
cargo fmt --all --check
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME

# Lint the main crate (lib only) with all its features - THIS MUST BE RAN WITH THE EXACT FLAG DO NOT CHANGE
cargo clippy -p ralph-workflow --lib --all-features -- -D warnings
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME

# Lint the separate integration test package (test-utils is enabled via its ralph-workflow dependency)
cargo clippy -p ralph-workflow-tests --all-targets -- -D warnings
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME

# Run the main crate's unit tests with all features DO NOT CHANGE
cargo test -p ralph-workflow --lib --all-features
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME
# THERE CAN BE NO IGNORED TESTS

# Run the integration tests package
# (dependency features for ralph-workflow should be enabled via ralph-workflow-tests/Cargo.toml) DO NOT CHANGE
cargo test -p ralph-workflow-tests
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME
# THERE CAN BE NO IGNORED TESTS

# Build release artifacts (default-members only)
cargo build --release
# DO NOT CONTINUE IF THE ABOVE COMMANDS PRODUCE ANYTHING AND FIX THE ISSUE,
# IT DOES NOT MATTER WHAT IT IS, IT DOES NOT MATTER IF YOU INTRODUCED OR NOT, YOU SEE IT YOU FIX IT YOU HAVE UNLIMITED TIME
```
