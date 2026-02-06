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

## Testing Prompt Path Resolution

When testing prompt generation, verify workspace-rooted paths:

```rust
use ralph_workflow::workspace::{Workspace, WorkspaceMemory};
use std::path::PathBuf;

let workspace_root = PathBuf::from("/tmp/test_workspace");
let workspace = WorkspaceMemory::new_at_path(workspace_root);

// Generate prompt
let prompt = prompt_planning_xml_with_references(&template_context, &prompt_ref, &workspace);

// Verify workspace-rooted paths
let expected_path = workspace.absolute_str(".agent/tmp/plan.xml");
assert!(prompt.contains(&expected_path));
```

**Do NOT** test implementation details like whether `resolve_absolute_path` vs `workspace.absolute_str()` is called. Test the observable behavior: prompts contain correct absolute paths.

## Testing Loop Recovery

Test loop detection through reducer behavior:

```rust
let mut state = PipelineState::initial(1, 0);
state.continuation.consecutive_same_effect_count = 5;
state.continuation.xsd_retry_pending = true;

// Behavioral test: system should not loop indefinitely
assert!(state.continuation.consecutive_same_effect_count <= 10);

// Test recovery event
let event = PipelineEvent::Continuation(ContinuationEvent::LoopRecoveryTriggered { ... });
let new_state = reduce(state, event);
assert!(!new_state.continuation.xsd_retry_pending);
```

**Do NOT** test internal loop detection algorithm. Test that:
1. Loop counters are bounded
2. Recovery events reset retry state
3. Pipeline advances after recovery
