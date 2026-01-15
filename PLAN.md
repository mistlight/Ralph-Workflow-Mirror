# Auto Rebase for Ralph - Implementation Plan

## Summary

This document outlines the implementation of automatic git rebase functionality for Ralph. The goal is to automatically rebase feature branches onto the main branch before and after the development/review cycle, reducing merge conflicts when multiple AI agents work on separate worktrees that get merged at different times.

The implementation will:
1. Detect if we're on a feature branch (not main/master)
2. Automatically rebase to main before development starts
3. Automatically rebase to main after review/fix completes
4. Handle conflicts by delegating resolution to AI agents with proper context
5. Provide `--skip-rebase` and `--rebase-only` flags for user control

## Implementation Steps

### Step 1: Add Branch Detection Infrastructure

**File**: `ralph-workflow/src/git_helpers/branch.rs` (new file)

Create a new module for branch-related operations:
- `get_current_branch_name()` - Get the current branch name using libgit2
- `is_main_or_master_branch()` - Check if current branch is "main" or "master"
- `get_default_branch()` - Detect the default branch (refs/remotes/origin/HEAD)
- `ensure_on_feature_branch()` - Validate we're not on main/master when needed

**Rationale**: This provides the foundational branch detection logic that the rest of the rebase feature will depend on. Using libgit2 ensures consistency with existing git operations.

### Step 2: Add Rebase Operations Module

**File**: `ralph-workflow/src/git_helpers/rebase.rs` (new file)

Implement core rebase functionality using libgit2:
- `rebase_onto(upstream_branch)` - Perform rebase using libgit2's Rebase API
- `get_rebase_conflicts()` - Detect if rebase has conflicts
- `abort_rebase()` - Abort a conflicted rebase
- `continue_rebase()` - Continue after conflict resolution
- `get_conflicted_files()` - Get list of files with conflicts

**Edge Cases to Handle**:
- Empty repository (no commits)
- Unborn branch
- Detached HEAD state
- Upstream branch doesn't exist
- Conflicts during rebase

**Rationale**: This encapsulates all rebase operations in one module, providing a clean API for the orchestrator to call. Using libgit2 directly (not git CLI) maintains consistency with the project's approach.

### Step 3: Add Conflict Resolution Prompt

**File**: `ralph-workflow/src/prompts/rebase.rs` (new file)

Create prompts for AI-assisted conflict resolution:
- `conflict_resolution_prompt(conflict_file, our_commit, their_commit)` - Generate prompt for fixing merge conflicts
- Include context about the original PROMPT.md if available
- Include context about the PLAN if it exists
- Show both sides of the conflict clearly

**Rationale**: When rebase conflicts occur, the AI agent needs proper context to resolve conflicts intelligently. This prompt provides the necessary context.

### Step 4: Add CLI Arguments

**File**: `ralph-workflow/src/cli/args.rs` (modify)

Add new CLI argument structures:
```rust
#[derive(Parser, Debug, Default)]
pub struct RebaseFlags {
    /// Skip automatic rebase before/after pipeline
    #[arg(long, help = "Skip automatic rebase to main branch")]
    pub skip_rebase: bool,

    /// Only perform rebase and exit
    #[arg(long, help = "Only rebase to main branch, then exit")]
    pub rebase_only: bool,
}
```

**Rationale**: Provides user control over rebase behavior. `--skip-rebase` is useful when conflicts are expected or handled manually. `--rebase-only` allows updating the feature branch without running the full pipeline.

### Step 5: Add Configuration Options

**File**: `ralph-workflow/src/config/types.rs` (modify)

Add to `FeatureFlags`:
```rust
pub struct FeatureFlags {
    pub(crate) checkpoint_enabled: bool,
    pub(crate) force_universal_prompt: bool,
    pub(crate) auto_rebase_enabled: bool,  // NEW
}
```

**File**: `ralph-workflow/src/config/unified.rs` or `loader.rs` (modify)

Add configuration loading for `auto_rebase` setting.

**Rationale**: Allows users to enable/disable auto-rebase via config file, providing a default behavior that can be overridden.

### Step 6: Integrate Pre-Development Rebase

**File**: `ralph-workflow/src/app/mod.rs` (modify)

In `run_pipeline()` function, before development phase starts:
1. Check if auto-rebase is enabled and not skipped
2. Check if we're on a feature branch
3. If yes, perform rebase to default branch
4. Handle conflicts if they arise

Add new function `run_initial_rebase()` that:
- Detects default branch
- Performs rebase
- If conflicts occur, invokes AI agent with conflict resolution prompt
- Continues rebase after resolution
- Reports success/failure

**Rationale**: Rebasing before development ensures the feature branch starts from the latest main, reducing the likelihood of conflicts during later merges.

### Step 7: Integrate Post-Review Rebase

**File**: `ralph-workflow/src/app/mod.rs` (modify)

