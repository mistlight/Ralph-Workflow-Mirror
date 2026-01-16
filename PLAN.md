# Design Stabilization Implementation Plan

## Summary

After visual diagnosis of all pages (index.html sections 1-7, getting-started.html, faq.html, docs/writing-specs.html), I've identified the fundamental causes of visual inconsistency between the baseline (Sections 1-3) and post-Section 3 content.

**The good news:** The site has strong bones. The design system is well-defined, typography choices are excellent (Instrument Serif + Space Grotesk + IBM Plex Mono), and the color palette (forest green + warm amber) is cohesive. The issues are primarily about **application consistency**, not design system flaws.

---

## Top 4 Fundamental Causes of Visual Inconsistency

### 1. **Container Width & Reading Lane Inconsistency**
- **Baseline (Sections 1-3):** Content uses `--container-xl` (1280px) with comfortable margins, creating a consistent reading lane
- **Post-Section 3:** Some sections break this pattern with ad-hoc widths, particularly in:
  - Section 05 (Features) - cards feel cramped within container
  - Section 06 (Audience) - inconsistent card grid spacing
  - FAQ items on index.html vs faq.html have different visual density
  - Docs pages have narrower effective content width in places

### 2. **Vertical Rhythm Breakdown**
- **Baseline:** Sections 1-3 have generous, consistent spacing between elements (80-120px section padding, consistent element gaps)
- **Post-Section 3:** Vertical spacing becomes erratic:
  - Section 04 (Install) has tighter spacing than surrounding sections
  - Section 07 (FAQ on index) feels compressed
  - Getting-started.html step sequences have inconsistent inter-step spacing
  - Docs pages have variable spacing between content blocks

### 3. **Component Visual Weight Imbalance**
- **Baseline:** Cards, code blocks, and callouts in Sections 1-3 have balanced visual weight with consistent border-radius, shadows, and padding
- **Post-Section 3:**
  - Feature cards (Section 05) feel lighter/less substantial than comparison cards
  - Audience cards (Section 06) have different hover behavior than baseline cards
  - Code blocks across pages have inconsistent padding and header treatment
  - Troubleshooting boxes, tips boxes, and callouts vary in styling
  - FAQ items on index vs dedicated FAQ page have different component treatment

### 4. **Typography Hierarchy Drift**
- **Baseline:** Clear distinction between section numbers, headings, subheadings, body, and captions
- **Post-Section 3:**
  - Section headings in 04-07 don't have the same visual impact as 01-03
  - Subheadings (h3, h4) compete with section headings in visual weight
  - List item styling varies (bullet styles, spacing, indentation)
  - Code block labels ("bash", "markdown") have inconsistent styling

---

## Implementation Passes

### Pass A — Layout Model Unification

**Goal:** Ensure all pages and sections share the same spatial DNA as Sections 1-3.

