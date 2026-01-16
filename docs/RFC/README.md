# Ralph RFCs (Request for Comments)

This directory contains design proposals for significant changes to Ralph.

## What is an RFC?

An RFC is a design document that describes a proposed change to Ralph. RFCs are used for:

- Major new features
- Significant architectural changes
- Breaking changes
- Cross-cutting concerns that affect multiple components

## RFC Process

1. **Draft**: Create a new RFC document using the template below
2. **Review**: Share with maintainers and community for feedback
3. **Accepted/Rejected**: Decision made based on discussion
4. **Implemented**: Code changes made according to the RFC
5. **Completed**: RFC marked as implemented

## RFC Status

| Status | Meaning |
|--------|---------|
| Draft | Under development, not yet ready for review |
| In Progress | Implementation has started, some features complete |
| Review | Open for community feedback |
| Accepted | Approved for implementation |
| Rejected | Not moving forward |
| Implemented | Code changes complete |
| Superseded | Replaced by a newer RFC |

## Current RFCs

| RFC | Title | Status |
|-----|-------|--------|
| [RFC-001](./RFC-001-enhanced-opencode-integration.md) | Enhanced OpenCode Integration | Draft |
| [RFC-002](./RFC-002-ux-improvements.md) | Developer Experience Improvements | In Progress |
| [RFC-003](./RFC-003-streaming-architecture-hardening.md) | AI Agent Streaming Architecture Hardening | Implemented |

## Creating a New RFC

1. Copy the template below
2. Name your file `RFC-NNN-short-description.md` where NNN is the next available number
3. Fill in all sections
4. Submit a PR or share for discussion

## RFC Template

```markdown
# RFC-NNN: Title

**RFC Number**: RFC-NNN
**Title**: Full Title Here
**Status**: Draft
**Author**: Your Name
**Created**: YYYY-MM-DD

---

## Abstract

Brief summary of the proposal (2-3 sentences).

---

## Motivation

Why is this change needed? What problem does it solve?

---

## Proposed Changes

Detailed description of the changes.

---

## Implementation Priority

| Item | Effort | Impact | Priority |
|------|--------|--------|----------|
| ... | ... | ... | ... |

---

## Success Criteria

How do we know when this is done?

---

## Risks & Mitigations

What could go wrong and how do we prevent it?

---

## Alternatives Considered

What other approaches were considered and why were they rejected?

---

## References

Links to relevant code, docs, issues.

---

## Open Questions

Unresolved decisions that need input.
```
