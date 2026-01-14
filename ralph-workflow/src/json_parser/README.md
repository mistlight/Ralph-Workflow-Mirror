# JSON Parser Module

This module provides streaming JSON parsers for various LLM agents (Claude, Codex, Gemini, OpenCode).

## Streaming Contract

The parsers enforce a strict **delta contract** for all streaming content to prevent duplication bugs.

### Core Principles

1. **Delta Contract**: Each streaming event must contain only the newly generated text (delta), never the full accumulated content (snapshot).

2. **Message Lifecycle**: `MessageStart` → (`ContentBlockStart` + deltas)* → `MessageStop`

3. **Deduplication Rule**: Content displayed during streaming is never re-displayed when the complete message arrives.

4. **State Reset**: Streaming state resets on `MessageStart`/Init events.

### Why This Matters

If a parser emits snapshot-style content when deltas are expected, it causes **exponential duplication bugs**. For example:
- First "delta": "Hello"
- Second "delta" (actually snapshot): "Hello World" → displays as "HelloHello World"
- Third "delta" (actually snapshot): "Hello World!" → displays as "HelloHello WorldHello World!"

### Validation

The `StreamingSession` validates incoming content:

1. **Size Threshold**: Deltas exceeding 200 characters trigger a warning (may indicate snapshot-as-delta).

2. **Pattern Detection**: Repeated large deltas (3+ occurrences) trigger a warning indicating likely snapshot-as-delta bug.

3. **Lifecycle Enforcement**: In debug builds, invalid state transitions panic.

## Event Lifecycle

```
┌──────────────────┐
│  MessageStart    │ → Reset state, prepare for new message
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ ContentBlockStart│ → Mark beginning of content block
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  TextDelta*      │ → Accumulate deltas, display with in-place updates
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  MessageStop     │ → Finalize message, add final newline
└──────────────────┘
```

## Implementation Guide

When implementing a new parser:

1. **Create a `StreamingSession`** in your parser struct:
   ```rust
   streaming_session: RefCell<StreamingSession>
   ```

2. **Call lifecycle methods** at appropriate events:
   - `on_message_start()` when a new message begins
   - `on_text_delta()` for each text chunk
   - `on_message_stop()` when message completes

3. **Check deduplication** before displaying complete messages:
   ```rust
   if session.has_any_streamed_content() {
       return String::new(); // Skip display, already streamed
   }
   ```

4. **Use the return value** from delta methods to determine prefix display:
   ```rust
   let show_prefix = session.on_text_delta(index, delta);
   ```

## Files

- **`streaming_state.rs`**: Core `StreamingSession` implementation
- **`delta_display.rs`**: `DeltaRenderer` trait for consistent display
- **`claude.rs`**: Claude API parser (primary parser for ccs-glm)
- **`codex.rs`**: Codex API parser
- **`gemini.rs`**: Gemini API parser
- **`opencode.rs`**: OpenCode API parser
- **`tests.rs`**: Comprehensive test suite

## Testing

Run tests with:
```bash
# All JSON parser tests
cargo test -p ralph-workflow json_parser::tests

# Streaming-specific tests
cargo test -p ralph-workflow streaming

# Snapshot-as-delta detection tests
cargo test -p ralph-workflow test_snapshot_as_delta
```

## Debugging

In debug builds, lifecycle violations will panic with detailed error messages showing:
- Expected states
- Actual state
- File and line number of the violation

Warnings about large deltas are always emitted to stderr to help identify potential snapshot-as-delta bugs in production.
