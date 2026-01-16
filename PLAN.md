# Design Stabilization Implementation Plan

## Summary

This plan addresses **critical design system gaps** across the Ralph Workflow static site. Through exploration, I've confirmed two interrelated problems:

1. **Missing CSS definitions**: Approximately **60+ CSS classes** referenced in post-Section 3 HTML have **no corresponding styles** in styles.css. Classes like `.content-container`, `.page-header`, `.features-grid`, `.audience-grid`, `.faq-container`, `.prereq-grid`, `.install-steps`, and many more return zero matches when searched.

2. **Font stack inconsistency**: Secondary pages (faq.html, getting-started.html, how-it-works.html, open-source.html, docs/*.html) import different fonts (Space Grotesk, Instrument Serif, IBM Plex Mono) than index.html's Terminal Noir theme (Syne, DM Sans, JetBrains Mono), creating visual dissonance.

**The index.html Sections 1-3 are also affected**: Even on the main page, classes like `.what-grid`, `.what-section`, `.how-section`, `.workflow-steps`, `.quick-benefits`, and `.features-grid` are undefined. These sections rely on fallback browser styling rather than intentional design.

The fix requires adding comprehensive CSS definitions while ensuring font consistency across all pages.

---

## Top 4 Fundamental Causes of Visual Inconsistency

### 1. **Missing CSS Class Definitions (~60+ classes)**

**On index.html (including Sections 1-3)**:
- Layout: `.what-grid`, `.what-section`, `.how-section`, `.what-content`, `.what-sidebar`
- Components: `.workflow-steps`, `.workflow-step`, `.step-number`, `.step-content`
- Grids: `.features-grid`, `.audience-grid`, `.quick-benefits`, `.quick-benefits-grid`
- Cards: `.quick-benefit-card`, `.key-points`, `.key-point`
- FAQ: `.faq-container`, `.faq-list`, `.faq-category` (with variants)
- Install: `.install-mode-toggle`, `.install-info`, `.install-requirements`, `.requirement`
- Demo: `.demo-container`, `.demo-interface`, `.demo-tabs`, `.demo-panel`, `.demo-editor`

**On subpages**:
- Layout: `.content-container`, `.page-header`, `.page-header-content`
- Typography: `.page-title`, `.page-subtitle`, `.section-header--left`
- Prereqs: `.prereq-grid`, `.prereq-card`, `.prereq-icon`, `.prereq-link`
- Steps: `.install-steps`, `.install-step`, `.overnight-steps`, `.workflow-steps-detailed`
- Callouts: `.safety-callout`, `.troubleshooting-box`, `.config-tips`
- Docs: `.breadcrumb`, `.template-intro`, `.example-spec`, `.mode-table`

### 2. **Font Stack Inconsistency Across Pages**

- **index.html** loads: Syne, DM Sans, JetBrains Mono (Terminal Noir)
- **All other pages** load: Instrument Serif, Space Grotesk, IBM Plex Mono (different theme)
- Theme-color meta also differs: index uses `#0a0a0b`, others use `#0d1f18`

### 3. **Inconsistent Container Model**
- index.html uses `.container` (defined, 1280px max-width)
- Subpages use `.content-container` (undefined, no styles applied)
- Some sections use neither, creating unpredictable widths

### 4. **Component Patterns Diverged**
- Index.html hero and comparison sections are well-styled
- Post-Section 3 components reference classes with no definitions
- Subpage components use entirely different naming conventions

---

## Implementation Passes

### Pass 0 — Font & Theme Unification (HTML changes)

**Goal**: Ensure all pages use the Terminal Noir font stack and theme colors.

**Files to modify**: faq.html, getting-started.html, how-it-works.html, open-source.html, docs/writing-specs.html, docs/overnight-runs.html, docs/workflows.html

**Changes**:
1. Replace font import line:
   ```html
   <!-- Replace this -->
   <link href="https://fonts.googleapis.com/css2?family=Instrument+Serif:ital@0;1&family=Space+Grotesk:wght@300;400;500;600;700&family=IBM+Plex+Mono:wght@400;500;600;700&display=swap" rel="stylesheet">

   <!-- With this -->
   <link href="https://fonts.googleapis.com/css2?family=Syne:wght@400;500;600;700;800&family=DM+Sans:ital,opsz,wght@0,9..40,100..1000;1,9..40,100..1000&family=JetBrains+Mono:ital,wght@0,100..800;1,100..800&display=swap" rel="stylesheet">
   ```

2. Replace theme-color meta:
   ```html
   <!-- Replace -->
   <meta name="theme-color" content="#0d1f18">
   <!-- With -->
   <meta name="theme-color" content="#0a0a0b">
   ```

---

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

**Index.html - "What is Ralph" section:**
1. `.what-section` - Section container with background
2. `.what-grid` - 2-column layout (content + sidebar)
3. `.what-content` - Main content area
4. `.what-lead` - Lead paragraph styling
5. `.workflow-steps` - Vertical step container
6. `.workflow-step` - Individual workflow step
7. `.step-number` - Circular step number
8. `.step-content` - Step text content
9. `.workflow-diagram` - SVG diagram container
10. `.what-sidebar` - Sidebar container
11. `.key-points` - Key points list
12. `.key-point` - Individual key point card

**Index.html - "How it Works" section:**
13. `.how-section` - Section container
14. `.quick-benefits` - Benefits container
15. `.quick-benefits-grid` - 3-column grid
16. `.quick-benefit-card` - Benefit card
17. `.quick-benefit-header` - Card header with icon
18. `.quick-benefit-title` - Title with color variants
19. `.how-steps` - Horizontal steps container
20. `.how-step` - Individual step
21. `.how-icon` - Step icon container

**Index.html - Comparison section:**
22. `.comparison-grid` - 2-column before/after layout
23. `.comparison-card` - Card with variants (--after)
24. `.comparison-badge` - Before/After badge
25. `.comparison-title` - Card title with variants
26. `.comparison-list` - List container
27. `.comparison-item` - Item with variants (--positive, --negative)
28. `.comparison-icon` - Icon with variants
29. `.comparison-footer` - Card footer
30. `.comparison-time-value` - Time display with color variants
31. `.comparison-summary` - Summary text

**Index.html - Glossary section:**
32. `.glossary-container` - Container wrapper
33. `.glossary-details` - Details element styling
34. `.glossary-summary` - Collapsible summary
35. `.glossary-grid` - 3-column grid
36. `.glossary-term` - Term card
37. `.glossary-term-header` - Term header with icon
38. `.glossary-term-icon` - Icon container
39. `.glossary-term-name` - Term name
40. `.glossary-term-content` - Definition content
41. `.glossary-note` - Bottom note

**Index.html - Demo section:**
42. `.demo-container` - Demo section wrapper
43. `.demo-interface` - Interface card
44. `.demo-header` - Header with tabs
45. `.demo-tabs` - Tab container
46. `.demo-tab` - Individual tab
47. `.demo-run-btn` - Run button
48. `.demo-content` - Content area
49. `.demo-panel` - Tab panel
50. `.demo-editor` - Code editor styling
51. `.demo-line-number` - Line numbers
52. `.demo-textarea` - Editable textarea
53. `.demo-terminal` - Terminal output
54. `.demo-terminal-line` - Terminal line
55. `.demo-code` - Generated code display
56. `.demo-status-bar` - Status bar
57. `.demo-explanation` - Explanation sidebar
58. `.demo-step` - Explanation step

**Index.html - Install section extended:**
59. `.install-mode-toggle` - Simple/Advanced toggle
60. `.mode-label` - Toggle labels
61. `.mode-switch` - Toggle switch
62. `.mode-switch-slider` - Slider element
63. `.install-info` - Info sidebar
64. `.install-requirements` - Requirements list
65. `.requirement` - Individual requirement
66. `.requirement-icon` - Requirement icon
67. `.install-tabs` - Tab container
68. `.install-tab` - Individual tab
69. `.install-content` - Content panel
70. `.install-note` - Note text
71. `.install-verify` - Verification text
72. `.troubleshooting-details` - Collapsible troubleshooting
73. `.troubleshooting-summary` - Summary element
74. `.troubleshooting-chevron` - Chevron icon
75. `.troubleshooting-content` - Content area

**Index.html - Features section:**
76. `.features-grid` - 3-column grid (matching .feature-grid pattern)
77. `.feature-expand-btn` - Expandable feature button
78. `.feature-expand-content` - Expanded content

**Index.html - Audience section:**
79. `.audience-grid` - 3-column grid
80. `.audience-badge` - Badge with variants (--primary, --accent)

**Index.html - FAQ section:**
81. `.faq-container` - FAQ wrapper
82. `.faq-category` - Category heading with variants

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

| File | Purpose | Changes |
|------|---------|---------|
| `styles.css` | All new CSS class definitions | +600-800 lines of CSS |
| `faq.html` | Font import fix, theme-color fix | 2 line replacements |
| `getting-started.html` | Font import fix, theme-color fix | 2 line replacements |
| `how-it-works.html` | Font import fix, theme-color fix | 2 line replacements |
| `open-source.html` | Font import fix, theme-color fix | 2 line replacements |
| `docs/writing-specs.html` | Font import fix, theme-color fix | 2 line replacements |
| `docs/overnight-runs.html` | Font import fix, theme-color fix | 2 line replacements |
| `docs/workflows.html` | Font import fix, theme-color fix | 2 line replacements |

**Note**: HTML class names are already correct—only CSS definitions and font imports need fixing.

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