**Targets:**
1. Audit and normalize container widths across all sections (index.html 04-07)
2. Standardize section padding to match baseline rhythm (80-100px top/bottom)
3. Fix grid gutters on feature cards (Section 05) and audience cards (Section 06)
4. Ensure subpages (getting-started, faq, docs/*) use consistent page container
5. Normalize content container max-width for reading comfort (~1000px for prose)

**Files affected:**
- `styles.css` - Container/section spacing rules
- `index.html` - Section wrapper classes (if needed)
- `getting-started.html` - Container consistency
- `faq.html` - Container consistency
- `docs/*.html` - Container consistency

**Verification:**
- All sections flow with same visual rhythm when scrolling
- No jarring width changes between sections
- Subpages feel like part of the same site

---

### Pass B — Typography & Rhythm Restoration

**Goal:** Re-establish consistent heading scale, spacing ladder, and text hierarchy.

**Targets:**
1. Ensure section number + heading combo (01, 02, etc.) has same treatment everywhere
2. Normalize h2/h3/h4 sizing and spacing relationships
3. Standardize paragraph margins and line-heights
4. Fix list styling (bullets, spacing, indentation) to be consistent
5. Ensure code block typography (size, padding, headers) is uniform

**Files affected:**
- `styles.css` - Typography scale, heading styles, list styles
- Possibly HTML files if classes need adjustment

**Verification:**
- Headings create clear hierarchy across all pages
- Body text is comfortable to read with consistent rhythm
- Lists and code blocks have predictable, pleasant spacing

---

### Pass C — Component Normalization

**Goal:** Ensure all UI components (cards, callouts, code blocks, FAQ items) share canonical styling.

**Targets:**
1. Unify card styling (feature cards, audience cards, prereq cards) - same shadows, borders, hover states
2. Standardize code block treatment (header bar, padding, border-radius, copy button position)
3. Normalize callout/tip/troubleshooting boxes to single visual pattern
4. Ensure FAQ items have consistent styling on index.html vs faq.html
5. Unify step indicators (numbered circles) across pages
6. Standardize link styling in different contexts

**Files affected:**
- `styles.css` - Card, code-block, callout, FAQ component styles
- HTML files - May need class normalization

**Verification:**
- Cards across all pages feel like the same component
- Code blocks are visually identical everywhere
- Callout-type elements share same visual language

---

### Pass D — Docs-Specific Polish (Optional)

**Goal:** Improve docs page scannability without introducing new visual patterns.

**Targets:**
1. Ensure docs page breadcrumbs have proper visual weight
2. Normalize section intros (01, 02, etc. pattern) on docs pages
3. Verify checklist and tips grid styling matches baseline
4. Ensure example spec blocks have consistent treatment
5. Polish "What's Next" and CTA sections at page bottoms

**Files affected:**
- `styles.css` - Docs-specific component refinements
- `docs/writing-specs.html`
- `docs/overnight-runs.html`
- `docs/workflows.html`

**Verification:**
- Docs pages feel like polished documentation, not "late-stage appended"
- Reading flow is comfortable with clear visual landmarks
- Navigation cues (breadcrumbs, next links) are consistent

---

## Critical Files for Implementation

| File | Role | Pass |
|------|------|------|
| `styles.css` | All styling changes | A, B, C, D |
| `index.html` | Sections 04-07 class/structure fixes | A, B, C |
| `getting-started.html` | Container/component normalization | A, C |
| `faq.html` | Container/FAQ styling | A, C |
| `docs/writing-specs.html` | Docs component styling | C, D |
| `docs/overnight-runs.html` | Docs component styling | C, D |
| `docs/workflows.html` | Docs component styling | C, D |
| `how-it-works.html` | Container consistency | A |
| `open-source.html` | Container consistency | A |

---

## Risks & Mitigations

### Risk 1: Breaking Sections 1-3
**Mitigation:** After each pass, visually verify Sections 1-3 are unchanged or improved. Use browser DevTools to compare before/after. Keep changes scoped to post-Section 3 selectors where possible.

### Risk 2: Cascade effects from global CSS changes
**Mitigation:** Prefer scoped class selectors over element selectors. Test all pages after each pass. Make changes incrementally within each pass.

### Risk 3: Mobile/tablet regression
**Mitigation:** Test responsive breakpoints after each pass. The existing responsive system is solid; maintain existing breakpoint logic.

### Risk 4: Over-engineering
**Mitigation:** Stick to the 4-pass structure. Each pass has clear, limited scope. Avoid scope creep into "nice to have" improvements.

---

## Verification Strategy

After each pass, verify:

1. **Visual parity:** Post-Section 3 pages match baseline quality (screenshot comparison)
2. **Spacing consistency:** No jarring rhythm breaks when scrolling
3. **Component consistency:** Same element types look identical across pages
4. **Responsive behavior:** Mobile and tablet views remain clean
5. **Functionality:**
   - File:// protocol still works
   - Dark mode toggle functions
   - FAQ accordions work
   - Code copy buttons work
   - Skip links and keyboard navigation work
   - Reduced motion preferences respected

---

## Completion Criteria

The task is complete when:

- [x] All post-Section 3 sections feel cohesive with Sections 1-3
- [x] Visual rhythm is consistent across all pages
- [x] Typography hierarchy is clear and predictable
- [x] Components (cards, code blocks, callouts, FAQs) are visually unified
- [x] Docs pages feel like polished documentation, not afterthoughts
- [x] Site reads as a single, intentionally designed product
- [x] No "unstyled" or "appended late" feeling remains
- [x] All functionality preserved (static file compatibility, dark mode, accessibility)

## Implementation Summary

All 4 passes have been completed:

### Pass A - Layout Model Unification
- Added missing `.section-number` styling
- Added `.install-mode-toggle` and mode switch styling
- Fixed `.install-code` to be container (was incorrectly styled as code block)
- Added grid layouts to `.install-grid`, `.features-grid`, `.audience-grid`

### Pass B - Typography & Rhythm Restoration
- Normalized `.audience-card h3` to match `.feature-card h3` sizing
- Added complete `.feature-expand-btn` and `.feature-expand-content` styling
- Added list styling within expandable content

### Pass C - Component Normalization
- Unified card hover states (audience-card now matches feature-card)
- Added comprehensive `.code-block` base component styling
- Added unified `.callout` system with variants (success, warning, info)

### Pass D - Docs-Specific Polish
- Verified breadcrumb styling consistency
- Verified section intro (01, 02, etc.) pattern across all docs pages
- Verified CTA sections have consistent styling
- All component classes used in HTML have corresponding CSS definitions
