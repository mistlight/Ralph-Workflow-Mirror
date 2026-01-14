# Implementation Plan: Fix Streaming Partials Rendering Issues

## Summary

This plan addresses the architectural root causes of streaming display bugs that manifest as prefix spam, line breaks within streaming output, glued text, and duplicate final messages. The fix enforces a strict streaming contract with explicit message lifecycle management, ensuring each streaming delta is processed exactly once and final messages are never duplicated. The implementation will modify the unified `StreamingSession` state machine to guarantee these invariants and update all parsers (Claude, Codex, Gemini, OpenCode) to consistently follow the contract.

---

## Implementation Steps

### Step 1: Add Message Lifecycle State Enforcement to StreamingSession

**File:** `ralph-workflow/src/json_parser/streaming_state.rs`

**Changes:**
1. Convert the soft `assert_lifecycle_state` debug assertion into a hard runtime guard that returns early or logs warnings in release mode
2. Add an explicit `ContentBlockState` enum to track whether we're in a content block:
   ```rust
   pub enum ContentBlockState {
       NotInBlock,
       InBlock { index: String, started_output: bool },
   }
   ```
3. Replace the boolean `in_content_block` with this richer state that tracks the current block's index
4. Add a method `ensure_content_block_finalized(&mut self)` that:
   - Emits a newline if `started_output` is true
   - Transitions to `NotInBlock`
   - Clears the current block index
5. Update `on_message_stop()` to call this method

**Rationale:** The current `in_content_block: bool` doesn't track which block is active, leading to edge cases where block boundaries are crossed without proper finalization.

---

### Step 2: Fix Missing Message ID Propagation in Codex and OpenCode Parsers

**Files:**
- `ralph-workflow/src/json_parser/codex.rs`
- `ralph-workflow/src/json_parser/opencode.rs`

**Changes:**
1. In **Codex parser**:
   - The `TurnStarted` event currently calls `on_message_start()` but doesn't generate a unique turn ID
   - Generate a synthetic turn ID (e.g., using a counter or timestamp)
   - Call `session.set_current_message_id(Some(turn_id))` after `on_message_start()`
   - On `TurnCompleted`, ensure `on_message_stop()` is called to mark the message as displayed

2. In **OpenCode parser**:
   - Use the `sessionID` from events as the message ID
   - Call `session.set_current_message_id(Some(session_id))` when a new session/step begins
   - On `step_finish` events, call `on_message_stop()` to mark completion

**Rationale:** Without message IDs, the `is_duplicate_final_message()` check falls back to `has_any_streamed_content()` which is fragile and can fail if state is accidentally reset.

---

### Step 3: Enforce Single Content Block Per Streaming Region

**File:** `ralph-workflow/src/json_parser/streaming_state.rs`

**Changes:**
1. When `on_content_block_start(index)` is called while `ContentBlockState::InBlock` is active:
   - First finalize the previous block (emit newline if needed)
   - Then start the new block
2. Add a warning log when consecutive `on_content_block_start` calls occur without an intervening delta, as this may indicate malformed event streams

**Rationale:** The "glued text" bug occurs when a new content block starts before the previous one is finalized, causing missing newlines between outputs.

---

### Step 4: Add Comprehensive Integration Tests for Streaming Contract

**File:** `ralph-workflow/src/json_parser/tests.rs` (expand existing)

**Add the following tests:**

1. **Test: Many tiny deltas produce exactly one prefix**
   ```rust
   #[test]
   fn test_streaming_many_tiny_deltas_single_prefix() {
       // Feed 100+ single-character deltas
       // Assert prefix count == 1
       // Assert final message appears once
       // Assert carriage returns used for in-place updates
   }
   ```

2. **Test: Final message not duplicated after streaming**
   ```rust
   #[test]
   fn test_streaming_deduplicates_final_message() {
       // Stream deltas → message_stop → Assistant event with same content
       // Assert content appears exactly once
   }
   ```

3. **Test: Content block transitions emit proper newlines**
   ```rust
   #[test]
   fn test_streaming_multiple_content_blocks_separated() {
       // Stream block 0, then block 1
       // Assert newline between blocks
       // Assert no glued text
   }
   ```

4. **Test: Snapshot-as-delta is auto-repaired**
   ```rust
   #[test]
   fn test_streaming_snapshot_as_delta_extracted() {
       // Feed deltas where second "delta" is actually full accumulated content
       // Assert extracted delta is only the new portion
       // Assert no duplication
   }
   ```

5. **Test: Finalize without streaming produces no output**
   ```rust
   #[test]
   fn test_streaming_finalize_without_deltas_no_output() {
       // message_start → message_stop (no deltas)
       // Assert empty output
   }
   ```

---

### Step 5: Add Streaming Contract Validation Module

**New File:** `ralph-workflow/src/json_parser/delta_contract.rs`

**Contents:**
1. Define `DeltaContract` trait that parsers must implement:
   ```rust
   pub trait DeltaContract {
       /// Validate that incoming text is a genuine delta, not a snapshot
       fn validate_delta(&self, text: &str, accumulated: Option<&str>) -> DeltaValidation;
   }

   pub enum DeltaValidation {
       GenuineDelta,
       SnapshotDetected { extracted_delta: String },
       InvalidInput(String),
   }
   ```

2. Move the `is_likely_snapshot` and `get_delta_from_snapshot` logic from `StreamingSession` into this module for cleaner separation
3. Add `#[cfg(test)]` helper functions to generate test fixtures for streaming scenarios

