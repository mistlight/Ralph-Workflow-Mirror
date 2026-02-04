# XSD Retry Placeholder Validation Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ensure XSD retry prompt preparation ignores literal `{{...}}` that appear inside XSD error strings so retries are not blocked by template validation.

**Architecture:** Extend review/fix prompt preparation to treat the XSD error message as trusted content for placeholder validation. Add regression tests in the reducer handler review prompt test suite to cover both review and fix XSD retry paths.

**Tech Stack:** Rust, reducer handler tests, MemoryWorkspace.

---

### Task 1: Add failing tests for XSD error placeholder handling

**Files:**
- Modify: `ralph-workflow/src/reducer/handler/tests/review_prompt/xsd_retry.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_prepare_review_prompt_xsd_retry_ignores_xsd_error_placeholders() {
    // test body added in implementation
}
```

```rust
#[test]
fn test_prepare_fix_prompt_xsd_retry_ignores_xsd_error_placeholders() {
    // test body added in implementation
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow reducer::handler::tests::review_prompt::xsd_retry::test_prepare_review_prompt_xsd_retry_ignores_xsd_error_placeholders`
Expected: FAIL with template variable validation error.

Run: `cargo test -p ralph-workflow reducer::handler::tests::review_prompt::xsd_retry::test_prepare_fix_prompt_xsd_retry_ignores_xsd_error_placeholders`
Expected: FAIL with template variable validation error.

**Step 3: Commit**

```bash
git add ralph-workflow/src/reducer/handler/tests/review_prompt/xsd_retry.rs
git commit -m "test: cover XSD error placeholder validation"
```

### Task 2: Ignore XSD error placeholders in review XSD retry prompts

**Files:**
- Modify: `ralph-workflow/src/reducer/handler/review/review_flow.rs`

**Step 1: Write the failing test**

Use the test from Task 1.

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow reducer::handler::tests::review_prompt::xsd_retry::test_prepare_review_prompt_xsd_retry_ignores_xsd_error_placeholders`
Expected: FAIL before implementation.

**Step 3: Write minimal implementation**

```rust
if is_xsd_retry {
    if let Some(error) = continuation_state.last_review_xsd_error.as_deref() {
        ignore_sources_owned.push(error.to_string());
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow reducer::handler::tests::review_prompt::xsd_retry::test_prepare_review_prompt_xsd_retry_ignores_xsd_error_placeholders`
Expected: PASS

**Step 5: Commit**

```bash
git add ralph-workflow/src/reducer/handler/review/review_flow.rs
git commit -m "fix: ignore XSD error placeholders in review retry"
```

### Task 3: Ignore XSD error placeholders in fix XSD retry prompts

**Files:**
- Modify: `ralph-workflow/src/reducer/handler/review/fix_flow.rs`

**Step 1: Write the failing test**

Use the test from Task 1.

**Step 2: Run test to verify it fails**

Run: `cargo test -p ralph-workflow reducer::handler::tests::review_prompt::xsd_retry::test_prepare_fix_prompt_xsd_retry_ignores_xsd_error_placeholders`
Expected: FAIL before implementation.

**Step 3: Write minimal implementation**

```rust
if is_xsd_retry {
    if let Some(error) = continuation_state.last_fix_xsd_error.as_deref() {
        ignore_sources.push(error);
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ralph-workflow reducer::handler::tests::review_prompt::xsd_retry::test_prepare_fix_prompt_xsd_retry_ignores_xsd_error_placeholders`
Expected: PASS

**Step 5: Commit**

```bash
git add ralph-workflow/src/reducer/handler/review/fix_flow.rs
git commit -m "fix: ignore XSD error placeholders in fix retry"
```
