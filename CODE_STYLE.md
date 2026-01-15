# Code Style Guide

This document defines the design principles, coding standards, and testing philosophy for this project.

---

## Design Principles

### High cohesion, low coupling

Group code by *reason to change*. Code that changes together should live together.

### Single Responsibility Principle (SRP)

Each module/type should have one primary responsibility and one primary reason to change.

### Clear boundaries

Isolate:
- Domain logic
- Application orchestration
- Infrastructure (I/O, persistence, networking)
- Framework/UI concerns

### Explicit, safe APIs

Public APIs should be hard to misuse and easy to understand. Prefer types that encode invariants and prevent invalid states.

### Minimize surface area

Keep modules private by default; only expose what is truly part of the public contract.

---

## Clean Code Guidelines

### Size and structure

- Functions should be small: aim for < 30 lines
- Modules/classes should be focused: aim for < 300 lines
- If a unit is too large, it likely has multiple responsibilities

### Naming

- Names should reveal intent
- Avoid abbreviations unless universally understood
- Functions should describe what they do, not how

### Control flow

- Use early returns to reduce nesting depth
- Avoid deep nesting (> 3 levels is a smell)
- Prefer guard clauses over nested conditionals

### Error handling

- Explicit error handling; no silent failures
- Use `Result` + `?` with meaningful error context
- Avoid `unwrap()` and `expect()` in production paths

### Constants and magic values

- No magic numbers; extract named constants
- Group related constants together

### Duplication

- DRY: extract duplicated logic into shared functions
- But avoid premature abstraction — duplication is better than the wrong abstraction

### Validation

- Validate at system boundaries (user input, external APIs)
- Trust internal data; don't re-validate everywhere

---

## Dead Code

Dead code is a form of design debt and must be aggressively removed.

### Definition

Code is considered **dead** if any of the following are true:

- It is not referenced by production code
- It is only referenced by tests
- It is not part of a documented, externally observable public API
- It does not contribute to observable runtime behavior
- It exists "for future use" without an active requirement

> **Critical rule:**
> If the only references to a piece of production code are from tests, that code is **dead and must be removed**, not preserved.

Tests must adapt to real behavior — production code must never be kept solely to satisfy tests.

### Removal requirements

- Remove unused modules, functions, types, traits, and constants
- Remove unused feature flags and conditional compilation paths
- Remove unused dependencies and unused dependency features
- Remove redundant abstractions with no clear responsibility
- Prefer deleting code over deprecating it unless external compatibility is explicitly required

---

## Testing Philosophy

### Black-box testing only

Tests must:
- Interact through public APIs or externally observable integration points
- Assert inputs → outputs, side effects, invariants, and error guarantees
- Be resilient to internal refactors

Tests must not:
- Inspect private fields or internal module structure
- Assert internal call order or helper invocation
- Mirror internal algorithms

If a test breaks due to internal refactoring without a behavior change, the test is incorrect.

### Behavior over implementation

Tests should assert:
- Observable outcomes
- Domain rules and constraints
- Error conditions that callers must handle

Tests must not:
- Assert internal types unless explicitly part of the public contract
- Lock in exact error strings unless guaranteed by API
- Encode timing, ordering, or concurrency assumptions unless guaranteed

### Mocking discipline

Mocking is allowed **only at true architectural boundaries**:
- External services
- Filesystem, OS, clock, randomness
- Databases, queues, message brokers

Mocking is **not allowed** for:
- Domain logic
- Internal collaborators within the same conceptual unit
- Pure or deterministic logic

If heavy mocking exists, treat it as evidence of poor boundaries.

---

## Guiding Principles

- **Tests do not legitimize production code.** If code exists only for tests, delete both.
- **Good tests protect behavior, not implementation.**
- **Dead code is a liability, not an asset.**
- **Prefer deletion over suppression.**
