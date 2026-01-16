# Implementation Plan: Enhanced KMP + Rolling Hash Delta Deduplication with Strong Overlap Detection

## Summary

The codebase already has a complete KMP + Rolling Hash implementation for snapshot-as-delta detection (`deduplication.rs`). This plan enhances it with **strong overlap detection** to prevent false positives that could incorrectly dedupe intentional repetitions. The enhancement adds configurable thresholds (minimum character count AND percentage of overlap), boundary sanity checks (whitespace/punctuation/newline), consecutive duplicate tracking, and special handling for short chunks. This ensures we only dedupe when overlap is "strong" and meets safe boundary conditions, distinguishing between agent resend bugs and legitimate repeated content.

## Current State Assessment

**Already Implemented** (`deduplication.rs:1-887`):
- `RollingHashWindow`: Polynomial rolling hash with base 256, modulus 2^31-1 (Rabin-Karp)
- `KMPMatcher`: Failure function precomputation, O(n+m) search
- `DeltaDeduplicator`: Two-phase algorithm (rolling hash filter → KMP verification)
- `extract_new_content()`: Returns new portion when delta starts with accumulated
- `is_likely_snapshot()`: Fast O(n) check using rolling hash only
- 25+ unit tests covering rolling hash, KMP, and integration scenarios
- Integrated into `StreamingSession` via `is_likely_snapshot()` and `get_delta_from_snapshot()`

**What's Missing** (addressed by this proposal):
- No minimum overlap threshold (currently dedupes ANY overlap at position 0)
- No boundary sanity checks (overlap could end mid-word/sentence)
- No consecutive duplicate tracking (no "3 strikes" heuristic for repeated identical chunks)
- No special handling for short chunks (".", "\n", "Ok" get deduped inappropriately)

## Implementation Steps

### Step 1: Add Configuration Constants for Overlap Detection

**File**: `ralph-workflow/src/json_parser/deduplication.rs`

**Purpose**: Define tunable thresholds for strong overlap detection.

**Changes**:
- Add `MIN_OVERLAP_CHARS: usize = 30` (minimum character count for dedupe)
- Add `MIN_OVERLAP_RATIO: f64 = 0.5` (50% of delta must be overlap)
- Add `SHORT_CHUNK_THRESHOLD: usize = 20` (below this, never dedupe unless exact repeat)
- Add `CONSECUTIVE_DUPLICATE_THRESHOLD: usize = 3` (trigger after N identical chunks)

**Rationale**: These constants are documented and can be tuned based on production feedback.

---

### Step 2: Implement Boundary Detection Helper

**File**: `ralph-workflow/src/json_parser/deduplication.rs`

**Purpose**: Check if a character position ends at a safe boundary for deduplication.

**Changes**:
- Add `is_safe_boundary(text: &str, pos: usize) -> bool` function
- Check if character at `pos` is whitespace, punctuation, or newline
- Use Unicode-aware character classification (not just ASCII)
- Return `true` if boundary is safe for deduplication

**Code**:
```rust
fn is_safe_boundary(text: &str, pos: usize) -> bool {
    if pos >= text.len() {
        return true; // End of string is always safe
    }
    
    let c = text[pos..].chars().next().unwrap();
    
    // Safe boundaries: whitespace, punctuation, newline
    c.is_whitespace() || c.is_ascii_punctuation() || c == '\n' || c == '\r'
}
```

---

### Step 3: Implement Overlap Quality Scoring

**File**: `ralph-workflow/src/json_parser/deduplication.rs`

**Purpose**: Score the "strength" of an overlap to determine if it's worth deduplicating.

**Changes**:
- Add `OverlapScore` struct with fields: `char_count`, `ratio`, `is_safe_boundary`
- Add `score_overlap(delta: &str, accumulated: &str) -> OverlapScore` function
- Use existing KMP to find overlap length
- Calculate ratio as `overlap_len / delta.len()`
- Check boundary safety at overlap end position

**Rationale**: Centralizes overlap quality logic for reuse in detection and extraction.

---

### Step 4: Add Consecutive Duplicate Tracking

**File**: `ralph-workflow/src/json_parser/streaming_state.rs`

**Purpose**: Detect when the exact same chunk arrives repeatedly ( resend glitch).

**Changes**:
- Add `consecutive_duplicates: HashMap<(ContentType, String), (usize, String)>` field
  - Key: `(content_type, index)`
  - Value: `(count, last_hash)` where count is consecutive occurrences, hash is for comparison
- Add `ConsecutiveDuplicateState` enum: `None | Suspected(count) | Confirmed(count)`
- Update `on_text_delta()` to increment counter when delta hash matches previous
- Reset counter on any different delta

