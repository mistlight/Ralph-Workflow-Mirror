# Implementation Plan: Improve Streaming Output Quality and Polish

## Summary

This plan outlines incremental UX polish improvements to the streaming output system in Ralph CLI. The streaming architecture is now functionally correct (message lifecycle, deduplication, snapshot detection all work), but the user experience can be improved by reducing visual noise during streaming, optimizing the completion sequence, and ensuring consistent behavior across all agents. The focus is on refinement rather than architectural changes—making streaming output feel smoother, cleaner, and more professional.

---

## Current State Analysis

The streaming system uses a multi-line cursor-positioning pattern:
```
First delta:   [prefix] Hello\n\x1b[1A
Subsequent:    \x1b[2K\r[prefix] Hello World\n\x1b[1A
Completion:    \x1b[1B\n
```

**What works well:**
- Unified `StreamingSession` handles state correctly
- Deduplication prevents double-display of final messages
- Snapshot-as-delta auto-repair handles GLM agents
- Content block transitions are properly finalized

**Pain points to address:**
1. Warning messages (`eprintln!`) during streaming can interleave with output
2. The prefix `[agent-name]` appears on every delta update, creating visual repetition
3. Completion sequence may leave extra vertical spacing
4. Verbose debug output in normal mode (snapshot detection warnings)

---

## Implementation Steps

### Step 1: Gate Warning Messages Behind Verbosity Levels

**Files:** `ralph-workflow/src/json_parser/streaming_state.rs`

**Changes:**
1. Remove or gate the `eprintln!` warning in `on_message_start()` (lines 240-246) that fires on mid-stream restarts
   - This warning is informational for debugging but clutters normal output
   - Keep the defensive behavior (preserving `output_started_for_key`), remove the print

2. Gate the large delta size warning in `on_text_delta_key()` (lines 470-475) behind a `#[cfg(debug_assertions)]` or remove entirely
   - The auto-repair handles this case correctly; the warning is redundant

3. Gate the pattern detection warning (lines 493-499) behind debug assertions
   - Same rationale: the system handles it, warning is noise in production

**Rationale:** These warnings were useful during development but add noise in normal operation. The system already handles these edge cases correctly.

---

### Step 2: Optimize Completion Sequence for Cleaner Transitions

**Files:** `ralph-workflow/src/json_parser/delta_display.rs`

**Current:** `render_completion()` returns `"\x1b[1B\n"` (cursor down + newline)

**Potential Issues:**
- If content already ends at a clean line boundary, this may add extra vertical space
- The cursor-down may be unnecessary if we're already on the correct line

**Changes:**
1. Audit whether the cursor-down (`\x1b[1B`) is always needed
   - If the last delta left cursor at line start after `\x1b[1A`, then cursor-down returns to the content line
   - The subsequent newline adds one blank line, which may be too much

2. Consider alternative completion: just `"\n"` without cursor movement
   - This assumes the cursor is already on the content line after the last delta's `\n\x1b[1A` sequence
   - Test this change carefully to ensure no visual regression

3. If cursor-down is needed, ensure only one newline follows to prevent extra spacing

**Rationale:** Reduce unnecessary vertical whitespace between streaming output and subsequent messages.

---

### Step 3: Consider Prefix Display Strategy (Evaluation Only)

**Files:** `ralph-workflow/src/json_parser/delta_display.rs`

**Current behavior:** Prefix `[agent-name]` is shown on every delta (both first and subsequent).

**Options to evaluate:**

**Option A (Current - Keep):**
- Pros: Consistent visual reference, user always knows source
- Cons: Visual repetition during rapid streaming

**Option B (Prefix on first delta only):**
- Change `render_subsequent_delta()` to omit the prefix
- Pros: Cleaner streaming appearance, less visual noise
- Cons: Lose visual anchor if user scrolls back during long streams

**Option C (Prefix on first and last only):**
- First delta shows prefix
- Subsequent deltas show only content (no prefix)
- Completion re-renders final line with prefix
- Pros: Clean streaming with final context
- Cons: More complex implementation

**Recommendation:**
- Start with **Option A (keep current)** as it's the safest
- If user feedback indicates prefix is too noisy, implement Option B as a future iteration
- Document this decision in the plan

---

### Step 4: Audit Newline Handling in Parsers

**Files:**
- `ralph-workflow/src/json_parser/claude.rs`
- `ralph-workflow/src/json_parser/codex.rs`
- `ralph-workflow/src/json_parser/gemini.rs`
- `ralph-workflow/src/json_parser/opencode.rs`

**Changes:**
1. Review all `MessageStop` / completion handlers to ensure consistent behavior:
   - Completion output should only emit when content was actually streamed
   - The check should be `was_in_block || session.has_any_streamed_content()`

2. Verify that thinking content (`ThinkingDelta`) and tool input content have proper line termination
   - Currently these use `DeltaDisplayFormatter` which adds `\n` at end
   - Ensure no double-newlines when transitioning between content types

3. Add comments documenting the expected output pattern for each parser

