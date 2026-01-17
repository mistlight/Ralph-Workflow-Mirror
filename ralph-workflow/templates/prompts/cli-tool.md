# CLI Tool: [Brief title]

> **How to use this template:** This template is for command-line interface development. Fill in the goal and acceptance criteria below to guide the AI agent.

## Goal
[One-line description of the CLI tool or feature]

## Questions to Consider

**User Experience:**
* What is the command name and subcommand structure?
* What arguments and flags does the user need to provide?
* Should it support shell completion?
* How should errors be displayed to users?

**Functionality:**
* What is the primary input and output?
* Are there any configuration files or environment variables?
* Should it support stdin/stdout or only file operations?
* Are there any long-running operations that need progress indication?

**Integration:**
* Does it need to interact with other tools or APIs?
* Should it support different output formats (JSON, plain text, etc.)?
* Are there any platform-specific considerations?

## Acceptance Checks
* [Command parses arguments correctly with help text]
* [Shell completion works for bash/zsh/fish]
* [Error messages are clear and actionable]
* [Exit codes follow standard conventions (0=success, non-zero=error)]
* [Output format is consistent and parseable]
* [Man page or comprehensive help available]

## Code Quality Specifications

Write clean, maintainable code:
- Single responsibility: one reason to change per function/class
- Small units: functions < 30 lines, classes < 300 lines
- Clear names that reveal intent
- Early returns; minimize nesting depth
- Explicit error handling; no silent failures
- No magic numbers; extract constants
- DRY: extract duplicated logic
- Validate at boundaries; trust internal data
- Test behavior, not implementation

**Feature Implementation Best Practices:**
- Start with the simplest working solution, optimize only if needed
- Prefer standard library solutions over external dependencies
- Add logging at key points (entry/exit of major functions, errors)
- Use types to make invalid states unrepresentable
- Document non-obvious design decisions in comments
- Consider the API ergonomics - is it pleasant to use?

**Security Considerations:**
- Validate all user input at system boundaries
- Sanitize data before display (prevent injection attacks)
- Use parameterized queries to prevent command injection
- Follow the principle of least privilege for permissions
- Never log sensitive data (passwords, tokens, PII)
- Consider rate limiting for public-facing tools

**EXAMPLE:**
```markdown
# CLI Tool: Image Converter

## Goal
Create a CLI tool that converts images between formats with optional resizing.

## Questions to Consider
**User Experience:**
- Command: `imgconvert <input> <output> [--size WxH]`
- Flags: `--format`, `--quality`, `--size`
- Support tab-completion for file formats

**Acceptance Checks:**
- [Converts PNG to JPEG correctly]
- [Resizing maintains aspect ratio]
- [Shows progress for large files]
```