**Rationale:** Centralizing delta validation logic makes it easier to enforce the contract consistently and add new validation rules.

---

### Step 6: Fix Prefix Spam in Edge Cases

**Files:**
- `ralph-workflow/src/json_parser/claude.rs`
- `ralph-workflow/src/json_parser/streaming_state.rs`

**Changes:**

1. In `StreamingSession::on_text_delta_key()`:
   - The `is_first` check currently uses `!self.accumulated.contains_key(&content_key)`
   - This can return `true` if the key is different (e.g., Codex using dynamic item IDs)
   - Add normalization: for certain parsers, map all text content to a canonical key like `"text:0"`

2. In Claude parser:
   - Ensure `content_block_start` always uses the same index passed to subsequent `content_block_delta` events
   - The current code already does this, but add validation to catch mismatches

**Rationale:** The prefix spam bug can occur when different delta events use inconsistent keys, causing each to be treated as "first".

---

### Step 7: Ensure Proper Newline Emission on MessageStop

**File:** `ralph-workflow/src/json_parser/claude.rs` (and other parsers)

**Changes:**
1. In `parse_stream_event` for `MessageStop`:
   ```rust
   StreamInnerEvent::MessageStop => {
       let was_in_block = session.on_message_stop();
       if was_in_block || session.has_any_streamed_content() {
           format!("{}{}", c.reset(), TextDeltaRenderer::render_completion())
       } else {
           String::new()
       }
   }
   ```
   - Currently only emits newline if `was_in_block` is true
   - Should also emit if any content was streamed (to handle edge cases where `in_content_block` was never set)

2. Apply similar fix to Codex, Gemini, and OpenCode parsers

**Rationale:** The "glued text" bug can occur when `in_content_block` is false but content was still streamed through other code paths.

---

### Step 8: Update Documentation

**File:** `ralph-workflow/src/json_parser/streaming_state.rs` (module docs)

**Changes:**
1. Add detailed documentation about the streaming contract:
   - Delta vs Snapshot definitions
   - Message lifecycle (Start → ContentBlockStart → Deltas → ContentBlockStop → MessageStop)
   - When and how deduplication occurs
   - How to add support for a new parser

2. Add examples showing correct and incorrect streaming sequences

---

## Critical Files for Implementation

1. **`ralph-workflow/src/json_parser/streaming_state.rs`** - Core state machine that needs lifecycle enforcement and content block state tracking

2. **`ralph-workflow/src/json_parser/claude.rs`** - Primary parser that handles most streaming scenarios; needs newline emission fix and serves as reference implementation

3. **`ralph-workflow/src/json_parser/codex.rs`** - Needs message ID generation for proper deduplication

4. **`ralph-workflow/src/json_parser/tests.rs`** - Comprehensive integration tests to lock behavior

5. **`ralph-workflow/src/json_parser/delta_display.rs`** - Already correct, but may need minor updates to support new ContentBlockState

---

## Risks & Mitigations

### Risk 1: Breaking existing streaming behavior
**Mitigation:** The existing comprehensive test suite (1700+ lines in tests.rs) will catch regressions. Run full test suite after each change. Key tests to monitor:
- `test_ccs_glm_streaming_no_duplicate_prefix`
- `test_streaming_accumulation_behavior`
- `test_streaming_consistency_across_parsers`

### Risk 2: Performance impact from additional state tracking
**Mitigation:** The changes are O(1) state machine operations with minimal allocation. The `ContentBlockState` enum is small (32 bytes max). No measurable performance impact expected.

### Risk 3: New `expect` attributes violating CLAUDE.md
**Mitigation:** All new code will be written to pass clippy without suppressions. The existing `#[expect(clippy::cast_precision_loss)]` on line 552 of streaming_state.rs is justified and documented.

### Risk 4: Codex/OpenCode message ID changes affecting deduplication
**Mitigation:** The changes are additive - we're adding IDs where none existed. The fallback `has_any_streamed_content()` check remains as a safety net.

---

## Verification Strategy

### Pre-Implementation Checks
```bash
# Ensure clean starting state
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### Post-Implementation Verification
```bash
# 1. Check for forbidden allow/expect attributes
rg -n --pcre2 '(?x)
  \#\s*!?\[\s*
  (allow|expect)
  \s*\(
    [^()\]]*
    (?:\([^()\]]*\)[^()\]]*)*
  \)
  \s*\]
' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .
# Must produce no NEW output (existing expect attributes are pre-approved)

# 2. Full verification suite
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# 3. Run the specific streaming tests
cargo test --all-features streaming
cargo test --all-features ccs_glm
cargo test --all-features deduplication
```

### Manual Testing
1. Run ralph with ccs-glm agent on a sample task
2. Observe terminal output during streaming:
   - Should see single `[ccs-glm]` prefix
   - Text should update in-place (single line growing)
   - No duplicate output after completion
3. Verify with `ralph --verbose` to see debug output

### Success Criteria
- [ ] All existing tests pass
- [ ] New streaming contract tests pass
- [ ] No new `#[allow(dead_code)]` attributes
- [ ] `cargo clippy` produces no warnings
- [ ] Manual testing shows:
  - Exactly 1 prefix per streaming message
  - In-place updates (carriage returns visible in raw output)
  - No duplicate final message
  - Proper newlines between content blocks