**Rationale:** Ensure all parsers behave identically for consistent UX across different agents.

---

### Step 5: Improve Test Coverage for Visual Output

**Files:** `ralph-workflow/src/json_parser/delta_display.rs` (tests section)

**Add tests:**
1. **Test: Completion sequence produces expected cursor movements**
   ```rust
   #[test]
   fn test_completion_sequence_minimal_spacing() {
       // Verify completion adds exactly 1 newline after content
   }
   ```

2. **Test: Full streaming sequence produces clean output**
   ```rust
   #[test]
   fn test_streaming_full_sequence_no_extra_lines() {
       let first = TextDeltaRenderer::render_first_delta(...);
       let second = TextDeltaRenderer::render_subsequent_delta(...);
       let complete = TextDeltaRenderer::render_completion();
       // Count total newlines, verify expected count
   }
   ```

3. **Test: Multiple content blocks have proper separation**
   ```rust
   #[test]
   fn test_multiple_blocks_single_separator() {
       // Transition from block 0 to block 1
       // Verify exactly 1 newline between them
   }
   ```

**Rationale:** Codify the expected visual behavior to catch regressions.

---

### Step 6: Documentation and Code Cleanup

**Files:**
- `ralph-workflow/src/json_parser/delta_display.rs`
- `ralph-workflow/src/json_parser/streaming_state.rs`

**Changes:**
1. Update module documentation to describe the expected visual output pattern
2. Add inline comments explaining the purpose of each ANSI escape sequence
3. Remove any dead code or unused helper functions
4. Ensure all public APIs have documentation

**Rationale:** Improve maintainability and make it easier to understand the streaming output contract.

---

## Critical Files for Implementation

1. **`ralph-workflow/src/json_parser/streaming_state.rs`** - Remove/gate warning messages (Step 1)

2. **`ralph-workflow/src/json_parser/delta_display.rs`** - Optimize completion sequence (Step 2), improve tests (Step 5)

3. **`ralph-workflow/src/json_parser/claude.rs`** - Primary parser to verify changes work correctly (Step 4)

4. **`ralph-workflow/src/json_parser/codex.rs`** - Secondary parser to verify consistency (Step 4)

5. **`ralph-workflow/src/logger/mod.rs`** - Reference for color handling, no changes expected

---

## Risks & Mitigations

### Risk 1: Terminal Compatibility Regression
**Description:** Changing ANSI escape sequences could break rendering on some terminals.
**Mitigation:**
- The current pattern is industry-standard (Rich, Ink, Bubble Tea use similar)
- Any changes should be minimal and tested on multiple terminals
- Keep changes conservative and easily revertible

### Risk 2: Breaking Existing Streaming Behavior
**Description:** Modifying completion sequence could cause visual glitches.
**Mitigation:**
- Extensive test coverage will catch regressions
- Make one change at a time, test after each
- Keep the ability to revert to current behavior

### Risk 3: GLM Agent Compatibility
**Description:** GLM agents have non-standard behavior that's specially handled.
**Mitigation:**
- Preserve all GLM-specific logic (mid-stream restart handling)
- Only reduce warning verbosity, not the handling itself
- Test with actual GLM agent output if possible

### Risk 4: Introducing Clippy Warnings
**Description:** New code might introduce clippy warnings or need `#[allow(...)]`.
**Mitigation:**
- Follow CLAUDE.md rules strictly: no `#[allow(dead_code)]` ever
- Run clippy after each change
- Fix any warnings immediately

---

## Verification Strategy

### Automated Checks
```bash
# 1. Check for forbidden allow/expect attributes (must produce no output)
rg -n --pcre2 '(?x)
  \#\s*!?\[\s*
  (allow|expect)
  \s*\(
    [^()\]]*
    (?:\([^()\]]*\)[^()\]]*)*
  \)
  \s*\]
' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# 2. Format and lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run all tests
cargo test --all-features

# 4. Run specific streaming tests
cargo test --all-features streaming
cargo test --all-features delta
```

### Manual Verification
1. **Stream a Claude response:**
   - Output should update smoothly in place
   - No extra blank lines after completion
   - Prefix visible on content line

2. **Stream a Codex response:**
   - Behavior should match Claude
   - No visual differences between parsers

3. **Trigger snapshot-as-delta (GLM agent):**
   - No warning messages in normal mode
   - Auto-repair should work silently
   - Final output should be correct

4. **Test with colors disabled (`NO_COLOR=1`):**
   - Output should be clean without ANSI codes
   - No visible escape sequences

5. **Test narrow terminal (e.g., 40 columns):**
   - Long lines should not corrupt display
   - In-place updates should still work

### Success Criteria
- [ ] All existing tests pass
- [ ] No new clippy warnings
- [ ] No `#[allow(dead_code)]` added
- [ ] Streaming output feels smoother (subjective but verifiable)
- [ ] No extra blank lines after streaming completion
- [ ] No warning messages during normal streaming
- [ ] Consistent behavior across Claude, Codex, Gemini, OpenCode parsers
- [ ] Documentation updated and accurate
