# [Feature Name]

> **How to use this template:** This Rust-specific template is implementing new features with Rust best practices. The sections below help you think through the design and provide clear acceptance criteria for the AI agent.

## Goal
[Clear description of what you want to build]

**Tips for a good goal:**
- "Add user authentication with email/password using Actix Web"
- "Implement async file processing with Tokio and error handling"
- "Create a generic trait for data serialization with multiple implementations"

## Questions to Consider
Before implementing, think through:

**Rust-Specific Design:**
- What ownership patterns are appropriate? (borrowed, owned, cloned, Arc/Rc)
- Are there lifetime considerations? How long do references need to live?
- Should this use `async`/`await`? What runtime (Tokio, async-std, smol)?
- What error handling approach? (Result<E>, Option, custom error types, anyhow/thiserror)
- Are there generic/type parameter considerations? (bounds, where clauses)

**Edge Cases:**
- What happens with invalid input? (use Result types, not panics)
- What about empty states or zero results?
- Are there concurrency or threading implications? (Send, Sync bounds)
- How do you handle `None`/missing values? (Option<T>, Default trait)

**Impact:**
- Are there performance implications? (allocation patterns, hot paths)
- What about unsafe code? (can you avoid it? document why if needed)
- Are there memory considerations? (stack vs heap, Box, Arc, Cow)
- Are external dependencies involved? (Cargo.toml updates needed)

**Security & Error Handling:**
- Are there potential security vulnerabilities? (buffer overflow, integer overflow, injection)
- How should errors be handled and communicated? (Result types, custom errors)
- What sensitive data is involved? (zeroize on drop, Secret<String>)
- Are there rate limiting or resource exhaustion concerns? (channels, backpressure)

**Compatibility:**
- Will this require breaking changes to existing APIs? (SemVer considerations)
- Are backward compatibility requirements? (feature flags)
- Will this require changes to dependent crates or consumers?

## Acceptance Checks
- [Specific, testable condition 1]
- [Specific, testable condition 2]
- [Specific, testable condition 3]

**Tips for acceptance criteria:**
- Make them specific and measurable
- Focus on behavior, not implementation
- Include error cases and edge cases

## Constraints
- [Any limitations or requirements]
- [Performance requirements, if applicable]
- [Compatibility notes]

## Context
[Relevant background information]
[Why this change is needed]
[Impact on existing code]

## Implementation Notes (Optional)
[Architecture considerations]
[Potential approaches]
[Crates likely affected]

## Rust Best Practices

**Ownership & Borrowing:**
- Prefer borrowing over cloning when possible (use `&T` instead of `T` to avoid copies)
- Use `Cow<[T]>` for conditional ownership (borrowed or owned)
- Use `Arc<T>` for shared ownership across threads
- Use `Rc<T>` for shared ownership within a single thread
- Use `Box<T>` to convert trait objects to sized types or for large types on stack

**Error Handling:**
- Never use `.unwrap()` or `.expect()` in production code paths (only in tests/examples)
- Use `Result<T, E>` for recoverable errors and `Option<T>` for absent values
- Implement `std::error::Error` for custom error types
- Use `thiserror` for derived error types or `anyhow` for application errors
- Provide context with `.context()` from anyhow or custom Display impls

**Generics & Traits:**
- Prefer trait objects (`dyn Trait`) when you need runtime polymorphism
- Prefer generics (`impl Trait` or `<T: Trait>`) for static dispatch and performance
- Use trait bounds sparingly; prefer where clauses for readability
- Consider `impl Trait` in argument position for ergonomics
- Return `impl Trait` for opaque types (single concrete implementation)

**Async Concurrency:**
- Use `async fn` for functions that await (not all functions need to be async)
- Prefer Tokio's task spawning: `tokio::spawn`, `tokio::task::spawn_blocking`
- Use channels for communication: `tokio::sync::mpsc`, `tokio::sync::broadcast`
- Apply backpressure to prevent unbounded memory growth
- Use `tokio::select!` for multi-way branch on async operations

**Unsafe Code:**
- Avoid unsafe code whenever possible (use safe abstractions)
- If unsafe is necessary, document why it's safe (SAFETY comments)
- Keep unsafe blocks small and well-contained
- Consider using `unsafe fn` with documented safety contracts

**Testing:**
- Write unit tests alongside code in the same module
- Use `#[cfg(test)]` for test-only code
- Use `cargo test` for unit tests, `cargo test --doc` for doctests
- Test error paths (unhappy paths), not just success paths
- Use property-based testing with `proptest` for algorithms with invariants

**Code Organization:**
- Keep modules focused (single responsibility)
- Use `pub(crate)` for items private to the crate but public to modules
- Re-export common types at the crate root (`pub use self::foo::Bar;`)
- Organize by feature, not by layer (e.g., `auth`, not `models`)
- Use features to expose optional functionality

**Performance:**
- Profile before optimizing (use `cargo flamegraph`, `perf`, or `cargo-assembler`)
- Prefer iterator chains over intermediate collections (`.map().filter().collect()`)
- Use `#[inline]` sparingly (let the compiler decide in most cases)
- Be aware of monomorphization bloat from generics (consider `dyn Trait` for large generic APIs)
- Use `lazy_static` or `once_cell` for global static data

**Dependencies:**
- Minimize external dependencies (prefer std when possible)
- Keep dependencies up to date (`cargo outdated`)
- Audit dependencies for security vulnerabilities (`cargo audit`)
- Prefer crates with active maintenance and permissive licenses
- Pin dependency versions in Cargo.lock for reproducibility

**Documentation:**
- Document public APIs with rustdoc (`///` or `//!`)
- Include examples in documentation that run as tests (`cargo test --doc`)
- Document panic conditions in doc comments (PANICS section)
- Document errors that can be returned (ERRORS section)
- Document safety requirements for unsafe functions (SAFETY section)

## Security Considerations
- Validate all user input at system boundaries (parse, validate, sanitize)
- Use constant-time comparisons for secrets (secrets, passwords, tokens)
- Be aware of integer overflow/underflow (use `checked_*`, `saturating_*`, or `wrapping_*`)
- Zero out sensitive data with `zeroize` crate before dropping
- Use `secrecy::Secret` wrapper for sensitive values (prevents accidental logging)
- Avoid `unsafe` code unless absolutely necessary and well-documented
- Be careful with deserialization (avoid `bincode`, use `serde` with validation)
- Consider side-channel attacks (timing, cache) in cryptography code
