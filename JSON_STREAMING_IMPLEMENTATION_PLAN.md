# JSON Streaming Support Implementation Plan

## Summary

This plan addresses the goal of improving JSON streaming support in Ralph (wt-streaming) to handle delta/partial content from AI coding agents more effectively. The current implementation ignores many delta events being streamed, resulting in health warnings like "Claude parser ignored 97.5% of events (2049 of 2102)." This plan aims to make the JSON parsers fault-tolerant, properly handle delta streaming for all agent types, and provide user-friendly display of partial vs. complete content.

## Current State Analysis

### Existing Delta Handling

**Claude Parser** (`src/json_parser/claude.rs`):
- ✅ Has delta accumulation via `DeltaAccumulator`
- ✅ Handles `StreamEvent` with nested `StreamInnerEvent`
- ✅ Supports `TextDelta`, `ThinkingDelta`, and `ToolUseDelta`
- ⚠️ Only shows deltas in verbose mode (`is_verbose()`)
- ❌ Health warnings indicate many events are still being ignored

**Codex Parser** (`src/json_parser/codex.rs`):
- ✅ Has delta accumulator but limited usage
- ✅ Handles `agent_message` and `reasoning` item types with deltas
- ❌ Incomplete delta handling for many event types
- ❌ No systematic approach to delta vs. complete content differentiation

**OpenCode Parser** (`src/json_parser/opencode.rs`):
- ✅ Has delta accumulator
- ✅ Handles `text` events with delta accumulation
- ❌ Missing delta handling for tool input streaming and other event types

**Health Monitoring** (`src/json_parser/health.rs`):
- ✅ Good distinction between "unknown events" (valid JSON but unhandled) and "parse errors" (malformed JSON)
- ✅ Only parse errors trigger health warnings (not unknown events)
- ⚠️ The 97.5% ignored events warning suggests many valid events aren't being handled

### Key Issues Identified

1. **Inconsistent Delta Handling**: Each parser handles deltas differently, with some parsers missing key delta event types
2. **Verbosity-Dependent Display**: Deltas are only shown in verbose mode, making partial content invisible to normal users
3. **No Clear Differentiation**: No visual or categorical distinction between partial (delta) and complete content
4. **Missing Delta Types**: Tool input streaming, partial JSON, and other delta types are not consistently handled
5. **Parser Brittleness**: Parsers may fail on unexpected JSON formats instead of gracefully degrading

## Implementation Steps

### Step 1: Enhance Delta Accumulation System

**File**: `src/json_parser/types.rs`

**Changes**:
1. Add `is_delta` flag to `ContentType` to track whether content is partial or complete
2. Add metadata tracking to `DeltaAccumulator` to track:
   - Whether content is being accumulated (partial) or complete
   - Event source (delta event vs. complete event)
   - Timestamp of last update for real-time display
3. Add methods to differentiate partial vs. complete content:
   ```rust
   impl DeltaAccumulator {
       pub fn mark_as_delta(&mut self, content_type: ContentType, key: &str);
       pub fn mark_as_complete(&mut self, content_type: ContentType, key: &str);
       pub fn is_delta(&self, content_type: ContentType, key: &str) -> bool;
       pub fn get_display_content(&self, content_type: ContentType, key: &str) -> Option<&str>;
   }
   ```

### Step 2: Create Unified Delta Display System

**File**: `src/json_parser/delta_display.rs` (new file)

**Purpose**: Centralized logic for displaying partial vs. complete content consistently across all parsers

**Components**:
1. `DeltaDisplayFormatter` - Formats delta content for user display
2. `PartialContentRenderer` - Renders partial content with visual indicators
3. `ContentStateTracker` - Tracks whether to show delta or complete content

**Features**:
- Visual distinction between partial and complete content (e.g., dimmed text, special prefix)
- Real-time streaming display of deltas
- Automatic transition from delta to complete content when available
- Configurable display modes (minimal, normal, verbose)

### Step 3: Enhance Claude Parser Delta Handling

**File**: `src/json_parser/claude.rs`

**Changes**:
1. Process ALL `ContentBlockDelta` types (currently only `TextDelta` and `ThinkingDelta`)
   - Add `ToolUseDelta` handling for partial tool input streaming
   - Add `InputJsonDelta` handling for partial JSON streaming
2. Show deltas in normal mode (not just verbose)
   - Use partial content renderer for deltas
   - Transition to complete content when `content_block_stop` received