**Rationale**: "3 strikes" heuristic only applies to identical chunks, not generic repeats.

---

### Step 5: Enhance `is_likely_snapshot()` with Strong Overlap Checks

**File**: `ralph-workflow/src/json_parser/deduplication.rs`

**Purpose**: Make snapshot detection stricter to avoid false positives on intentional repetitions.

**Changes**:
- Add new method `is_likely_snapshot_with_thresholds(delta: &str, accumulated: &str) -> bool`
- Call existing `is_likely_snapshot()` for hash check
- If hash matches, use `score_overlap()` to get overlap metrics
- Apply thresholds: `score.char_count >= MIN_OVERLAP_CHARS AND score.ratio >= MIN_OVERLAP_RATIO`
- Check `score.is_safe_boundary` is true
- For short chunks (`delta.len() < SHORT_CHUNK_THRESHOLD`), only dedupe if `delta == accumulated` (exact match)
- Return `true` only if ALL conditions pass

**Rationale**: Prevents deduping legitimate repetitions that happen to start with previous content.

---

### Step 6: Enhance `extract_new_content()` with Boundary Checks

**File**: `ralph-workflow/src/json_parser/deduplication.rs`

**Purpose**: Only extract new content if overlap meets quality thresholds.

**Changes**:
- Add new method `extract_new_content_with_thresholds<'a>(delta: &'a str, accumulated: &str) -> Option<&'a str>`
- Use existing rolling hash + KMP to find overlap
- Before returning `Some(&delta[overlap_len..])`, verify:
  - `overlap_len >= MIN_OVERLAP_CHARS`
  - `(overlap_len as f64 / delta.len() as f64) >= MIN_OVERLAP_RATIO`
  - `is_safe_boundary(delta, overlap_len)` is true
- Return `None` if any condition fails (treat as genuine delta)

**Rationale**: Ensures we only dedupe when overlap is "strong" and ends at safe boundary.

---

### Step 7: Add Consecutive Duplicate Handling

**File**: `ralph-workflow/src/json_parser/streaming_state.rs`

**Purpose**: Implement "3 strikes" heuristic for repeated identical chunks.

**Changes**:
- In `on_text_delta()`, compute hash of incoming delta
- Check `consecutive_duplicates` map for previous hash at same key
- If hash matches:
  - Increment count
  - If count >= `CONSECUTIVE_DUPLICATE_THRESHOLD`:
    - Skip rendering entirely (drop the delta)
    - Log warning about resend glitch
- If hash doesn't match:
  - Reset count to 1
  - Update stored hash
- Clear map in `on_message_start()` and `on_content_block_start()` for different indices

**Rationale**: Aggressive deduplication only for proven resend glitches, not intentional content.

---

### Step 8: Update StreamingSession Integration

**File**: `ralph-workflow/src/json_parser/streaming_state.rs`

**Purpose**: Replace old snapshot detection calls with enhanced threshold-aware versions.

**Changes**:
- Update `is_likely_snapshot()` at line 655 to use `DeltaDeduplicator::is_likely_snapshot_with_thresholds()`
- Update `get_delta_from_snapshot()` at line 658 to use `DeltaDeduplicator::extract_new_content_with_thresholds()`
- Add verbose logging when threshold checks fail (for debugging)
- Update `snapshot_repairs_count` to only count repairs that pass threshold checks

**Rationale**: Apply new strong overlap logic to all delta processing.

---

### Step 9: Add Comprehensive Test Coverage

**File**: `ralph-workflow/src/json_parser/deduplication.rs` (tests module)

**Purpose**: Ensure new threshold logic works correctly and doesn't regress existing behavior.

**Tests to Add**:
- `test_strong_overlap_meets_char_threshold()`: Verify 30+ char overlap passes
- `test_strong_overlap_meets_ratio_threshold()`: Verify 50%+ ratio passes
- `test_strong_overlap_fails_char_threshold()`: Verify <30 chars fails even if ratio good
- `test_strong_overlap_fails_ratio_threshold()`: Verify <50% ratio fails even if chars good
- `test_boundary_check_whitespace()`: Verify whitespace boundary passes
- `test_boundary_check_punctuation()`: Verify punctuation boundary passes
- `test_boundary_check_mid_word_fails()`: Verify mid-word boundary fails
- `test_short_chunk_never_deduped()`: Verify <20 char chunks never deduped unless exact match
- `test_short_chunk_exact_match_deduped()`: Verify exact match short chunks ARE deduped
- `test_consecutive_duplicates_triggers()`: Verify 3+ identical chunks trigger aggressive dedupe
- `test_consecutive_duplicates_reset_on_change()`: Verify counter resets on different content
- `test_intentional_repetition_not_deduped()`: Verify legitimate repeated content passes through
- `test_snapshot_strong_overlap_deduped()`: Verify actual snapshots still detected and deduped