In `run_pipeline()` function, after review phase completes:
1. Check if auto-rebase is enabled and not skipped
2. Check if we're still on a feature branch
3. If yes, perform rebase to default branch
4. Handle conflicts if they arise

**Rationale**: Rebasing after development/review ensures the feature branch is up-to-date with main before final commit, making the eventual merge cleaner.

### Step 8: Handle `--rebase-only` Flag

**File**: `ralph-workflow/src/app/mod.rs` (modify)

Add handler in `run()` function:
1. Check if `--rebase-only` is set
2. If yes, skip all pipeline phases
3. Only run the rebase operation
4. Exit with appropriate status

**Rationale**: Provides a quick way to update a feature branch without running the full AI development cycle.

### Step 9: Add Rebase Phase to Checkpoint System

**File**: `ralph-workflow/src/checkpoint/state.rs` (modify)

Add new checkpoint phases:
- `PreRebase` - Before initial rebase
- `PostDevelopmentRebase` - After development, before review
- `PostReviewRebase` - After review

**Rationale**: Allows resuming from rebase operations if interrupted.

### Step 10: Update Module Exports

**File**: `ralph-workflow/src/git_helpers/mod.rs` (modify)

Export new modules:
```rust
pub mod branch;
pub mod rebase;

pub use branch::{get_current_branch_name, is_main_or_master_branch};
pub use rebase::{rebase_onto, abort_rebase, continue_rebase};
```

**Rationale**: Makes the new functionality available to the rest of the application.

### Step 11: Add Comprehensive Tests

**File**: `tests/rebase_workflow.rs` (new file)

Add integration tests for:
- Branch detection on main/master
- Branch detection on feature branch
- Successful rebase without conflicts
- Rebase with conflicts (mock conflict)
- `--skip-rebase` flag behavior
- `--rebase-only` flag behavior
- Empty repository handling
- Detached HEAD handling

**Rationale**: Comprehensive tests ensure the rebase functionality works correctly across various scenarios.

### Step 12: Update Documentation

**Files**:
- `docs/git-workflow.md` - Add rebase section
- `CLAUDE.md` - Add rebase-related rules if needed
- `README.md` - Document new flags

Document:
- How auto-rebase works
- When it runs (before/after phases)
- How to control it (--skip-rebase, --rebase-only)
- How conflicts are handled

**Rationale**: Clear documentation helps users understand and use the new feature effectively.

## Critical Files for Implementation

1. **`ralph-workflow/src/git_helpers/branch.rs`** (new) - Branch detection logic
   - Determines if we're on main/master or a feature branch
   - Finds the default branch for rebasing

2. **`ralph-workflow/src/git_helpers/rebase.rs`** (new) - Core rebase operations
   - Wraps libgit2 rebase API
   - Handles conflict detection
   - Provides abort/continue operations

3. **`ralph-workflow/src/app/mod.rs`** (modify) - Pipeline orchestration
   - Integrate pre and post rebase calls
   - Handle --rebase-only flag
   - Manage conflict resolution flow

4. **`ralph-workflow/src/cli/args.rs`** (modify) - CLI interface
   - Add --skip-rebase flag
   - Add --rebase-only flag

5. **`ralph-workflow/src/prompts/rebase.rs`** (new) - Conflict resolution prompts
   - Generate prompts for AI agents to resolve merge conflicts
   - Include context from PROMPT.md and PLAN.md

## Risks & Mitigations

### Risk 1: Rebase Conflicts Causing Pipeline Failures

**Challenge**: When main has advanced significantly, rebasing a feature branch may cause numerous conflicts that AI agents cannot resolve automatically.

**Mitigation**: 
- Provide clear error messages when conflicts occur
- Allow manual intervention with --skip-rebase
- Consider adding `--continue-rebase` flag for resuming after manual conflict resolution
- Log all conflicted files for user reference

### Risk 2: libgit2 Rebase API Complexity

**Challenge**: libgit2's rebase API is complex and has edge cases that may be difficult to handle correctly.

**Mitigation**:
- Start with simple linear rebases (no cherry-pick complexity)
- Extensive testing in various scenarios
- Consider falling back to git CLI for complex cases if needed (document this decision)
- Handle all error cases gracefully with informative messages

### Risk 3: Breaking Agent Isolation During Conflict Resolution

**Challenge**: The requirement states "AI Agents should not know that we are in a middle of a rebase" but conflict resolution requires showing conflicts.

**Mitigation**:
- Frame conflicts as "merge conflicts between two versions" without mentioning rebase
- Only show the two conflicting commits and the file content
- Don't expose git metadata that reveals rebase state
- Use the existing conflict resolution prompt pattern

### Risk 4: Default Branch Detection Issues

**Challenge**: Different repos use "main", "master", or other default branch names. Detection may fail in repos without origin configured.

**Mitigation**:
- Try multiple detection methods (origin/HEAD, common names)
- Allow configuration of default branch name
- Fall back to "main" as default with warning
- Document how to configure for non-standard setups

