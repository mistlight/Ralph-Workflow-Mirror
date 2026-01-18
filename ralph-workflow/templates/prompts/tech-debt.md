# Technical Debt: [Area/Component]

> **How to use this template:** This template is for addressing technical debt in a structured way. It helps prioritize, plan, and execute refactoring work with clear outcomes.

## Goal
[Clear description of what technical debt needs to be addressed]

**Tips for a good technical debt goal:**
- ✅ "Refactor user authentication module to improve testability"
- ✅ "Replace deprecated library X with modern alternative Y"
- ❌ "Clean up the code" (too vague)

**EXAMPLE:**
```markdown
## Goal
Refactor the payment processing module to extract payment gateway logic into separate abstractions, improving testability and enabling easier addition of new payment providers.
```

## Questions to Consider
Before addressing technical debt, evaluate:

**Impact:**
- What problems does this debt cause? (bugs, slow development, outages)
- How often does this area change? (frequency of pain)
- Who is affected? (developers, users, operations)
- What's the risk of not addressing it now?

**Priority:**
- Is this blocking other work?
- Is this getting worse over time?
- Will this be more expensive to fix later?
- Are there quick wins that provide significant value?

**Scope:**
- What exactly needs to be refactored?
- What's the minimum viable fix?
- Can this be done incrementally?
- Are there dependencies or coupling to consider?

**Risk:**
- What could go wrong during refactoring?
- How will we ensure behavior is preserved?
- Do we have tests to verify correctness?
- What's the rollback plan if things go wrong?

## Acceptance Checks
- [Technical debt reduced or eliminated]
- [Code is more maintainable than before]
- [All existing tests still pass]
- [New tests added if applicable]
- [No regressions in functionality]
- [Documentation updated if needed]
- [Performance is not degraded]

## Technical Debt Details

### Type of Debt
Choose the categories that apply:

- [ ] **Code Smell**: Poor code structure, naming, or organization
- [ ] **Duplication**: Same logic repeated in multiple places
- [ ] **Dead Code**: Unused or commented-out code
- [ ] **Complexity**: Overly complex logic or abstractions
- [ ] **Coupling**: High dependency between modules
- [ ] **Missing Tests**: Insufficient test coverage
- [ ] **Dependencies**: Outdated or vulnerable libraries
- [ ] **Performance**: Suboptimal algorithms or data structures
- [ ] **Documentation**: Missing or outdated documentation
- [ ] **Configuration**: Hardcoded values or missing config

### Current Pain Points
[Describe the specific problems this debt causes]

**EXAMPLE:**
```markdown
### Current Pain Points

**Developer Experience:**
- Adding a new payment gateway requires modifying 10+ files
- Unit tests require mocking entire payment flow
- Code is difficult to understand for new developers

**Operational:**
- Payment gateway secrets are hardcoded
- No visibility into which gateway is being used
- Errors from payment gateways are not logged consistently

**Maintenance:**
- Current payment library is deprecated and unmaintained
- Security vulnerabilities in version 2.1.0
- Cannot upgrade without breaking changes
```

### Proposed Solution
[Describe the refactoring approach]

**EXAMPLE:**
```markdown
### Proposed Solution

1. Extract payment gateway interface with standard methods:
   - `process_payment(amount, method)`
   - `refund(transaction_id)`
   - `get_status(transaction_id)`

2. Implement existing gateway as adapter

3. Move gateway-specific logic to separate modules

4. Add configuration for gateway selection and credentials

5. Add integration tests for each gateway
```

## Code Quality Specifications

**Refactoring Best Practices:**
- Make small, incremental changes
- Ensure tests pass after each change
- Preserve existing behavior unless intentionally changing it
- Add tests before refactoring (if missing)
- Commit frequently with clear messages
- Run the full test suite before and after

**What to Aim For:**
- Single responsibility: one reason to change per function/class
- Small units: functions < 30 lines, classes < 300 lines
- Clear names that reveal intent
- Early returns; minimize nesting depth
- Explicit error handling; no silent failures
- No magic numbers; extract constants
- DRY: extract duplicated logic
- Validate at boundaries; trust internal data

**When to Accept Technical Debt:**
- When speed is critical (prototype, MVP)
- When requirements will change significantly
- When the cost of fixing exceeds the benefit
- When more information is needed before refactoring

**When to Pay Down Technical Debt:**
- When it slows down feature development
- When it causes bugs or outages
- When new developers are struggling
- When it's blocking important work
- When the team has capacity between features
