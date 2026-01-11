# Contributing to Ralph

Thank you for your interest in contributing to Ralph! This document provides guidelines for contributing to this project, whether you're a human developer or an AI agent.

## Getting Started

1. **Fork the repository** on CodeBerg
2. **Clone your fork** locally
3. **Set up the development environment**:
   ```bash
   cargo build
   cargo test
   ```

## Development Guidelines

### Code Style

Ralph follows idiomatic Rust practices. Before submitting a pull request:

- **Format code**: Run `cargo fmt` before committing
- **Lint code**: Run `cargo clippy -- -D warnings` (treat warnings as errors)
- **Test code**: Run `cargo test` and ensure all tests pass

### Rust Conventions

- **Edition**: Rust 2021 (stable)
- **Error handling**: Use `Result` + `?` with meaningful error context. Avoid `unwrap()` and `expect()` in production paths.
- **Safety**: Default to `#![deny(unsafe_code)]`. If unsafe is required, justify and minimize scope.
- **Memory**: Prefer borrowing, slices, and iterators. Avoid unnecessary clones and allocations.
- **Types**: Use strong types and exhaustive matching. Document invariants and error cases.
- **Visibility**: Keep public API minimal (`pub(crate)` by default). Document public items.

### Testing

- Add unit tests for new functionality (cover happy path, failure cases, and edge cases)
- Add property-based tests when invariants matter
- Integration tests live in the `tests/` directory

## Pull Request Process

### Before Submitting

1. Ensure your code compiles: `cargo check`
2. Format your code: `cargo fmt`
3. Pass linting: `cargo clippy -- -D warnings`
4. Pass tests: `cargo test`
5. Update documentation if you've changed public APIs or behavior

### PR Guidelines

- **Keep PRs focused**: One feature or fix per PR
- **Write clear commit messages**: Explain the "why", not just the "what"
- **Update the README** if you've added new features or changed behavior
- **Add tests** for new functionality

### PR Title Format

Use conventional commit style:
- `feat: add new feature`
- `fix: resolve bug in X`
- `refactor: improve Y structure`
- `docs: update contributing guide`
- `test: add tests for Z`
- `chore: update dependencies`

## For AI Agents

If you're an AI agent contributing to this project:

### Understanding the Codebase

- Read `PROMPT.md` for current goals and acceptance criteria
- Check `.agent/STATUS.md` for current progress and blockers
- Review `.agent/NOTES.md` for context from previous work
- Review `.agent/ISSUES.md` for known issues to address

### Working with Ralph

Ralph itself uses a multi-agent workflow:
1. A developer agent makes progress toward `PROMPT.md` goals
2. A reviewer agent reviews and applies fixes
3. Checks run to validate changes

### Agent Best Practices

- **Read before writing**: Always read a file before modifying it
- **Incremental changes**: Make small, focused changes
- **Update status**: Keep `.agent/STATUS.md` current
- **Document notes**: Add context to `.agent/NOTES.md`
- **Follow the style**: Match existing code patterns

## Project Structure

```
ralph/
├── src/
│   ├── main.rs         # Entry point and CLI
│   ├── config.rs       # Configuration and CLI args
│   ├── agents.rs       # Agent definitions and execution
│   ├── prompts.rs      # Prompt generation for agents
│   ├── json_parser.rs  # Agent output parsing
│   ├── git_helpers.rs  # Git operations
│   ├── colors.rs       # Terminal colors
│   ├── timer.rs        # Timing utilities
│   └── utils.rs        # Shared utilities
├── tests/              # Integration tests
├── examples/           # Example configurations
└── .agent/             # Agent working directory
```

## Reporting Issues

When reporting issues:

1. **Search existing issues** first to avoid duplicates
2. **Provide context**: What were you trying to do?
3. **Include reproduction steps**: How can we reproduce the issue?
4. **Share error messages**: Include the full error output
5. **Environment info**: OS, Rust version, agent versions

## License

By contributing to Ralph, you agree that your contributions will be licensed under the project's AGPL-3.0 license.

## Questions?

If you have questions about contributing, feel free to open an issue for discussion.
