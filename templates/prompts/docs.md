# Documentation: [What needs documentation]

> **How to use this template:** This template is for writing or improving documentation. Good documentation helps users and contributors understand and use your code effectively.

## Goal
[What documentation needs to be written/updated]
[Why this documentation is needed]

## Questions to Consider
Before documenting:

**Audience:**
- Who is the audience? (developers, users, contributors)
- What questions will they have? What problems will they try to solve?
- What is their skill level? (beginner, intermediate, advanced)

**Content:**
- Are there examples for common use cases?
- Is the terminology clear and consistent?
- Are edge cases and errors documented?
- Is the information current and accurate?

## Acceptance
- [Documentation is clear and audience-appropriate]
- [Examples work and can be copied/pasted]
- [No broken links or missing references]
- [API changes are documented]
- [Quick start guide is included if applicable]

## Documentation Tips

**For API documentation:**
- Include examples for each function/method
- Document parameters, return values, and errors
- Keep descriptions concise but complete

**For user guides:**
- Start with a quick start or getting started section
- Use progressive disclosure (simple → complex)
- Include screenshots or diagrams where helpful

**For contribution guides:**
- Explain how to set up the development environment
- Document the code structure and architecture
- Provide guidelines for submitting changes

## Code Quality Specifications

**Documentation Completeness Checklist:**
- All public functions/types have doc comments
- Parameters, return values, and errors are documented
- Examples are provided for non-trivial APIs
- `# Panics`, `# Errors`, `# Safety` sections where applicable
- Configuration options and environment variables are documented

**For Rust Documentation:**
- Use `///` for item docs, `//!` for module-level docs
- Include code examples that `cargo test --doc` can verify
- Use `#[doc]` attributes for advanced documentation control
- Cross-reference related items with intra-doc links (`[`TypeName`]`)
- Document safety invariants for `unsafe` code

**Testing Documentation:**
- Document test coverage expectations
- Explain how to run the test suite
- Include examples of testing patterns used in the project
- Document any test utilities or helpers available
