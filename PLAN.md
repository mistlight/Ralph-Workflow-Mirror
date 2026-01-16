# Implementation Plan: Fix Duplicate Output and Blank Lines in Streaming

## Summary

This plan addresses two related but distinct issues in the streaming output system:

1. **Blank lines from control events**: Events like `content_block_stop`, `message_delta`, and `message_stop` that don't contain user-facing content are producing blank lines in the output.

2. **Duplicate output**: The same content is being rendered multiple times, creating a "stuttering" effect where identical text appears repeatedly in the stream.

**ROOT CAUSE ANALYSIS:**

After thorough code analysis, the issues stem from:

1. **Duplicate output bug**: The `processed_deltas` HashSet (used for delta-level deduplication) is cleared when handling repeated `MessageStart` events (lines 574, 589 in `streaming_state.rs`), but is NOT preserved like `output_started_for_key`. When agents like GLM/ccs-glm send repeated `MessageStart` events during streaming (a known protocol quirk), the same delta can be processed multiple times.

2. **Blank lines issue**: The Claude parser already filters control events (commit `9ca3dc3`), but other parsers (Codex, Gemini, OpenCode) may not have the same level of filtering.

**IMPORTANT:** The prefix trie data structure is ALREADY IMPLEMENTED (lines 102-234 in `streaming_state.rs`) and is working correctly. The issue is not with the data structure, but with how `processed_deltas` is handled during repeated MessageStart events.

The solution involves two targeted fixes:
1. **Preserve `processed_deltas` during repeated MessageStart**: Similar to how `output_started_for_key` is preserved, we must preserve `processed_deltas` to prevent duplicate delta processing
2. **Audit control event filtering in all parsers**: Ensure all parsers properly filter control events

## Implementation Steps

### Step 1: Preserve `processed_deltas` During Repeated MessageStart

**File**: `ralph-workflow/src/json_parser/streaming_state.rs`

**What to accomplish**:
- Preserve `processed_deltas` HashSet when handling repeated `MessageStart` events during active streaming
- This prevents the same delta from being processed multiple times when agents send repeated `MessageStart` events

**Why**:
- GLM/ccs-glm agents send repeated `MessageStart` events during streaming (a protocol quirk)
- Currently, `processed_deltas` is cleared but not preserved, allowing duplicate delta processing
- `output_started_for_key` is already preserved to prevent prefix spam - we should do the same for `processed_deltas`

**Implementation details**:
Modify the `on_message_start()` method around line 564:

```rust
if is_mid_stream_restart {
    // Track protocol violation
    self.protocol_violations += 1;
    // Log the contract violation for debugging (only if verbose warnings enabled)
    if self.verbose_warnings {
        eprintln!(
            "Warning: Received MessageStart while state is Streaming. \
            This indicates a non-standard agent protocol (e.g., GLM sending \
            repeated MessageStart events). Preserving output_started_for_key \
            AND processed_deltas to prevent duplicate processing. File: streaming_state.rs, Line: {}",
            line!()
        );
    }

    // Preserve both output_started_for_key AND processed_deltas to prevent duplicates
    let preserved_output_started = std::mem::take(&mut self.output_started_for_key);
    let preserved_processed_deltas = std::mem::take(&mut self.processed_deltas);

    self.state = StreamingState::Idle;
    self.streamed_types.clear();
    self.current_block = ContentBlockState::NotInBlock;
    self.accumulated.clear();
    self.key_order.clear();
    self.delta_sizes.clear();
    self.last_rendered.clear();
    self.rendered_content.clear();
    // Note: processed_deltas is NOT cleared here, it's preserved above

    // Restore preserved state
    self.output_started_for_key = preserved_output_started;
    self.processed_deltas = preserved_processed_deltas;  // ADD THIS LINE
}
```

### Step 2: Add Test for Repeated MessageStart Delta Preservation

**File**: `ralph-workflow/src/json_parser/streaming_state.rs` (in the `tests` module)

**What to accomplish**:
- Add test `test_repeated_message_start_preserves_processed_deltas()`
- Verify that the same delta sent after a repeated MessageStart is not processed twice

**Dependencies**: Step 1 must be completed first

**Why**:
- Ensures the fix for duplicate delta processing works correctly
- Prevents regression if the repeated MessageStart handling is modified

**Implementation details**:
```rust
#[test]
fn test_repeated_message_start_preserves_processed_deltas() {
    let mut session = StreamingSession::new();
    session.on_message_start();
    session.on_content_block_start(0);

    // First delta - should be processed
    let delta_hash = 12345_u64;
    assert!(!session.is_delta_processed(delta_hash));
    session.mark_delta_processed(delta_hash);
    assert!(session.is_delta_processed(delta_hash));

    // Repeated MessageStart (simulating GLM protocol quirk)
    session.on_message_start();

    // After repeated MessageStart, processed_deltas should be preserved
    assert!(
        session.is_delta_processed(delta_hash),
        "Delta should still be marked as processed after repeated MessageStart"
    );

    // Sending the same delta again should be detected as duplicate
    assert!(
        session.is_delta_processed(delta_hash),
        "Same delta should be detected as duplicate"
    );
}
```

