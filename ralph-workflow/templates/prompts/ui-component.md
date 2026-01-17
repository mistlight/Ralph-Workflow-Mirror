# UI Component: [Brief title]

> **How to use this template:** This template is for frontend UI component development. Fill in the goal and acceptance criteria below to guide the AI agent.

## Goal
[One-line description of the UI component or feature]

## Questions to Consider

**Component Design:**
* What is the component's primary purpose and responsibility?
* What props/inputs does the component accept?
* What events/actions does the component emit?
* Should the component be reusable or single-purpose?

**User Experience:**
* How should the component behave on user interaction?
* Are there any loading, error, or empty states to handle?
* Should the component support keyboard navigation?
* How should the component respond to different screen sizes?

**Accessibility:**
* What ARIA roles and attributes are needed?
* Should the component support screen readers?
* Are there any keyboard shortcuts or focus management needs?
* Does the component have sufficient color contrast?

**Styling:**
* Should the component use a specific design system or theme?
* Are there any animation or transition requirements?
* How should the component handle dark mode or theme switching?
* Are there any responsive design considerations?

## Acceptance Checks
* [Component renders correctly with all prop combinations]
* [User interactions trigger expected events]
* [Component is accessible via keyboard and screen reader]
* [Responsive design works on mobile and desktop]
* [Error and loading states display appropriately]
* [Component passes automated accessibility tests]
* [Visual design matches mockups or design system]
* [Component has storybook or example documentation]

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
- Sanitize data before display (prevent XSS)
- Use parameterized queries to prevent injection attacks
- Follow the principle of least privilege for permissions
- Never log sensitive data (passwords, tokens, PII)
- Consider CSP (Content Security Policy) for inline scripts

**EXAMPLE:**
```markdown
# UI Component: Search Autocomplete

## Goal
Create a search input component that shows suggestions as the user types.

## Questions to Consider
**Component Design:**
- Props: placeholder, minChars, debounceTime
- Events: onSearch, onSelect
- Reusable across the application

**User Experience:**
- Show loading indicator while fetching
- Highlight matching text in suggestions
- Close on Escape or click outside

**Accessibility:**
- ARIA role="combobox"
- Arrow key navigation through suggestions
- Enter to select, Escape to close

**Acceptance Checks:**
- [Shows suggestions after 3 characters]
- [Keyboard navigation works correctly]
- [Screen reader announces suggestion count]
```
