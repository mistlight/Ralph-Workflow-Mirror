# Implementation Plan: Complete Site Styling

## Summary

The Ralph Workflow documentation site has incomplete styling. The main pages (index.html, how-it-works.html, getting-started.html, faq.html) are well-styled with the Terminal Noir dark theme, but **4 pages have HTML components with missing CSS definitions**:

1. **open-source.html** - License grid, contribute cards, values grid, acknowledgments
2. **docs/writing-specs.html** - Breadcrumbs, template sections, example specs, checklists, tip cards
3. **docs/overnight-runs.html** - Safety callouts, config sections, overnight steps, review checklists, cost grid
4. **docs/workflows.html** - Workflow steps detailed, mode reference table

The visual audit (via Playwright screenshots) confirmed these pages render with unstyled content - components display as plain stacked elements without proper grid layouts, card styling, or visual hierarchy.

---

## Implementation Steps

### Step 1: Add Breadcrumb Styles
**Files:** `styles.css`
**Components:** `.breadcrumb`, `.breadcrumb ol`, `.breadcrumb li`, `.breadcrumb a`

Create navigation breadcrumb styling with:
- Horizontal inline list with separators
- Muted text color for non-active items
- Current page styling (no link, different color)
- Proper spacing and typography matching the terminal aesthetic

---

### Step 2: Add Open Source Page Styles
**Files:** `styles.css`
**Components:**
- `.license-grid`, `.license-content`, `.license-summary`, `.license-points`, `.license-point`
- `.license-point--can`, `.license-point--must`, `.license-point--cannot`
- `.license-sidebar`, `.license-clarification`, `.license-link`
- `.contribute-grid`, `.contribute-card`, `.contribute-icon`, `.contribute-link`, `.contribute-intro`
- `.values-grid`, `.value-card`
- `.acknowledgments-content`, `.acknowledgments-list`
- `.dev-setup`

Create:
- 2-column license grid with main content and sidebar
- Colored license point badges (green for CAN, amber for MUST, red for CANNOT)
- 4-column contribute cards grid with icons and links
- Values grid with card styling
- Acknowledgments list styling

---

### Step 3: Add Writing Specs Page Styles
**Files:** `styles.css`
**Components:**
- `.template-intro`, `.template-tips`
- `.example-spec`, `.example-header`, `.example-badge`, `.example-badge--developer`, `.example-badge--vibe`
- `.example-analysis`
- `.checklist-grid`, `.checklist-card`, `.checklist-card--must`, `.checklist-card--should`, `.checklist-card--avoid`
- `.checklist`
- `.tips-grid`, `.tip-card`

Create:
- Example spec containers with headers and analysis sections
- Badge variants for developer vs vibe coder examples
- 3-column checklist grid with colored card variants
- Checkbox styling for checklists
- Tips grid with card styling

---

### Step 4: Add Overnight Runs Page Styles
**Files:** `styles.css`
**Components:**
- `.safety-callout`, `.safety-grid`, `.safety-card`, `.safety-icon`, `.safety-content`
- `.config-example`, `.config-tips`, `.config-list`
- `.overnight-steps`, `.overnight-step`, `.step-badge`
- `.review-checklist`, `.review-item`, `.review-icon`, `.review-content`
- `.cost-grid`, `.cost-card`, `.cost-note`

Create:
- Safety callout with warning styling and icon
- Safety grid with 4-column cards
- Config tips using definition list styling
- Numbered overnight steps with badges
- Review checklist with icon + content layout
- Cost estimation grid with cards

---

### Step 5: Add Workflows Page Styles
**Files:** `styles.css`
**Components:**
- `.workflow-example`, `.workflow-intro`
- `.workflow-steps-detailed`, `.workflow-step-detailed`, `.workflow-step-header`, `.workflow-step-number`
- `.workflow-command`, `.workflow-tips`
- `.mode-reference`, `.mode-table`

Create:
- Workflow example containers
- Detailed step containers with numbered badges
- Step headers with numbers
- Tips section styling
- Mode reference table with proper styling matching the terminal theme

---

### Step 6: Responsive Breakpoints
**Files:** `styles.css`

Add responsive styles for all new components:
- Mobile (max-width: 640px): Single column layouts
- Tablet (max-width: 768px): 2-column layouts where appropriate
- Desktop (max-width: 1024px): Full grid layouts

---

### Step 7: Visual Verification
**Method:** Playwright browser screenshots

Verify each page renders correctly:
1. open-source.html - License grid, contribute cards, values display properly
2. docs/writing-specs.html - Examples, checklists, tips render with correct styling
3. docs/overnight-runs.html - Safety callouts, steps, cost grid display correctly
4. docs/workflows.html - Workflow steps, mode table render properly

---

## Critical Files for Implementation

| File | Purpose |
|------|---------|
| `styles.css` | Add all missing component styles (lines 4489+) |
| `open-source.html` | Reference for class names and structure |
| `docs/writing-specs.html` | Reference for class names and structure |
| `docs/overnight-runs.html` | Reference for class names and structure |
| `docs/workflows.html` | Reference for class names and structure |

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Style conflicts with existing CSS | Use specific selectors, follow existing naming patterns |
| Inconsistent visual language | Reference existing component styles (feature cards, FAQ items) as templates |
| Responsive breakage | Test at 640px, 768px, 1024px breakpoints |
| Dark/light mode issues | Use CSS variables consistently (--color-*, --border-*, etc.) |

---

## Verification Strategy

1. **Visual inspection via Playwright** - Take full-page screenshots of all 4 pages
2. **Responsive testing** - Resize viewport to test mobile/tablet breakpoints
3. **Cross-reference with index.html** - Ensure visual consistency with main pages
4. **CSS validation** - Ensure no syntax errors, proper selector specificity

---

## Design Guidelines (from frontend-design skill)

The site uses **Terminal Noir** aesthetic:
- **Typography:** Syne (display), DM Sans (body), JetBrains Mono (code)
- **Colors:** Dark background (#0a0a0a), cyan primary (#22d3ee), pink accent (#ff006e), lime success (#a3ff12)
- **Components:** Cards with soft borders, subtle hover states, gradient accents
- **Motion:** Smooth transitions (150-300ms), reveal animations on scroll

All new styles must maintain this cohesive dark terminal aesthetic.
