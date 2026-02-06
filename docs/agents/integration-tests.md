# Integration Tests (CRITICAL)

**Read `tests/INTEGRATION_TESTS.md` before touching integration tests.**

## Principles

- Test **observable behavior**, not implementation details
- Mock only at **architectural boundaries** (filesystem, network, external APIs)
- NEVER use `cfg!(test)` branches or test-only flags in production code
- When tests fail, fix implementation (unless expected behavior changed)

## Common mistakes

- Mocking internal functions (only mock external dependencies)
- Testing private details (test through public APIs)
- Adding `#[cfg(test)]` in production (use dependency injection)
- Updating tests because implementation changed (only if behavior changed)

## Required patterns

- Parser tests: `TestPrinter` from `ralph_workflow::json_parser::printer`
- File operations: `MemoryWorkspace` (NO `TempDir`, NO `std::fs::*`)
- Process execution: `MockProcessExecutor` (NO real process spawning)
