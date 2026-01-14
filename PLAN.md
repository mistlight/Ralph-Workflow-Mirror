# Merge Conflict Resolution Plan

## Summary

Fix merge conflicts between `wt-help-update` branch and `main` branch. The conflicts arise from two divergent feature developments:

1. **wt-help-update branch**: Improved help text UX with tiered help system (basic + `--help-advanced` command), added `-U/--rapid` preset mode, and reorganized help text.
2. **main branch**: Added git user identity CLI arguments (`--git-user-name`, `--git-user-email`) with env var support (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`), added `GitConfig` variant to `IdentitySource` enum, and removed the tiered help system in favor of comprehensive inline help.

The resolution will:
- Keep main branch's improved identity override system (CLI args + env vars)
- Remove the `--help-advanced` command and handler (deprecated in favor of comprehensive inline help)
- Keep help text improvements from wt-help-update where they don't conflict
- Preserve the `-U/--rapid` preset mode which is valuable functionality
- Merge the `GitConfig` variant into `IdentitySource` enum from main

## Implementation Steps

### Step 1: Resolve merge conflict in `src/git_helpers/identity.rs`

**File**: `src/git_helpers/identity.rs`

The conflict is in the `IdentitySource` enum definition. HEAD removed blank lines between variants, while main added a `GitConfig` variant.

**Resolution**: Keep main's version which includes the `GitConfig` variant. This variant is used for tracking where git identity was resolved from and is part of main's identity resolution chain.

**Action**: Remove the `<<<<<<< HEAD` through `>>>>>>> main` markers and keep the main branch version with all variants including `GitConfig`.

### Step 2: Resolve merge conflict in `src/cli/args.rs`

**File**: `src/cli/args.rs`

Two conflicts:

1. **Duplicate field declarations**: Both HEAD and main declare `git_user_name` and `git_user_email` but with different configurations:
   - HEAD: Only has `long` and basic `help`
   - main: Has `long`, `env`, and more detailed `help` text indicating priority

   **Resolution**: Keep main's version which includes environment variable support (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`).

2. **Help text vs `--help-advanced` flag**:
   - HEAD: Has compact help text with `--help-advanced` reference
   - main: Has comprehensive inline help text (no `--help-advanced` needed)

   **Resolution**: Keep main's comprehensive inline help text. The `--help-advanced` flag and handler are being removed in favor of this comprehensive approach.

**Actions**:
- Remove duplicate `git_user_name` and `git_user_email` fields (keep main's version with env support)
- Remove the `help_advanced: bool` field (this feature was removed in main)
- Use main's comprehensive `after_help` text (it's inline, not separated)

### Step 3: Remove unused `--help-advanced` handler files

**Files affected**:
- `src/cli/handlers/advanced_help.rs` - **DELETE** (this file is removed in main)
- `src/cli/handlers/mod.rs` - **MODIFY** to remove references to `advanced_help` module

**Actions**:
- Delete `src/cli/handlers/advanced_help.rs` if it exists
- Remove `pub mod advanced_help;` from `src/cli/handlers/mod.rs`
- Remove `pub use advanced_help::handle_help_advanced;` from `src/cli/handlers/mod.rs`

### Step 4: Update CLI module exports

**File**: `src/cli/mod.rs`

Remove the export of `handle_help_advanced` since the handler no longer exists.

**Action**: Remove `handle_help_advanced` from the re-export list.

### Step 5: Update app module to remove `--help-advanced` handling

**File**: `src/app/mod.rs`

Remove the code that handles the `--help-advanced` flag.

**Action**: Remove the `if args.help_advanced { handle_help_advanced(&colors); return Ok(()); }` block.

### Step 6: Verify `-U/--rapid` preset mode is preserved

**File**: `src/cli/presets.rs`

The `-U/--rapid` mode was added in wt-help-update branch. Main doesn't have it. This is valuable functionality that should be preserved.

**Action**: Add the rapid mode handling back to `apply_args_to_config()` function if it was removed. The logic should be:
```rust
// Rapid mode: 2 developer iterations, 1 review pass
if args.rapid {
    if args.developer_iters.is_none() {
        config.developer_iters = 2;
    }
    if args.reviewer_reviews.is_none() {
        config.reviewer_reviews = 1;
    }
}
```

