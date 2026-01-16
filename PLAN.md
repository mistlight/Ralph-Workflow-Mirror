# Design Stabilization Implementation Plan

## Summary

This plan addresses a **critical design system gap**: the post-Section 3 pages (getting-started.html, how-it-works.html, faq.html, open-source.html, and all docs/*.html pages) use dozens of CSS classes that **do not exist** in styles.css. These pages render without their intended styling because the HTML references undefined selectors.

**Root cause confirmed**: Searching styles.css for classes like `.content-container`, `.page-header`, `.page-title`, `.prereq-grid`, `.install-steps`, `.safety-callout`, `.workflow-example`, `.license-grid`, `.faq-list`, `.cta-content`, and `.breadcrumb` returns **zero matches**. The HTML was written expecting these styles to exist, but they were never added.

The fix requires adding comprehensive CSS definitions for all missing component classes while maintaining consistency with the established Terminal Noir design system from the index.html hero sections.

---

## Top 4 Fundamental Causes of Visual Inconsistency

### 1. **Missing CSS Class Definitions**
The subpages use approximately **60+ CSS class names** that have no corresponding styles in styles.css. This includes:
- Layout classes: `.content-container`, `.page-header`, `.page-header-content`
- Typography classes: `.page-title`, `.page-subtitle`, `.section-header--left`, `.section-number`
- Component grids: `.prereq-grid`, `.safety-grid`, `.cost-grid`, `.license-grid`, `.contribute-grid`, `.values-grid`
- Component cards: `.prereq-card`, `.safety-card`, `.cost-card`, `.value-card`, `.contribute-card`
- Step components: `.install-steps`, `.install-step`, `.overnight-steps`, `.overnight-step`, `.workflow-steps-detailed`
- Callout components: `.safety-callout`, `.troubleshooting-box`, `.config-tips`, `.license-clarification`
- FAQ components: `.faq-list`, `.faq-item`, `.faq-summary`, `.faq-toggle`, `.faq-content`
- CTA components: `.cta-section`, `.cta-content`, `.cta-buttons`
- Navigation: `.breadcrumb`

### 2. **Inconsistent Container Model**
- index.html uses `.container` (defined, 1280px max-width)
- Subpages use `.content-container` (undefined, no styles applied)
- This creates width inconsistency between pages

### 3. **Typography Hierarchy Not Applied**
- Section numbers use `.section-number` class (undefined)
- Page titles use `.page-title` class (undefined)
- Left-aligned headers use `.section-header--left` modifier (undefined)

### 4. **Component Patterns Diverged**
- Index.html hero sections have well-styled components
- Subpage components reference different class names with no styles

---

## Implementation Passes

### Pass A — Layout Model Unification (styles.css additions)

**Goal**: Establish consistent page containers, section layouts, and content widths.

**CSS to add:**
1. `.content-container` - Narrower reading-width container (~900px max-width), centered, with --space-6 padding
2. `.page-header` - Hero-like section with --space-24 top padding, --space-16 bottom, --color-bg-alt background
3. `.page-header-content` - Center-aligned content wrapper with max-width constraint
4. `.page-header-badges` - Flex container for badges, centered, with margin-bottom
5. `.page-title` - Display font, clamp(2rem, 6vw, 3.5rem), tight letter-spacing
6. `.page-subtitle` - Muted color, --text-lg, max-width 600px
7. `.section-header--left` - Left-aligned modifier (text-align: left, margin: 0)
8. `.section-number` - Monospace, uppercase, --color-primary, tracking-widest, small size
9. `.section--alt` - Alternate background (--color-bg-alt) with subtle top border gradient

**Responsive additions:**
- Mobile adjustments for page-header padding
- Touch-friendly spacing for content-container

---

### Pass B — Typography & Rhythm Restoration (styles.css additions)

**Goal**: Re-establish heading scale and consistent vertical rhythm.

**CSS to add:**
1. `.lead-paragraph` - Larger body text (--text-lg or --text-xl), --color-text-primary
2. `.step-note` - Muted helper text below steps
3. `.step-success` - Success indicator with green/lime styling
4. `.code-inline-block` - Inline code container styling
5. Responsive typography adjustments for headings on mobile

---

### Pass C — Component Normalization (styles.css additions)

**Goal**: Create canonical styles for all shared components.

**Getting-started.html components:**
1. `.prereq-grid` - 3-column responsive grid
2. `.prereq-card` - Card with icon, title, description, link
3. `.prereq-icon` - Icon container with primary-dim background
4. `.prereq-link` - Inline link with external icon
5. `.prereq-list` - Styled bullet list
6. `.install-steps` - Vertical step container
7. `.install-step` - Individual step with header + content
8. `.install-step-header` - Flex layout for number + heading
9. `.install-step-number` - Circular number indicator with glow
10. `.troubleshooting-box` - Callout box with icon heading
11. `.troubleshooting-list` - Styled troubleshooting items

**Overnight-runs.html components:**
12. `.safety-callout` - Large callout with icon + content
13. `.safety-grid` - 2x2 responsive grid
14. `.safety-card` - Card with icon heading
15. `.safety-icon` - Icon container
16. `.safety-content` - Content area
17. `.config-example` - Code example container
18. `.config-tips` - Tips section with heading
19. `.config-list` - Definition list (dt/dd) styling
20. `.overnight-steps` - Vertical step flow
21. `.overnight-step` - Step with number + content
22. `.review-checklist` - Review item list
23. `.review-item` - Icon + content layout
24. `.review-icon` - Review icon container
25. `.review-content` - Review content area
26. `.cost-grid` - 3-column grid
27. `.cost-card` - Cost info card
28. `.cost-note` - Muted note text

**Workflows.html components:**
29. `.workflow-example` - Workflow example container
30. `.workflow-intro` - Intro paragraph styling
31. `.workflow-steps-detailed` - Detailed step container
32. `.workflow-step-detailed` - Individual detailed step
33. `.workflow-step-header` - Header with badge + title
34. `.step-badge` - "Step 1" style badge
35. `.workflow-command` - Command display section
36. `.workflow-tips` - Tips box with list
37. `.mode-reference` - Table container
38. `.mode-table` - Styled data table with hover, alignment rules

**Open-source.html components:**
39. `.license-grid` - 2-column responsive layout
40. `.license-content` - Main content area
41. `.license-summary` - Summary section
42. `.license-points` - Grid for CAN/MUST/CANNOT
43. `.license-point` - Point card with variants (--can, --must, --cannot)
44. `.license-sidebar` - Sidebar styling
45. `.license-clarification` - Clarification callout
46. `.license-link` - Link with icon
47. `.contribute-intro` - Intro text
48. `.contribute-grid` - 4-column responsive grid
49. `.contribute-card` - Contribution card
50. `.contribute-icon` - Large icon container
51. `.contribute-link` - Card action link
52. `.dev-setup` - Development setup section
53. `.values-grid` - 4-column values grid
54. `.value-card` - Simple value card
55. `.acknowledgments-content` - Content container
56. `.acknowledgments-list` - Styled acknowledgments

---

### Pass D — Docs-Specific Polish (styles.css additions)

**Goal**: Add remaining documentation UI patterns.

**CSS to add:**
1. `.breadcrumb` - Navigation breadcrumb with separator styling
2. `.cta-section` - Call-to-action section styling
3. `.cta-content` - CTA content wrapper
4. `.cta-buttons` - Button group with gap and centering
5. `.faq-list` - FAQ container
6. `.faq-item` - Details element styling with border
7. `.faq-summary` - Summary with hover, cursor
8. `.faq-toggle` - +/- indicator with rotation
9. `.faq-content` - Answer content with animation

**Responsive additions for all components:**
- Grid collapses at 1024px and 640px breakpoints
- Touch target minimum sizes
- Mobile-friendly spacing

---

## Critical Files for Implementation

| File | Purpose | Estimated Lines |
|------|---------|-----------------|
| `styles.css` | All new CSS classes | +400-500 lines |

**HTML files require NO changes** - the class names are already correct; only CSS definitions are missing.

---

## Risks & Mitigations

### Risk 1: Breaking index.html Sections 1-3
**Mitigation:** All new classes are scoped to subpage components. No modifications to existing selectors. Verify hero sections after implementation.

### Risk 2: Inconsistent styling within new components
**Mitigation:** Use only existing design tokens (--space-*, --color-*, --radius-*, --shadow-*). Follow existing component patterns from feature-card, code-block, etc.

### Risk 3: Mobile/tablet regression
**Mitigation:** Add responsive breakpoints following existing 1024px/768px/640px pattern. Test at all breakpoints.

### Risk 4: Light mode inconsistency
**Mitigation:** Use CSS custom properties for all colors. Test light mode toggle after implementation.

---

## Verification Strategy

After each pass, verify:

1. **Visual rendering:** Open each subpage and confirm components now display styled
2. **Spacing consistency:** Verify vertical rhythm matches index.html sections
3. **Component parity:** Cards, code blocks, callouts look consistent
4. **Responsive behavior:** Test at 1440px, 1024px, 768px, 640px, 375px
5. **Interactive states:** Hover, focus-visible, active states work
6. **Theme toggle:** Light and dark modes work correctly
7. **File compatibility:** Open via file:// protocol, no server needed

---

## Completion Criteria

The task is complete when:

- [ ] All CSS classes referenced in HTML have corresponding definitions in styles.css
- [ ] Post-Section 3 pages render with full styling (no unstyled elements)
- [ ] Visual rhythm matches index.html baseline
- [ ] Typography hierarchy is consistent across all pages
- [ ] All interactive components have proper state styling
- [ ] Responsive layouts work at all breakpoints
- [ ] Light and dark mode both render correctly
- [ ] Pages work when opened directly as files