**File**: `ralph-workflow/src/json_parser/streaming_state.rs` (tests module)

**Tests to Add**:
- `test_streaming_consecutive_duplicate_tracking()`: Integration test for 3-strikes heuristic
- `test_streaming_boundary_aware_deduplication()`: Full flow with boundary checks
- `test_streaming_short_chunk_passthrough()`: Verify short chunks render correctly

---

### Step 10: Add Environment Variable Configuration

**File**: `ralph-workflow/src/json_parser/deduplication.rs`

**Purpose**: Allow tuning thresholds without recompilation for production experimentation.

**Changes**:
- Add environment variable readers for each threshold:
  - `RALPH_STREAMING_MIN_OVERLAP_CHARS` (default 30, range 10-100)
  - `RALPH_STREAMING_MIN_OVERLAP_RATIO` (default 0.5, range 0.1-0.9)
  - `RALPH_STREAMING_SHORT_CHUNK_THRESHOLD` (default 20, range 5-50)
  - `RALPH_STREAMING_CONSECUTIVE_DUPLICATE_THRESHOLD` (default 3, range 2-10)
- Use `OnceLock` for lazy initialization (like existing `snapshot_threshold()`)
- Validate ranges and fall back to defaults if invalid
- Add `fn get_overlap_thresholds() -> OverlapThresholds` accessor

**File**: `docs/RFC/RFC-003-streaming-architecture-hardening.md`

**Changes**:
- Document new environment variables in the quick reference section
- Update "Open Questions" section to note threshold tuning is now configurable

**Rationale**: Enables production tuning without code changes.

---

### Step 11: Update Documentation and RFC

**File**: `docs/RFC/RFC-003-streaming-architecture-hardening.md`

**Purpose**: Document the enhanced deduplication approach.

**Changes**:
- Add new section: "Enhanced Snapshot Detection with Strong Overlap"
- Document the two-phase approach (rolling hash → KMP → threshold checks → boundary checks)
- Explain "3 strikes" heuristic for consecutive duplicates
- Update Issue 1 (Snapshot-as-Delta) to reference enhanced detection
- Add changelog entry for the enhancement

**File**: `ralph-workflow/src/json_parser/deduplication.rs` (module documentation)

**Changes**:
- Update module-level docs to describe strong overlap detection
- Add examples showing threshold behavior
- Document boundary detection rules

---

### Step 12: Verify All Existing Tests Still Pass

**Purpose**: Ensure no regressions in existing behavior.

**Steps**:
1. Run `cargo test -p ralph-workflow streaming` - all streaming tests pass
2. Run `cargo test -p ralph-workflow test_snapshot` - all snapshot tests pass
3. Run `cargo test -p ralph-workflow test_dedup` - all deduplication tests pass
4. Run `cargo test -p ralph-workflow json_parser::tests` - all parser integration tests pass
5. Run `cargo clippy --all-targets --all-features -- -D warnings` - no new warnings
6. Run `cargo fmt --all` - code is formatted
7. Run `rg -n -U --pcre2 '(?x)\#\s*!?\[\s*(allow|expect)\s*\(' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .` - no suppressed warnings

**Expected Outcome**: All existing tests pass, no clippy warnings, no dead code suppression.

---

## Critical Files for Implementation

### `ralph-workflow/src/json_parser/deduplication.rs`
- **Justification**: Contains the core KMP + Rolling Hash implementation that needs enhancement. Will add threshold logic, boundary detection, overlap scoring, and consecutive duplicate handling here.
- **Key Changes**:
  - Add configuration constants (Step 1)
  - Add `is_safe_boundary()` helper (Step 2)
  - Add `OverlapScore` struct and `score_overlap()` function (Step 3)
  - Add `is_likely_snapshot_with_thresholds()` method (Step 5)
  - Add `extract_new_content_with_thresholds()` method (Step 6)
  - Add environment variable readers (Step 10)
  - Add comprehensive test coverage (Step 9)
  - Update module documentation (Step 11)