### Step 3: Audit Control Event Filtering in All Parsers

**Files**:
- `ralph-workflow/src/json_parser/codex/mod.rs`
- `ralph-workflow/src/json_parser/codex/event_handlers.rs`
- `ralph-workflow/src/json_parser/gemini.rs`
- `ralph-workflow/src/json_parser/opencode.rs`

**What to accomplish**:
- Audit each parser to ensure control events return `String::new()` immediately
- Identify any code paths where control events might produce output
- Document any differences from the Claude parser's implementation

**Dependencies**: None (can be done in parallel with Step 1)

**Why**:
- Claude parser already has proper control event filtering (commit `9ca3dc3`)
- Other parsers may have gaps in their control event handling
- Blank lines may be coming from parsers other than Claude

**Implementation details**:
For each parser:
1. Identify all control events (start, stop, delta, ping, etc.)
2. Verify each returns `String::new()` immediately
3. Check for any code paths that might bypass the early return
4. Document findings

### Step 4: Fix Control Event Filtering in Other Parsers

**Files**: Based on findings from Step 3

**What to accomplish**:
- Add control event filtering to any parser that's missing it
- Ensure all parsers consistently return `String::new()` for control events
- Add comments explaining why control events produce no output

**Dependencies**: Step 3 must be completed first

**Why**:
- Consistency across all parsers
- Prevents blank lines from any parser

**Implementation details**:
Pattern to follow (from claude.rs lines 244-254):
```rust
StreamInnerEvent::ContentBlockStop { .. } => {
    // Content block completion event - no output needed
    // This event marks the end of a content block but doesn't produce
    // any displayable content. It's a control event for state management.
    String::new()
}
StreamInnerEvent::MessageDelta { .. } => {
    // Message delta event with usage/metadata - no output needed
    // This event contains final message metadata (stop_reason, usage stats)
    // but is used for tracking/monitoring purposes only, not display.
    String::new()
}
```

### Step 5: Add Control Event Tests for All Parsers

**Files**:
- `ralph-workflow/src/json_parser/codex_tests.rs`
- `ralph-workflow/src/json_parser/gemini_tests.rs`
- `ralph-workflow/src/json_parser/opencent_tests.rs`

**What to accomplish**:
- Add tests verifying control events produce no output
- Test each parser's specific control events

**Dependencies**: Step 4 must be completed first

**Why**:
- Ensures control event filtering works correctly
- Prevents regressions

**Implementation details**:
```rust
#[test]
fn test_control_events_produce_no_output() {
    let parser = Parser::new(Colors { enabled: false }, Verbosity::Normal);

    // Test various control events
    let control_events = vec![
        // Add parser-specific control events here
    ];

    for event_json in control_events {
        let output = parser.parse_event(&event_json);
        assert!(
            output.is_none() || output.unwrap().is_empty(),
            "Control event should produce no output: {}",
            event_json
        );
    }
}
```

### Step 6: Integration Test for ccs-glm Event Sequence

**File**: `ralph-workflow/src/json_parser/tests.rs`

**What to accomplish**:
- Add test simulating the exact ccs-glm event sequence that produces duplicates
- Verify the fix prevents duplicate output
- Test includes repeated MessageStart events

**Dependencies**: Steps 1-2 must be completed first

**Why**:
- Validates the fix works for the real-world scenario reported by the user
- Ensures no regressions in GLM/ccs-glm protocol handling

**Implementation details**:
```rust
#[test]
fn test_ccs_glm_duplicate_prevention() {
    let parser = ClaudeParser::new(
        Colors { enabled: false },
        Verbosity::Normal
    ).with_display_name("ccs-glm");

    // Simulate ccs-glm event sequence:
    // 1. MessageStart
    // 2. ContentBlockStart
    // 3. ContentBlockDelta with "Hello"
    // 4. REPEATED MessageStart (GLM quirk)
    // 5. ContentBlockDelta with "Hello" again (should be deduplicated)

    let events = vec![
        // Add actual event JSON here
    ];

    let mut outputs = Vec::new();
    for event in events {
        if let Some(output) = parser.parse_event(&event) {
            if !output.is_empty() {
                outputs.push(output);
            }
        }
    }

    // Verify "Hello" appears only once
    assert_eq!(
        outputs.iter().filter(|o| o.contains("Hello")).count(),
        1,
        "Content should appear only once despite repeated MessageStart"
    );
}
```

## Critical Files for Implementation

1. **`ralph-workflow/src/json_parser/streaming_state.rs`** (~2,555 lines)
   - **Line ~564**: Add `preserved_processed_deltas` to preserve delta tracking during repeated MessageStart
   - **Line ~577**: Restore `processed_deltas` after clearing state
   - **Tests module**: Add `test_repeated_message_start_preserves_processed_deltas()`
   - **Why**: This is the ROOT CAUSE of the duplicate output bug

