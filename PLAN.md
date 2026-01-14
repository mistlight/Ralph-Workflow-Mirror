# Bug Fix Plan: JSON Commit Message in Markdown Code Fence

## Summary

The commit message extraction fails when the AI agent outputs a JSON commit message wrapped in a markdown code fence. The current implementation has two issues:

1. **`try_extract_structured_commit`**: When processing raw NDJSON log content, the naive `{...}` search finds the NDJSON stream wrapper objects instead of the actual commit JSON (which is inside a string value of the `result` field, with escaped quotes).

2. **`remove_thought_process_patterns`**: Even after the Claude extractor correctly extracts the `result` field content, the thought process filtering cannot locate the commit message because `find_conventional_commit_start` requires the commit type (e.g., "fix:") to be at the START of a line. When the commit is in JSON format (`{"subject": "fix(streaming):...}`), the type appears mid-line after `"subject": "`.

**Result**: Both JSON extraction and pattern-based extraction return empty, leading to "Commit message is empty" error.

## Root Cause

Given the log content from a CCS/Claude agent:
```
{"type":"result","result":"Looking at the diff...\n\n1. **colors.rs**...\n\n```json\n{\"subject\": \"fix(streaming): ...\", \"body\": \"...\"}\n```"}
```

**Flow:**
1. `try_extract_structured_commit` is called on raw NDJSON - finds first `{` (stream wrapper), fails
2. `extract_claude_result` correctly extracts the `result` string value
3. `remove_thought_process_patterns` runs on:
   ```
   Looking at the diff...

   1. **colors.rs**...

   ```json
   {"subject": "fix(streaming): ...", "body": "..."}
   ```
   ```
4. It strips "Looking at the diff" prefix, continues with numbered analysis
5. `find_conventional_commit_start` searches for "fix:" at line start
6. The JSON `{"subject": "fix(streaming)...` has "fix" mid-line → not found
7. Content looks like analysis with no valid commit → returns empty string

## Implementation Steps

### Step 1: Add code fence JSON extraction to `remove_thought_process_patterns`

**File**: `ralph-workflow/src/files/llm_output_extraction.rs`

In `remove_thought_process_patterns` (around line 426), before the existing numbered analysis check, add detection and extraction of JSON from markdown code fences:

```rust
// NEW: Check for JSON commit in markdown code fence
if let Some(json_commit) = extract_commit_from_code_fence(result) {
    return json_commit;
}
```

Add helper function:

```rust
/// Extract a commit message from JSON inside a markdown code fence.
///
/// Handles patterns like:
/// ```json
/// {"subject": "feat: add feature", "body": "description"}
/// ```
fn extract_commit_from_code_fence(content: &str) -> Option<String> {
    let fence_patterns = ["```json\n", "```JSON\n", "```\n"];

    for pattern in fence_patterns {
        if let Some(start_pos) = content.find(pattern) {
            let after_fence = &content[start_pos + pattern.len()..];
            if let Some(end_pos) = after_fence.find("```") {
                let json_content = after_fence[..end_pos].trim();
                // Try to parse as structured commit
                if json_content.starts_with('{') && json_content.ends_with('}') {
                    if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(json_content) {
                        return format_structured_commit(&msg);
                    }
                }
            }
        }
    }
    None
}
```

Note: This requires moving `StructuredCommitMessage` struct and `format_structured_commit` function to be accessible from `remove_thought_process_patterns`, or inlining the logic.

### Step 2: Add code fence handling to `try_extract_structured_commit`

**File**: `ralph-workflow/src/files/llm_output_extraction.rs`

Modify `try_extract_structured_commit` (line 996) to:
1. First try extracting from NDJSON `result` field if detected
2. Then try extracting JSON from markdown code fence
3. Fall back to existing logic

```rust
pub fn try_extract_structured_commit(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // NEW: If content looks like NDJSON stream, extract from result field first
    if looks_like_ndjson(trimmed) {
        for line in trimmed.lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("type").and_then(|v| v.as_str()) == Some("result") {
                    if let Some(result_str) = json.get("result").and_then(|v| v.as_str()) {
                        // Try to extract commit from the result content
                        if let Some(msg) = try_extract_from_text(result_str) {
                            return Some(msg);
                        }
                    }
                }
            }
        }
    }

    // Existing logic (refactored into helper)
    try_extract_from_text(trimmed)
}