### Step 7: Verify `rapid` field exists in Args

**File**: `src/cli/args.rs`

Ensure the `rapid: bool` field exists in the Args struct (it should be there from HEAD).

**Action**: Confirm `rapid` field exists with `-U` short flag and `--rapid` long flag.

## Critical Files for Implementation

1. **`src/cli/args.rs`** - Contains merge conflicts for `git_user_name`, `git_user_email`, and `help_advanced` fields. Needs to keep main's identity args with env var support, remove `help_advanced`, and keep `rapid` field.

2. **`src/git_helpers/identity.rs`** - Contains merge conflict in `IdentitySource` enum. Needs to keep main's version with `GitConfig` variant.

3. **`src/cli/handlers/advanced_help.rs`** - Should be DELETED as this feature was removed in main in favor of comprehensive inline help.

4. **`src/cli/handlers/mod.rs`** - Needs to remove module declaration and re-export for `advanced_help`.

5. **`src/app/mod.rs`** - Needs to remove the `--help-advanced` command handling block.

6. **`src/cli/mod.rs`** - Needs to remove `handle_help_advanced` from re-exports.

7. **`src/cli/presets.rs`** - Needs to preserve the `-U/--rapid` preset mode logic.

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| **Losing `-U/--rapid` preset mode** | The rapid mode is useful for fast iteration workflows. After resolving conflicts, verify the `rapid` field exists in `Args` and the logic exists in `apply_args_to_config()`. |
| **Breaking existing `--help-advanced` users** | The `--help-advanced` flag was recently added and may not have wide adoption yet. The comprehensive inline help in main is a better UX. Document this change. |
| **Environment variable support for git identity** | This is new functionality in main that improves identity resolution. Verify `RALPH_GIT_USER_NAME` and `RALPH_GIT_USER_EMAIL` work correctly. |
| **Test failures due to removed `--help-advanced`** | Check if any tests reference the removed handler. Update or remove such tests. |
| **IdentitySource enum test failures** | The main branch added `GitConfig` variant. Ensure tests don't break when this variant is added. |

## Verification Strategy

### Manual Verification Steps

1. **Build succeeds**:
   ```bash
   cargo build
   ```

2. **No merge conflict markers remain**:
   ```bash
   grep -r "<<<<<<< HEAD" --include="*.rs"
   grep -r ">>>>>>> main" --include="*.rs"
   ```
   Should return empty.

3. **Help text displays correctly**:
   ```bash
   cargo run -- --help
   ```
   Verify comprehensive help is shown inline (no reference to `--help-advanced`).

4. **`-U/--rapid` preset works**:
   ```bash
   cargo run -- -U "test"
   ```
   Should use 2 dev iterations and 1 review.

5. **Git identity override works**:
   ```bash
   cargo run -- --git-user-name "Test User" --git-user-email "test@example.com" "test"
   ```
   Should use the provided identity for commits.

6. **Environment variables work**:
   ```bash
   RALPH_GIT_USER_NAME="Env User" RALPH_GIT_USER_EMAIL="env@example.com" cargo run -- "test"
   ```
   Should use environment-provided identity.

### Test Strategy

1. **Run unit tests**:
   ```bash
   cargo test
   ```

2. **Check git_helpers/identity.rs tests** - Ensure tests pass with the `GitConfig` variant.

3. **Verify no references to `handle_help_advanced`**:
   ```bash
   grep -r "handle_help_advanced" --include="*.rs"
   grep -r "help_advanced" --include="*.rs"
   ```
   Should only find the removal sites, not usage sites.

### Success Criteria

- [ ] Merge conflict markers removed from all files
- [ ] `cargo build` succeeds without errors
- [ ] `cargo test` passes all tests
- [ ] `ralph --help` shows comprehensive inline help
- [ ] `ralph --help` does NOT mention `--help-advanced`
- [ ] `ralph -U "test"` uses rapid preset (2 dev + 1 review)
- [ ] `ralph --git-user-name "X" --git-user-email "Y" "test"` uses provided identity
- [ ] Environment variables `RALPH_GIT_USER_NAME` and `RALPH_GIT_USER_EMAIL` work
- [ ] No leftover references to `advanced_help` module or `handle_help_advanced` function