### `ralph-workflow/src/json_parser/streaming_state.rs`
- **Justification**: Integrates deduplication into the delta processing flow. Needs to track consecutive duplicates and use enhanced threshold methods.
- **Key Changes**:
  - Add `consecutive_duplicates` HashMap field (Step 4)
  - Add `ConsecutiveDuplicateState` enum (Step 4)
  - Update `on_text_delta()` to use threshold-aware methods (Step 8)
  - Update `is_likely_snapshot()` call at line 655 (Step 8)
  - Update `get_delta_from_snapshot()` call at line 658 (Step 8)
  - Add integration tests (Step 9)

### `docs/RFC/RFC-003-streaming-architecture-hardening.md`
- **Justification**: Documents the streaming architecture and must reflect the enhanced deduplication approach.
- **Key Changes**:
  - Add "Enhanced Snapshot Detection" section (Step 11)
  - Document new environment variables (Step 10)
  - Update Issue 1 with enhanced detection details (Step 11)
  - Add changelog entry (Step 11)

### `ralph-workflow/src/json_parser/tests.rs`
- **Justification**: Contains integration tests for the full parsing flow. Needs tests for the new threshold behavior in realistic scenarios.
- **Key Changes**:
  - Add integration test for boundary-aware deduplication (Step 9)
  - Add integration test for short chunk handling (Step 9)
  - Add integration test for consecutive duplicate handling (Step 9)

### `ralph-workflow/src/json_parser/claude.rs` (and other parsers)
- **Justification**: May need to update any direct calls to deduplication methods if they exist, though most flow goes through StreamingSession.
- **Key Changes**:
  - Audit for any direct calls to `DeltaDeduplicator` methods
  - Update to use threshold-aware variants if needed
  - Add parser-specific tests for new behavior (Step 9)

---

## Risks & Mitigations

### Risk 1: False Positives from Threshold Tuning

**Risk**: Overly aggressive thresholds could cause legitimate snapshots to not be deduped, leading to content duplication in the UI.

**Mitigation**:
- Start with conservative defaults (30 chars, 50% ratio) based on the proposal
- Make thresholds configurable via environment variables for production tuning
- Add verbose logging when threshold checks fail to aid debugging
- Comprehensive test coverage for edge cases

### Risk 2: Boundary Detection Breaking Non-English Languages

**Risk**: `is_ascii_punctuation()` might not handle all Unicode punctuation correctly, causing boundary checks to fail for international content.

**Mitigation**:
- Use Unicode-aware character classification where possible
- Test with CJK, Arabic, RTL languages
- Consider using `unicode-segmentation` crate for proper grapheme detection
- Make boundary checks configurable (can disable if problematic)

### Risk 3: Consecutive Duplicate Tracking Memory Overhead

**Risk**: Storing hashes for all content keys could consume significant memory in long-running sessions with many content blocks.

**Mitigation**:
- Store only the most recent hash per key (not full history)
- Use 64-bit hashes (already using DefaultHasher) to minimize memory
- Clear tracking in `on_message_start()` and `on_content_block_start()`
- Consider using LRU cache if memory becomes an issue

### Risk 4: Performance Impact from Additional Checks

**Risk**: Threshold and boundary checks add overhead to every delta, potentially impacting streaming performance.

**Mitigation**:
- Existing KMP + Rolling Hash already does O(n+m) work
- Threshold checks are O(1) after KMP finds overlap
- Boundary check is O(1) character inspection
- Most deltas should fail fast on the initial rolling hash check
- Benchmark before/after to quantify any slowdown

### Risk 5: Regression in Existing Snapshot Detection

**Risk**: New threshold logic could cause existing snapshot-as-delta bugs to no longer be detected, reverting to exponential duplication.

**Mitigation**:
- Comprehensive test coverage for existing snapshot detection scenarios
- Run all existing tests before and after changes
- Add specific test for "real GLM snapshot bug" scenario
- Make threshold logic opt-in initially, gate behind feature flag if needed
- Monitor `snapshot_repairs_count` metric in production

---

## Verification Strategy

### Unit Tests for New Functionality

**File**: `deduplication.rs` tests module

```bash
# Run all deduplication tests
cargo test -p ralph-workflow test_dedup

# Run specific test categories
cargo test -p ralph-workflow test_strong_overlap
cargo test -p ralph-workflow test_boundary_check
cargo test -p ralph-workflow test_short_chunk
cargo test -p ralph-workflow test_consecutive_duplicates
```

**Expected**: All new tests pass, covering:
- Overlap thresholds (char count and ratio)
- Boundary detection (whitespace, punctuation, mid-word)
- Short chunk handling (exact match vs. partial overlap)
- Consecutive duplicate tracking (counter increment and reset)

### Integration Tests for Full Flow

**File**: `streaming_state.rs` and `tests.rs`

