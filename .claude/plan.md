# Design Stabilization Plan for Ralph Workflow Website

## Summary

The Ralph Workflow website currently implements a **Terminal Noir** aesthetic with electric cyan (#00d4ff), hot magenta (#ff006e), and lime green (#a3ff12) as signature colors, paired with Syne/DM Sans/JetBrains Mono typography. After comprehensive visual inspection of all 7+ pages and analysis of the 5,724-line `styles.css`, the site demonstrates strong foundational design work but requires targeted refinements to achieve full production-grade polish per the frontend-design skill requirements.

**Key Findings:**
- Design system is well-established with CSS custom properties
- Terminal Noir aesthetic is consistently applied across pages
- Several CSS organization issues need cleanup (duplicate rules, naming inconsistencies)
- Missing PostCSS/Stylelint tooling configuration referenced in CLAUDE.md
- Light mode needs additional refinement for visual hierarchy
- Some animation and micro-interaction opportunities remain unexplored

---

## Implementation Steps

### Phase 1: CSS Architecture Cleanup (Critical)

#### 1.1 Remove Duplicate `.hero-title` Rules
**Files:** `styles.css`
**Issue:** `.hero-title` is defined twice - at line 1215 and line 4087 with slightly different properties.
**Action:** Consolidate into a single definition, keeping the responsive media queries organized together.

```
Location 1: lines 1215-1241 (original definition)
Location 2: lines 4087-4119 (duplicate with different mobile breakpoints)
```

#### 1.2 Standardize Badge Color Naming
**Files:** `styles.css`
**Issue:** Badge classes use mixed naming conventions - old forest theme names (`badge-forest`, `badge-amber`, `badge-moss`) alongside Terminal Noir names (`badge-primary`, `badge-accent`, `badge-success`).
**Action:** Remove the deprecated forest/amber/moss variants and update any HTML references to use the Terminal Noir naming convention.

```css
/* Current (lines 4031-4065): Both naming patterns exist */
.badge-forest, .badge-primary { ... }  /* Redundant */
.badge-amber, .badge-accent { ... }    /* Redundant */
.badge-moss, .badge-success { ... }    /* Redundant */

/* Target: Single naming pattern */
.badge-primary { ... }
.badge-accent { ... }
.badge-success { ... }
```

#### 1.3 Implement Custom Media Queries
**Files:** Create `tokens.css`, update `styles.css`
**Issue:** CLAUDE.md references `@custom-media` usage but styles.css uses hardcoded breakpoint values (640px, 768px, 900px, 1024px) inconsistently.
**Action:** Define reusable breakpoint tokens and migrate to consistent usage.

```css
/* tokens.css - to create */
@custom-media --sm (max-width: 480px);
@custom-media --md (max-width: 768px);
@custom-media --lg (max-width: 1024px);
```

### Phase 2: Visual Hierarchy Refinement

#### 2.1 Enhance Section Differentiation
**Files:** `styles.css`, all HTML files
**Issue:** Some content sections blend together without clear visual separation.
**Action:** Add subtle background variations and enhanced border treatments to section containers.

**Specific areas to address:**
- FAQ section headings (Getting Started, Installation, Usage, etc.) need stronger visual hierarchy
- "How Ralph Works" steps on index.html could use more visual distinction
- Documentation page sections need clearer separation

#### 2.2 Light Mode Polish
**Files:** `styles.css` (lines 267-313)
**Issue:** Light mode is functional but lacks the same polish as dark mode. Some text contrast could be improved for better readability.
**Action:**
- Increase contrast for muted text colors
- Enhance card shadows for better depth perception
- Ensure all interactive elements have visible hover states

### Phase 3: Typography & Spacing Consistency

#### 3.1 Normalize Section Header Treatment
**Files:** `styles.css`
**Issue:** Page hero titles and section headers have varying styling approaches across different pages.
**Action:** Create unified section header component styles:
- Primary page titles (h1 in heroes)
- Section headers (h2 with numbered prefixes like "01", "02")
- Sub-section headers (h3)

#### 3.2 Code Block Consistency
**Files:** `styles.css`, HTML files
**Issue:** Code blocks appear with slightly different styling in different contexts (inline terminal demos vs code snippets in docs).
**Action:** Ensure all code blocks share base terminal styling with appropriate contextual variations.

### Phase 4: Animation & Interaction Enhancement

#### 4.1 Extend Scroll Reveal Coverage
**Files:** All HTML files, `styles.css`
**Issue:** Scroll reveal animations (`.reveal`, `.stagger-children`) are implemented but not applied consistently across all pages.
**Action:** Apply reveal animations to:
- FAQ accordion items on faq.html
- Documentation section cards
- Footer content on page load

#### 4.2 Button Hover State Enhancement
**Files:** `styles.css`
**Issue:** Button hover states are functional but could benefit from more distinctive micro-interactions matching Terminal Noir aesthetic.
**Action:** Add subtle glow pulse or border animation to primary CTA buttons on hover.

### Phase 5: Missing Configuration Files

#### 5.1 Create PostCSS Configuration
**Files:** Create `postcss.config.cjs`
**Issue:** CLAUDE.md references PostCSS setup but configuration file doesn't exist in repo.
**Action:** Create configuration per CLAUDE.md specifications with:
- `postcss-import`
- `postcss-nesting`
- `postcss-custom-media`
- `postcss-preset-env`
- `cssnano` (production)

#### 5.2 Create Stylelint Configuration
**Files:** Create `.stylelintrc.cjs`
**Issue:** CLAUDE.md references Stylelint rules but configuration doesn't exist.
**Action:** Create configuration enforcing:
- Max nesting depth: 3
- No ID selectors
- No `!important`
- Color variable usage

---

## Critical Files for Implementation

| File | Priority | Changes |
|------|----------|---------|
| `styles.css` | HIGH | Duplicate removal, naming standardization, hierarchy improvements |
| `tokens.css` | MEDIUM | New file - custom media queries and design tokens |
| `postcss.config.cjs` | MEDIUM | New file - build tooling setup |
| `.stylelintrc.cjs` | LOW | New file - code quality enforcement |
| `index.html` | HIGH | Badge class updates, reveal class additions |
| `faq.html` | MEDIUM | Reveal animations, visual hierarchy |
| `how-it-works.html` | LOW | Section differentiation |
| `getting-started.html` | LOW | Code block consistency |
| `open-source.html` | LOW | Reveal animations |
| `docs/writing-specs.html` | LOW | Code block consistency |
| `docs/workflows.html` | LOW | Code block consistency |

---

## Risks & Mitigations

### Risk 1: Breaking Light Mode
**Probability:** Medium
**Impact:** High
**Mitigation:** Test all CSS changes in both dark and light modes. Use browser devtools to toggle `data-theme="light"` and verify no regressions.

### Risk 2: Animation Performance
**Probability:** Low
**Impact:** Medium
**Mitigation:** Ensure all animations use `transform` and `opacity` only (GPU-accelerated). Respect `prefers-reduced-motion` media query (already implemented at line 410).

### Risk 3: PostCSS Migration Complexity
**Probability:** Medium
**Impact:** Medium
**Mitigation:** Implement PostCSS configuration incrementally. Start with basic setup, then migrate breakpoints one section at a time. Keep original CSS as fallback initially.

### Risk 4: Badge Class Updates Breaking HTML
**Probability:** Medium
**Impact:** Low
**Mitigation:** Search all HTML files for deprecated class names before removing them from CSS. Update HTML first, then clean CSS.

---

## Verification Strategy

### Visual Verification
1. **Page-by-page comparison:** Take screenshots before/after each major change
2. **Dark/Light mode toggle:** Test every modified component in both themes
3. **Responsive testing:** Test at breakpoints 375px, 480px, 640px, 768px, 1024px
4. **Animation testing:** Verify scroll reveals trigger correctly on all pages

### Code Quality Verification
1. **CSS validation:** Run through W3C CSS validator
2. **Lighthouse audit:** Check Performance, Accessibility, Best Practices scores
3. **Stylelint (once configured):** Run `stylelint styles.css` for rule compliance
4. **Cross-browser:** Test in Chrome, Firefox, Safari (minimum)

### Functional Verification
1. **Navigation:** All links work correctly after class changes
2. **Theme toggle:** Dark/light switch persists and applies correctly
3. **FAQ accordion:** Expand/collapse animations work smoothly
4. **Terminal demo:** Animation playback unaffected by CSS changes

### Acceptance Criteria
- [ ] No duplicate CSS rule definitions
- [ ] Consistent naming convention (Terminal Noir only)
- [ ] All pages render correctly in both dark and light modes
- [ ] Lighthouse accessibility score ≥ 90
- [ ] All scroll animations trigger correctly
- [ ] PostCSS config compiles without errors
- [ ] Stylelint passes with no errors