3. Track delta state per content block index
4. Handle unknown stream events gracefully (log in debug, don't fail)

### Step 4: Enhance Codex Parser Delta Handling

**File**: `src/json_parser/codex.rs`

**Changes**:
1. Enable delta handling for ALL item types that support it:
   - `agent_message` - text deltas
   - `reasoning` - thinking deltas
   - `command_execution` - partial command display
   - `mcp_tool_call` - partial tool input streaming
   - `plan_update` - incremental plan updates
2. Add completion tracking for delta-based content
3. Differentiate between `item.started` (delta phase) and `item.completed` (complete phase)
4. Use unified delta display system

### Step 5: Enhance OpenCode Parser Delta Handling

**File**: `src/json_parser/opencode.rs`

**Changes**:
1. Process delta events for all streaming content types:
   - `text` events - already handled, enhance with unified display
   - `tool_use` events - add partial input streaming
   - Add handling for partial JSON in tool input
2. Track step lifecycle (`step_start` → deltas → `step_finish`)
3. Show partial tool input in real-time during execution

### Step 6: Implement Algorithmic Delta Detection

**File**: `src/json_parser/stream_classifier.rs` (enhance existing file)

**Purpose**: Automatically detect delta vs. complete content without prior format knowledge

**Algorithm**:
1. **Event Pattern Analysis**:
   - Detect event naming patterns: `delta`, `partial`, `progress`, `chunk`
   - Detect event lifecycle patterns: `start` → `update`/`delta` → `stop`/`done`/`complete`
2. **Content Analysis**:
   - Check for incremental content (small chunks being appended)
   - Detect partial JSON (incomplete structures, missing closing braces)
   - Track content accumulation over time
3. **Statistical Detection**:
   - Monitor event frequency (deltas come in rapid succession)
   - Track content growth patterns (deltas gradually build up content)

**Implementation**:
```rust
pub enum StreamEventType {
    Complete,
    PartialDelta,
    Unknown,
}

pub struct StreamEventClassifier {
    event_history: VecDeque<(String, StreamEventType)>,
    content_buffers: HashMap<String, String>,
}

impl StreamEventClassifier {
    pub fn classify_event(&mut self, event_type: &str, content: Option<&str>) -> StreamEventType;
    pub fn is_accumulating_content(&self, key: &str) -> bool;
}
```

### Step 7: Add Fault-Tolerant Fallback Parsing

**File**: `src/json_parser/fallback.rs` (new file)

**Purpose**: Gracefully handle unexpected JSON formats without losing information

**Strategy**:
1. **Generic JSON Parser**:
   - Parse any valid JSON as a generic `serde_json::Value`
   - Extract common fields (`type`, `event`, `delta`, `content`, `text`)
   - Display in structured format in verbose/debug modes
2. **Unknown Event Handler**:
   - Never ignore unknown events in verbose mode
   - Log event structure for debugging
   - Track unknown event patterns for future parser improvements
3. **Error Recovery**:
   - Continue parsing after malformed JSON (log error, skip line)
   - Use accumulated state to recover partial content
   - Provide fallback display for corrupted streams

### Step 8: Update Health Monitoring

**File**: `src/json_parser/health.rs`

**Changes**:
1. Separate delta events from ignored events in statistics
   - `delta_events` counter
   - `complete_events` counter
   - `unknown_events` counter (already exists)
2. Update warning logic to only warn on parse errors, not unknown/delta events
3. Add delta-specific statistics to warnings:
   ```
   "Parser processed 1500 delta events (streaming content) and 53 complete events.
    Encountered 50 unknown event types (valid JSON but unhandled format)."
   ```

### Step 9: Add Configuration Options

**File**: `src/config/types.rs` and `src/agents/config.rs`

**New Configuration**:
```toml
[json_streaming]
# How to display partial/delta content
delta_display = "minimal"  # options: "minimal", "normal", "verbose"

# Show partial content in real-time (vs. waiting for complete content)
show_delta_in_realtime = true

# Distinguish partial content visually (e.g., dimmed text, special prefix)
mark_partial_content = true

# In verbose mode, show all unknown events
show_unknown_events = true
```

### Step 10: Comprehensive Testing

**Files**: `src/json_parser/tests.rs` (update existing)

**Test Coverage**:
1. **Delta Accumulation Tests**:
   - Text delta accumulation and display
   - Thinking delta accumulation
   - Tool input delta streaming
   - Multiple simultaneous content blocks
2. **Fault Tolerance Tests**:
   - Malformed JSON handling
   - Unknown event type handling
   - Mixed delta and complete content
3. **Algorithmic Detection Tests**:
   - Pattern-based delta detection
   - Lifecycle-based detection
   - Statistical pattern detection
4. **Integration Tests**:
   - Full Claude streaming scenarios
   - Full Codex streaming scenarios
   - Full OpenCode streaming scenarios
   - Mixed agent scenarios
5. **Health Monitoring Tests**:
   - Verify no false warnings on high delta counts
   - Verify parse errors still trigger warnings
   - Verify unknown events don't trigger warnings

## Critical Files

### Files to Modify:
1. **`src/json_parser/types.rs`** - Enhance `DeltaAccumulator` and `ContentType`
2. **`src/json_parser/claude.rs`** - Improve delta handling for all event types
3. **`src/json_parser/codex.rs`** - Add comprehensive delta support
4. **`src/json_parser/opencode.rs`** - Enhance delta handling
5. **`src/json_parser/health.rs`** - Update health monitoring logic
6. **`src/json_parser/stream_classifier.rs`** - Implement algorithmic detection
7. **`src/config/types.rs`** - Add streaming configuration options
8. **`src/agents/config.rs`** - Add streaming config to agent config

### Files to Create:
1. **`src/json_parser/delta_display.rs`** - Unified delta display system
2. **`src/json_parser/fallback.rs`** - Fault-tolerant fallback parser

### Files to Update Tests:
1. **`src/json_parser/tests.rs`** - Add comprehensive test coverage

## Risks & Mitigations

### Risk 1: Performance Impact from Enhanced Delta Handling
**Mitigation**:
- Use efficient string accumulation (avoid excessive cloning)
- Implement display caching for repeated content
- Profile before/after to measure impact

### Risk 2: Breaking Changes to Existing Agent Behavior
**Mitigation**:
- Make new behavior opt-in via configuration
- Maintain backward compatibility with existing display modes
- Add feature flags for gradual rollout

### Risk 3: Increased Complexity Leading to Bugs
**Mitigation**:
- Comprehensive test coverage (unit + integration)
- Incremental implementation with testing at each step
- Code reviews focusing on error handling paths

### Risk 4: Algorithmic Detection False Positives
**Mitigation**:
- Use multiple detection methods in combination
- Allow manual override via configuration
- Log detection decisions for debugging
- Start conservative, expand based on real-world data

### Risk 5: Memory Usage from Content Accumulation
**Mitigation**:
- Implement buffer size limits
- Clear accumulators when content is complete
- Use bounded queues for event history
- Monitor memory usage in testing

## Verification Strategy

### Acceptance Criteria Validation:

1. **"All expected and unexpected json should be handled"**:
   - ✅ Test with valid but unknown event types - should not fail
   - ✅ Test with malformed JSON - should log and continue
   - ✅ Test with mixed delta and complete events - should handle both
   - ✅ Verify no "Parser Health Warning" for high delta percentages

2. **"Differentiate between normal streaming data and delta partial data"**:
   - ✅ Visual distinction in output (different formatting/prefixes)
   - ✅ Separate tracking in statistics
   - ✅ Algorithmic detection works without prior format knowledge
   - ✅ Real-time streaming display of partial content

3. **"No parser health warnings for high delta percentages"**:
   - ✅ Run with Claude agent - verify no warnings about 97% ignored events
   - ✅ Run with Codex agent - verify delta events are tracked separately
   - ✅ Run with OpenCode agent - verify all delta types handled

### Testing Checklist:

- [ ] Unit tests for `DeltaAccumulator` enhancements
- [ ] Unit tests for `DeltaDisplayFormatter`
- [ ] Unit tests for `StreamEventClassifier` detection
- [ ] Integration tests for Claude parser with all delta types
- [ ] Integration tests for Codex parser with all delta types
- [ ] Integration tests for OpenCode parser with all delta types
- [ ] Integration tests for fault tolerance (malformed JSON, unknown events)
- [ ] Performance benchmarks (memory, speed)
- [ ] Real-world testing with actual agent outputs
- [ ] Regression tests to ensure existing functionality works

### Manual Testing Scenarios:

1. **Claude Agent**:
   - Run with text generation only
   - Run with tool use (verify partial tool input displayed)
   - Run with extended thinking (verify thinking deltas displayed)
   - Run with web search tool (verify search streaming displayed)

2. **Codex Agent**:
   - Run with agent message streaming
   - Run with reasoning/thinking content
   - Run with tool execution (command, MCP tools)
   - Run with file operations

3. **OpenCode Agent**:
   - Run with text streaming
   - Run with tool use streaming
   - Run with step-by-step execution

4. **Edge Cases**:
   - Mixed delta and complete events
   - Malformed JSON in middle of stream
   - Unknown event types in verbose mode
   - Very long delta sequences (1000+ events)
   - Simultaneous multiple content blocks

## Sources

- [Claude Streaming Messages Documentation](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [OpenAI Responses API Streaming Events Guide](https://community.openai.com/t/responses-api-streaming-the-simple-guide-to-events/1363122)
- [Claude Code Best Practices](https://www.anthropic.com/engineering/claude-code-best-practices)
- [OpenAI Codex CLI Issues](https://github.com/openai/codex/issues/2556)
- [Response Streaming Architecture](https://zread.ai/openai/codex/25-response-streaming-architecture)

## Notes

- Implementation should follow the code quality specifications from PROMPT.md
- Use type system to make invalid states unrepresentable
- Early returns, minimal nesting, explicit error handling
- Single responsibility per function/class
- Document non-obvious design decisions
- Consider API ergonomics for future extensibility
