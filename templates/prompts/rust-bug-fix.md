# [Bug Description]

> **How to use this template:** This Rust-specific template helps debug and fix bugs systematically. Focus on understanding the root cause before making changes.

## Goal
[Clear description of the bug to fix]

**Tips for a good goal:**
- "Fix panic in `parse_config` when config file is empty"
- "Fix data race in `RequestHandler::process` that causes occasional crashes"
- "Fix memory leak in `ConnectionPool` that grows unbounded over time"

## Bug Report & Investigation

**Observed Behavior:**
[What is actually happening? Include error messages, stack traces, panics]

**Expected Behavior:**
[What should happen instead?]

**Reproduction Steps:**
1. [Step 1]
2. [Step 2]
3. [Step 3]

**Environment:**
- Rust version: `rustc --version`
- Cargo version: `cargo --version`
- Operating system:
- Relevant dependencies:

**Error Messages / Stack Traces:**
```
[Paste any panic messages, error output, or stack traces here]
```

## Questions to Consider

**Root Cause Analysis:**
- When does this bug occur? (always, sometimes, under specific conditions)
- What changed that may have introduced this bug? (recent commits, dependency updates)
- Is this a logic error, panic, segmentation fault, data race, deadlock, or memory leak?
- Can you reproduce the bug reliably in a minimal test case?

**Rust-Specific Bug Patterns:**
- **Panic:** Is code calling `.unwrap()`, `.expect()`, `panic!`, or indexing?
- **Borrow Checker Issue:** Are lifetimes too short? Are there conflicting borrows?
- **Thread Safety:** Is data shared between threads without proper synchronization? (Send, Sync)
- **Deadlock:** Are multiple locks acquired in inconsistent order?
- **Memory Leak:** Are reference cycles created? (Rc<RefCell<T>> cycles)
- **Integer Overflow:** Does arithmetic overflow in debug mode? (use `checked_*` or `wrapping_*`)
- **Data Race:** Are there unsynchronized mutable accesses across threads?

**Debugging Strategy:**
- Can you add `dbg!()` or `println!()` statements to trace execution?
- Can you write a unit test that reproduces the bug?
- Can you use `cargo test` to run tests under `miri` for undefined behavior?
- Can you use `gdb` or `lldb` to debug segfaults?
- Can you use `RUST_BACKTRACE=1` to get a full stack trace from panics?
- Can you use thread sanitizer (`tsan`) or address sanitizer (`asan`)?
- Can you use `cargo clippy` to catch common bugs?

## Acceptance Checks
- [ ] Bug no longer occurs (verify fix works)
- [ ] Unit tests added that prevent regression
- [ ] No new panics or errors introduced
- [ ] Edge cases related to the bug are tested

## Solution Approach

**Proposed Fix:**
[Describe the fix in detail]

**Why This Fix Works:**
[Explain why this fix addresses the root cause]

**Alternative Approaches Considered:**
- [Alternative 1] - [Why it wasn't chosen]
- [Alternative 2] - [Why it wasn't chosen]

## Testing Strategy

**Unit Tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_for_bug() {
        // Test that the bug is fixed
    }

    #[test]
    fn test_edge_cases() {
        // Test related edge cases
    }
}
```

**Integration Tests:**
[How will you test this in the full system?]

## Rust Bug-Fix Best Practices

**Handling Panics:**
- Replace `.unwrap()` and `.expect()` with proper error handling (`?` operator, `match`)
- Use `Result<T, E>` for recoverable errors
- Use `Option<T>` for values that may be absent
- Only panic in truly unrecoverable situations (assertions for invariants)
- Use `catch_unwind` for testing code that should panic

**Borrow Checker Issues:**
- Shorten lifetimes where possible (reduce scope of borrows)
- Clone values when ownership is unclear (consider performance impact)
- Use `Cow<[T]>` for conditional ownership
- Restructure code to avoid conflicting borrows (split borrows)
- Use interior mutability patterns (`RefCell<T>`, `Cell<T>`, `Mutex<T>`, `RwLock<T>`)

**Thread Safety & Concurrency:**
- Ensure `T: Send + Sync` when sharing across threads
- Use `Arc<T>` for shared ownership across threads
- Use `Mutex<T>` or `RwLock<T>` for mutable shared state
- Use channels for message passing (`tokio::sync::mpsc`, `std::sync::mpsc`)
- Acquire locks in a consistent order to avoid deadlocks
- Use `AtomicUsize`, `AtomicBool`, etc. for lock-free synchronization

**Memory Leaks:**
- Avoid `Rc<RefCell<T>>` cycles that cause leaks
- Use `Weak<T>` to break cycles in reference-counted types
- Be careful with `std::mem::forget` (ManuallyDrop is safer)
- Profile memory usage with `valgrind`, `heaptrack`, or `dhat`

**Integer Arithmetic:**
- Use `checked_*`, `saturating_*`, or `wrapping_*` for explicit overflow behavior
- Enable `overflow_checks` in debug mode (default: enabled)
- Consider using `BigInt` or `BigUint` for arbitrary precision

**Unsafe Code:**
- Document unsafe blocks with SAFETY comments
- Keep unsafe blocks small and isolated
- Verify assumptions with tests or assertions
- Prefer safe alternatives (abstractions) when available

## Code Quality Specifications

Write clean, maintainable code:
- Single responsibility: one reason to change per function
- Small units: functions < 30 lines, modules focused
- Clear names that reveal intent
- Early returns; minimize nesting depth
- Explicit error handling with `Result` and `Option`; no silent failures
- No magic numbers; extract constants
- DRY: extract duplicated logic into helper functions
- Test behavior, not implementation

**Bug Fix Best Practices:**
- Write tests that reproduce the bug before fixing it (test-driven bug fixing)
- Fix the root cause, not the symptom
- Add regression tests to prevent the bug from returning
- Keep changes minimal; don't refactor unrelated code
- Document why the bug occurred and why the fix works
- Consider edge cases and similar issues in other parts of the codebase