```bash
# Run streaming state tests
cargo test -p ralph-workflow test_streaming

# Run parser integration tests
cargo test -p ralph-workflow json_parser::tests

# Run snapshot detection tests
cargo test -p ralph-workflow test_snapshot
```

**Expected**: All tests pass, including:
- Full delta processing with threshold checks
- Boundary-aware deduplication in realistic scenarios
- Short chunks pass through correctly
- Consecutive duplicates trigger aggressive dedupe

### Regression Tests for Existing Behavior

**Commands**:
```bash
# All streaming-related tests
cargo test -p ralph-workflow streaming

# All deduplication tests (old and new)
cargo test -p ralph-workflow dedup

# Full test suite
cargo test --all-features
```

**Expected**: No regressions—all existing tests still pass.

### Code Quality Checks

**Commands**:
```bash
# Check for suppressed warnings (should produce NO output)
rg -n -U --pcre2 '(?x)\#\s*!?\[\s*(allow|expect)\s*\(' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# Check cfg_attr suppressed warnings (should produce NO output)
rg -n -U --pcre2 '(?x)\#\s*!?\[\s*cfg_attr\s*\([^()]*?\b(allow|expect)\s*\(' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# Format check
cargo fmt --all -- --check

# Clippy check
cargo clippy --all-targets --all-features -- -D warnings
```

**Expected**: All checks pass with no output or errors.

### Manual Verification Scenarios

**Scenario 1: Legitimate Repetition Passes Through**
- Agent sends: "Hello" → "Hello World" → "Hello World! Hello World!"
- Expected: All three render (no dedupe on third, even though it repeats "Hello World")

**Scenario 2: Snapshot Bug Still Detected**
- Agent sends: "Hello" → "Hello World" (snapshot) → "Hello World!" (snapshot)
- Expected: Only "Hello World!" renders (snapshots detected and deduped)

**Scenario 3: Short Chunk Passthrough**
- Agent sends: "." → "." → "." (three periods as separate deltas)
- Expected: All three periods render (short chunks not deduped unless exact match of entire accumulated)

**Scenario 4: Consecutive Duplicate Aggressive Dedupe**
- Agent sends: "test" → "test" → "test" → "test" (identical chunk 4 times)
- Expected: First three render, fourth is dropped (3-strikes heuristic)

**Scenario 5: Boundary Awareness**
- Accumulated: "Hello World"
- Delta: "Hello World!" (overlap ends at safe boundary: space before "!")
- Expected: Deduped, only "!" renders

**Scenario 6: Boundary Fails Mid-Word**
- Accumulated: "Hello"
- Delta: "HelloWorld" (overlap ends mid-"word")
- Expected: NOT deduped, full "HelloWorld" renders (boundary not safe)

### Success Criteria

**All acceptance checks must be satisfied**:

1. **Thresholds Applied**: Deltas only deduped when overlap >= 30 chars AND >= 50% of delta
2. **Boundaries Checked**: Dedupe only when overlap ends at whitespace/punctuation/newline
3. **ShortChunks Pass**: <20 char chunks never deduped unless exact match
4. **Consecutive Duplicates**: 3+ identical chunks trigger aggressive drop
5. **Existing Behavior**: All existing snapshot detection still works
6. **Tests Pass**: All unit and integration tests pass
7. **No Code Quality Issues**: No clippy warnings, no dead code suppression
8. **Documentation Updated**: RFC and module docs reflect new behavior
9. **Configurable**: Thresholds tunable via environment variables

---

## Migration Path

The implementation follows the phased approach outlined in the user's proposal:

1. **Phase 1**: Implement new functionality alongside existing (Steps 1-7)
2. **Phase 2**: Integrate into StreamingSession (Steps 8-9)
3. **Phase 3**: Add configuration and documentation (Steps 10-11)
4. **Phase 4**: Verify and validate (Step 12)

No breaking changes to existing APIs—all enhancements are additive. The new threshold-aware methods can be adopted incrementally.

---

## Open Questions for Clarification

1. **Should boundary checks be configurable?** If non-English languages have issues, should we add an environment variable to disable boundary checks?

2. **What happens when overlap is "moderate"?** The proposal says "do nothing (append all)" for moderate overlap. Should we add a "warning" mode that logs but doesn't dedupe?

3. **Should consecutive duplicate tracking persist across MessageStart?** The proposal says "clear on MessageStart" but resend glitches might span message boundaries. Should this be configurable?

4. **Performance budget?** What's the acceptable overhead for these checks? If benchmarks show >10% slowdown, should we make threshold checks opt-in?

These questions can be answered during implementation based on findings from test coverage and early production usage.