fn looks_like_ndjson(content: &str) -> bool {
    content.lines().next().map_or(false, |first_line| {
        let trimmed = first_line.trim();
        trimmed.starts_with('{') && trimmed.contains(r#""type""#)
    })
}

fn try_extract_from_text(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // NEW: Try extracting from markdown code fence
    if let Some(json_content) = extract_json_from_code_fence(trimmed) {
        if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(&json_content) {
            return format_structured_commit(&msg);
        }
    }

    // Existing: Try direct parse
    if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(trimmed) {
        return format_structured_commit(&msg);
    }

    // Existing: Try to find JSON object within content
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if start < end {
                let json_str = &trimmed[start..=end];
                if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(json_str) {
                    return format_structured_commit(&msg);
                }
            }
        }
    }

    None
}

fn extract_json_from_code_fence(content: &str) -> Option<String> {
    let fence_patterns = ["```json\n", "```JSON\n", "```\n"];

    for pattern in fence_patterns {
        if let Some(start_pos) = content.find(pattern) {
            let after_fence = &content[start_pos + pattern.len()..];
            if let Some(end_pos) = after_fence.find("```") {
                let json_content = after_fence[..end_pos].trim();
                if json_content.starts_with('{') && json_content.ends_with('}') {
                    return Some(json_content.to_string());
                }
            }
        }
    }
    None
}
```

### Step 3: Add regression tests

**File**: `ralph-workflow/src/files/llm_output_extraction.rs` (test section)

```rust
#[test]
fn test_regression_json_in_markdown_code_fence() {
    // The exact bug scenario: analysis followed by JSON in code fence
    let content = r#"Looking at the diff, I need to analyze...

1. **colors.rs** - Adds a constant
2. **claude.rs** - Changes handling

```json
{"subject": "fix(streaming): improve message lifecycle tracking", "body": "Add turn_counter for ID generation."}
```
"#;

    // Test via remove_thought_process_patterns
    let filtered = remove_thought_process_patterns(content);
    assert!(filtered.contains("fix(streaming):"), "Should extract commit from code fence");
    assert!(!filtered.contains("Looking at"), "Should remove analysis");
}

#[test]
fn test_structured_commit_from_code_fence() {
    let content = r#"Here's the commit:

```json
{"subject": "feat: add feature", "body": "Description here."}
```
"#;

    let result = try_extract_structured_commit(content);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "feat: add feature\n\nDescription here.");
}

#[test]
fn test_structured_commit_from_ndjson_with_code_fence() {
    // NDJSON stream with JSON commit in code fence inside result field
    let ndjson = r#"{"type":"system","session_id":"abc"}
{"type":"result","result":"Analysis...\n\n```json\n{\"subject\": \"fix: bug\", \"body\": null}\n```"}"#;

    let result = try_extract_structured_commit(ndjson);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "fix: bug");
}
```

### Step 4: Run verification commands (per CLAUDE.md)

```bash
# Check for forbidden attributes (must produce no output)
rg -n --pcre2 '(?x)
  \#\s*!?\[\s*
  (allow|expect)
  \s*\(
    [^()\]]*
    (?:\([^()\]]*\)[^()\]]*)*
  \)
  \s*\]
' --glob '!target/**' --glob '!.git/**' --glob '*.rs' .

# Format, lint, test
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Critical Files for Implementation

1. **`ralph-workflow/src/files/llm_output_extraction.rs`**
   - Add `extract_json_from_code_fence` helper function
   - Add `extract_commit_from_code_fence` helper for thought process filtering
   - Modify `try_extract_structured_commit` to handle NDJSON and code fences
   - Modify `remove_thought_process_patterns` to check for JSON in code fences
   - Add regression tests

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| **Breaking existing extraction** | New code paths are checked BEFORE existing logic and only activate for specific patterns. Existing tests verify no regression. |
| **False positive on code fence detection** | Only extract if content inside fence starts with `{` and ends with `}`, and parses as valid JSON with correct schema. |
| **Performance** | String search for ` ``` ` is O(n), runs only once. NDJSON detection is O(1) - just checks first line. |
| **Multiple code fences** | Take first valid JSON code fence. If parsing fails, fall through to existing logic. |

## Verification Strategy

1. **Unit Tests**: All existing tests pass, new regression tests pass
2. **Integration**: The exact bug scenario from logs produces correct commit message
3. **CLAUDE.md Compliance**:
   - No `#[allow(...)]` or `#[expect(...)]` attributes added
   - `cargo fmt --all` produces no changes
   - `cargo clippy --all-targets --all-features -- -D warnings` passes
   - `cargo test --all-features` passes

## Acceptance Criteria

- [ ] JSON commit in NDJSON result field with code fence is correctly extracted
- [ ] JSON commit in plain text with code fence is correctly extracted
- [ ] `remove_thought_process_patterns` handles code fence JSON
- [ ] All existing tests pass (no regressions)
- [ ] New regression tests for exact bug scenario pass
- [ ] CLAUDE.md verification commands succeed
