# RFC-003 Streaming Bug Fix Implementation Plan

## Summary

This plan addresses a critical bug in RFC-003's streaming architecture where **debug output (`[DEBUG]` prefix) is not flushed immediately**, causing it to be lost or overwritten when subsequent output is written. The fix ensures that debug JSON output in streaming parsers is flushed immediately after writing, just like the parsed event output. This is necessary because debug mode output must appear synchronously with the streaming events it describes, otherwise the user sees incomplete/corrupted output where the `[DEBUG]` line appears truncated or missing.

## Implementation Steps

1. **Fix debug output flush in Claude parser** (`claude.rs:863-873`)
   - Add `writer.flush()?` immediately after the debug `writeln!` call
   - This ensures debug output appears before the parsed event output
   - Location: `ralph-workflow/src/json_parser/claude.rs` around line 873

2. **Fix debug output flush in Codex parser** (`codex/mod.rs`)
   - Find the equivalent debug output section (if exists)
   - Add `writer.flush()?` after debug `writeln!`
   - Location: `ralph-workflow/src/json_parser/codex/mod.rs`

3. **Fix debug output flush in Gemini parser** (`gemini.rs`)
   - Find the equivalent debug output section (if exists)
   - Add `writer.flush()?` after debug `writeln!`
   - Location: `ralph-workflow/src/json_parser/gemini.rs`

4. **Fix debug output flush in OpenCode parser** (`opencode.rs`)
   - Find the equivalent debug output section (if exists)
   - Add `writer.flush()?` after debug `writeln!`
   - Location: `ralph-workflow/src/json_parser/opencode.rs`

5. **Add test coverage for debug output flushing**
   - Create a test that verifies debug output is flushed before event output
   - Test should simulate the real-world scenario where debug and event output are mixed
   - Location: `ralph-workflow/src/json_parser/tests.rs`

6. **Update RFC-003 changelog**
   - Document the bug fix
   - Update status if needed
   - Location: `docs/RFC/RFC-003-streaming-architecture-hardening.md`

7. **Run verification**
   - Run `cargo fmt --all`
   - Run `cargo clippy --all-targets --all-features -- -D warnings`
   - Run `cargo test --all-features`

## Critical Files for Implementation

- `ralph-workflow/src/json_parser/claude.rs:863-873` - **PRIMARY FIX**: Add flush after debug output `writeln!` to ensure `[DEBUG]` lines appear synchronously with streaming events. Without this flush, debug output gets buffered and may be overwritten by subsequent event output.

- `ralph-workflow/src/json_parser/codex/mod.rs` - Check for equivalent debug output pattern and add flush if present.

- `ralph-workflow/src/json_parser/gemini.rs` - Check for equivalent debug output pattern and add flush if present.

- `ralph-workflow/src/json_parser/opencode.rs` - Check for equivalent debug output pattern and add flush if present.

- `ralph-workflow/src/json_parser/tests.rs` - Add test to verify debug output flushing behavior.

- `docs/RFC/RFC-003-streaming-architecture-hardening.md` - Document the fix in changelog.

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| **Additional flush() calls may impact performance** | The flush is only called in debug mode (`verbosity.is_debug()`), which is not the production default. Performance impact is acceptable for debug scenarios. |
| **Other parsers may not have debug output** | Will check each parser individually. Only add flush where debug output exists. |
| **Test may be flaky due to timing** | Test will use a mock writer that tracks flush calls, not timing-based assertions. |
| **RFC status may need updating** | This is a bug fix, not a feature change. Status remains "Implemented" but changelog gets updated. |

## Verification Strategy

1. **Manual verification** - Run ralph in debug mode (`-vvvv` or `--verbosity=debug`) and verify that:
   - `[DEBUG]` lines appear immediately before their corresponding event output
   - No `[DEBUG]` output is truncated or missing
   - Output is not cut off mid-line

2. **Automated test** - Add test in `tests.rs`:
   ```rust
   #[test]
   fn test_debug_output_flushed_immediately() {
       // Create a mock writer that tracks flush calls
       // Parser in debug mode
       // Feed streaming JSON events
       // Verify flush() was called after each debug writeln!
   }
   ```

3. **Run existing tests** - Ensure no regressions:
   ```bash
   cargo test -p ralph-workflow json_parser
   cargo test -p ralph-workflow streaming
   ```

4. **Clippy/fmt checks** - Must pass with no warnings:
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   ```

5. **Success criteria**:
   - Debug output (`[DEBUG] {...}`) appears before each parsed event output
   - No truncated debug lines
   - All existing tests pass
   - No new clippy warnings
   - User can see full debug output in real-time during streaming
