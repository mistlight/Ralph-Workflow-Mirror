# Workspace Trait (CRITICAL)

ALL filesystem operations MUST use the `Workspace` trait. Direct `std::fs::*` is FORBIDDEN.

If you need deeper architecture context (CLI `AppEffect` vs pipeline `Effect`, and where `std::fs` is allowed), see `docs/architecture/effect-system.md`.

| FORBIDDEN | REQUIRED |
|-----------|----------|
| `std::fs::read_to_string(path)` | `workspace.read(path)` |
| `std::fs::write(path, content)` | `workspace.write(path, content)` |
| `std::fs::create_dir_all(path)` | `workspace.create_dir_all(path)` |
| `std::fs::read_dir(path)` | `workspace.read_dir(path)` |
| `path.exists()` | `workspace.exists(path)` |
| `std::fs::remove_file(path)` | `workspace.remove(path)` |

## Implementation

```rust
// Production code - accepts workspace via dependency injection
fn my_function(workspace: &dyn Workspace) -> Result<()> {
    let content = workspace.read(Path::new(".agent/config.toml"))?;
    workspace.write(Path::new(".agent/output.txt"), "result")?;
    Ok(())
}

// Tests - use MemoryWorkspace
#[test]
fn test_my_function() {
    let workspace = MemoryWorkspace::new_test()
        .with_file(".agent/config.toml", "key = value");

    my_function(&workspace).unwrap();

    assert!(workspace.was_written(".agent/output.txt"));
}
```

## Exceptions

The ONLY acceptable uses of `std::fs` are:

1. Inside `WorkspaceFs` implementation itself (the production `Workspace` impl)
2. Bootstrap code that discovers the repo root before `Workspace` is created

## Documented Exceptions

The following specific uses of `std::fs` are acceptable and do not need refactoring:

| Location | Reason |
|----------|--------|
| `workspace.rs` (`WorkspaceFs`) | This IS the production filesystem implementation |
| `app/effect_handler.rs` (`RealAppEffectHandler`) | This IS the production effect handler |
| `config/path_resolver.rs` (`RealConfigEnvironment`) | Production config environment implementation |
| `ralph-workflow/src/agents/opencode_api/cache.rs` (`RealCacheEnvironment`) | Production cache implementation |
| `git_helpers/rebase.rs` | Operating on `.git/` directory internals |
| `git_helpers/hooks.rs` | Bootstrap operation on `.git/hooks/` (see module docs) |
| `files/protection/monitoring.rs` | Atomic file open for TOCTOU security |
| `files/io/agent_files.rs` (CWD functions) | CLI plumbing commands before workspace available |
| `checkpoint/file_state.rs` (CWD-relative impl functions) | CLI-layer code before workspace available |
| `logger/output.rs` (legacy mode logging) | Legacy mode logging for CLI layer code before workspace available |
| `config/unified.rs` (`load_from_path`) | Convenience method for config loading when ConfigEnvironment not available |

All other production code MUST use the Workspace trait.

**When you see `std::fs` in production code outside these exceptions, it MUST be refactored to use `Workspace`.**