2. **`ralph-workflow/src/json_parser/claude.rs`** (~1,033 lines)
   - **Lines 244-254**: Verify control event filtering is complete (already done in commit `9ca3dc3`)
   - **Lines 606-651**: Verify delta processing logic correctly uses `processed_deltas`
   - **Why**: Ensure the Claude parser continues to work correctly

3. **`ralph-workflow/src/json_parser/codex/mod.rs`** (~200+ lines)
   - Audit for control event handling
   - Add filtering if missing

4. **`ralph-workflow/src/json_parser/gemini.rs`** (~500+ lines)
   - Audit for control event handling
   - Add filtering if missing

5. **`ralph-workflow/src/json_parser/opencode.rs`** (~500+ lines)
   - Audit for control event handling
   - Add filtering if missing

6. **`ralph-workflow/src/json_parser/tests.rs`** (~200+ lines)
   - Add integration test for ccs-glm duplicate prevention

## Risks & Mitigations

### Risk 1: Preserving `processed_deltas` May Cause False Positives
**Challenge**: If `processed_deltas` is preserved across repeated MessageStart events, legitimate new deltas with the same hash might be incorrectly skipped.

**Mitigation**:
- Delta hash is computed from the exact delta content, so a legitimate new delta with the same hash would be identical byte-for-byte
- This is acceptable behavior - if the delta content is identical, it should be deduplicated
- In the extremely unlikely case of a hash collision, the worst outcome is skipping a delta that would have shown the same content

### Risk 2: Memory Usage from Preserving `processed_deltas`
**Challenge**: Preserving `processed_deltas` across repeated MessageStart events could allow it to grow unbounded.

**Mitigation**:
- `processed_deltas` is cleared on normal message boundaries (when state is NOT Streaming)
- It's only preserved during mid-stream restarts (repeated MessageStart during active streaming)
- The HashSet grows linearly with the number of unique deltas, which is bounded by message length
- This is acceptable memory usage for preventing duplicate output

### Risk 3: Other Parsers May Have Different Control Event Semantics
**Challenge**: Codex, Gemini, or OpenCode parsers may have different event structures where some events we consider "control" actually have user-facing meaning.

**Mitigation**:
- Carefully audit each parser's documentation and event structure
- Only filter events that are clearly state-management only
- Add tests to verify no legitimate output is being filtered

### Risk 4: Breaking Existing Functionality
**Challenge**: Changes to message start handling or control event filtering might break existing functionality.

**Mitigation**:
- The change to preserve `processed_deltas` only affects the repeated MessageStart case (a protocol violation)
- Normal message boundaries still clear all state as before
- Comprehensive tests will catch any regressions
- Manual testing with each agent type (Claude, Codex, Gemini, OpenCode)

## Verification Strategy

### Specific Tests to Run

1. **Test processed_deltas preservation**:
```bash
cargo test test_repeated_message_start_preserves_processed_deltas
```
Expected: Pass - verifies delta tracking is preserved during repeated MessageStart

2. **Test ccs-glm duplicate prevention**:
```bash
cargo test test_ccs_glm_duplicate_prevention
```
Expected: Pass - verifies no duplicate content with repeated MessageStart

3. **Test control events produce no output (all parsers)**:
```bash
cargo test test_control_events_produce_no_output
```
Expected: Pass - control events return empty/None in all parsers

4. **Full test suite**:
```bash
cargo test --all-features
```
Expected: All tests pass

### Manual Verification Steps

1. **Test with ccs-glm agent**:
   - Run a streaming session with ccs-glm
   - Verify no blank lines appear in output
   - Verify no duplicate content appears
   - Look for the specific pattern mentioned in the issue:
     - "ts (line 84 in the log) 2. **`message_delta`** events"
     - This should NOT repeat multiple times

2. **Test with long streaming content**:
   - Run a session with very long streaming content
   - Verify memory usage is reasonable (processed_deltas HashSet doesn't grow unbounded)
   - Verify no performance degradation

3. **Test across all parsers**:
   - Test with Claude, Codex, Gemini, and OpenCode parsers
   - Verify consistent behavior across all parsers
   - Verify no blank lines from any parser

### Success Criteria

1. **No duplicates**: Identical deltas are not processed multiple times, even with repeated MessageStart events
2. **No blank lines**: Control events (`content_block_stop`, `message_delta`, `message_stop`, etc.) produce no visible output in ANY parser
3. **Message boundaries**: Same content in different messages is rendered (processed_deltas cleared on normal message boundaries)
4. **All tests pass**: No regressions in existing functionality
5. **Performance acceptable**: No significant slowdown or memory increase
6. **Real-world validation**: The ccs-glm agent no longer produces duplicate content or blank lines