### Risk 5: Empty Repository or No Upstream

**Challenge**: Rebasing fails in empty repos or when there's no upstream branch.

**Mitigation**:
- Gracefully skip rebase with informative message
- Don't fail the pipeline for these expected cases
- Log at appropriate verbosity level

## Verification Strategy

### Unit Tests

1. **Branch Detection Tests**
   - Test `is_main_or_master_branch()` returns true for "main" and "master"
   - Test it returns false for feature branches
   - Test `get_default_branch()` with various repo configurations

2. **Rebase Operation Tests**
   - Test successful rebase on clean branch
   - Test rebase with simulated conflicts
   - Test abort and continue operations
   - Test error handling for invalid states

3. **Conflict Detection Tests**
   - Test `get_conflicted_files()` returns correct file list
   - Test `get_rebase_conflicts()` detects conflict state

### Integration Tests

1. **Full Pipeline Test with Rebase**
   - Create feature branch with commits
   - Update main with new commits
   - Run Ralph with auto-rebase
   - Verify feature branch was rebased onto main
   - Verify commits from feature branch are preserved

2. **Conflict Resolution Test**
   - Set up conflicting changes on main and feature
   - Trigger rebase
   - Run AI agent conflict resolution
   - Verify conflicts were resolved
   - Verify rebase completed

3. **Flag Behavior Tests**
   - Test `--skip-rebase` skips rebase operations
   - Test `--rebase-only` only does rebase and exits
   - Test config file setting overrides defaults

4. **Edge Case Tests**
   - Empty repository (no commits yet)
   - Detached HEAD state
   - No origin configured
   - Feature branch is already up-to-date

### Manual Verification Steps

1. **Basic Rebase Verification**
   ```bash
   # Create feature branch
   git checkout -b feature/test-rebase
   echo "feature change" > test.txt
   git add test.txt
   git commit -m "feature commit"

   # Update main
   git checkout main
   echo "main change" > main.txt
   git add main.txt
   git commit -m "main commit"

   # Go back to feature and run Ralph
   git checkout feature/test-rebase
   ralph "add feature"
   # Verify feature was rebased onto main
   ```

2. **Conflict Resolution Verification**
   ```bash
   # Setup conflicting changes
   git checkout -b feature/conflict-test
   echo "version 1" > shared.txt
   git add shared.txt
   git commit -m "feature version"

   git checkout main
   echo "version 2" > shared.txt
   git add shared.txt
   git commit -m "main version"

   # Trigger conflict and resolution
   git checkout feature/conflict-test
   ralph "resolve conflict"
   # Verify AI agent resolved the conflict
   ```

3. **Flag Verification**
   ```bash
   # Test skip flag
   ralph --skip-rebase "test feature"
   # Verify no rebase occurred

   # Test rebase-only
   ralph --rebase-only
   # Verify only rebase ran, no AI agents
   ```

### Success Criteria

1. **Acceptance Check**: After running Ralph on a feature branch, the branch should be rebased onto main automatically (unless --skip-rebase is used)
2. **Acceptance Check**: Rebase happens BEFORE development starts
3. **Acceptance Check**: Rebase happens AFTER review/fix completes
4. **Acceptance Check**: AI agents resolve conflicts without knowing they're in a rebase
5. **Acceptance Check**: `--rebase-only` flag works correctly

### Testing Commands

```bash
# Run all tests
cargo test --all-features

# Run only rebase-related tests
cargo test --test rebase_workflow

# Run with verbose output
cargo test --all-features -- --nocapture

# Check for dead code (must produce no output)
rg -n -U --pcre2 '(?x)\#\s*!?\[\s*(allow|expect)\s*\(' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# Run clippy (must pass)
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --all -- --check
```

## Notes

### libgit2 Rebase API Considerations

The libgit2 rebase API works as follows:
1. `repo.rebase(...)` starts a rebase operation
2. Iterate through rebase operations with `rebase.next()`
3. For each operation, apply it
4. If conflicts occur, they appear as unmerged files in the index
5. After resolving conflicts, mark as resolved and continue
6. Finally, `rebase.finish()` completes the rebase

### Conflict Resolution Flow

When conflicts occur during rebase:
1. Detect conflicts (check for unmerged files in index)
2. For each conflicted file, generate a conflict resolution prompt
3. Invoke the AI agent (likely the developer agent) with:
   - The conflict file content (showing both sides)
   - Context from PROMPT.md (original task)
   - Context from PLAN.md (if available)
   - The two commit OIDs that are conflicting
4. Agent produces resolved file content
5. Orchestrator writes resolved content and stages it
6. Continue rebase

### Deterministic Rebase Operations

Per the requirements, rebase operations must be deterministic:
- The orchestrator controls all rebase operations via libgit2
- AI agents only resolve conflicts (file content merges)
- Agents don't run git commands or know about rebase state
- This maintains the existing pattern of agent isolation from git operations
