# Ralph Workflow Website — Implementation Plan

## Summary

The Ralph Workflow website is a **mature, well-architected static site** (~6,875 lines of code across 3 main files) with a distinctive "Forest Editorial" design system. The codebase is essentially complete with comprehensive content across 11 major sections, a fully-defined design token system (400+ CSS custom properties), interactive terminal demos, accessibility features, and responsive layouts.

**The site requires targeted refinements, not a rebuild.** Four specific issues need resolution:

1. **Logo SVG Files Off-Palette** — Both `assets/logo.svg` and `assets/logo-icon.svg` use legacy "Noir" colors (#CCFF00 lime, #FF00AA magenta, #9D4EDD purple, #00FFF0 cyan) instead of Forest Editorial colors
2. **OG Image Generation Off-Palette** — `og-generator.js` and `og-image.html` use the same Noir colors; the generated `og-image.png` is inconsistent with the site
3. **Repository URL Discrepancy** — Installation uses `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` but Codeberg shows `RalphWithReviewer` as the actual repo name
4. **CSS Class Naming Legacy** — `faq-category--magenta` and `faq-summary--magenta` class names reference the old palette (they function correctly but naming is inconsistent)

**Approach:** Fix the 4 targeted issues, regenerate the OG image, and perform a final visual verification pass.

---

## Implementation Steps

### Phase 1: Clarify Repository Information (Blocking Question)

**Step 1.1: Resolve repository URL and binary name discrepancy**

Before any code changes, the user must confirm:

| Question | Option A (Per PROMPT.md) | Option B (Per Codeberg) |
|----------|--------------------------|-------------------------|
| Clone URL | `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` | `https://codeberg.org/mistlight/RalphWithReviewer.git` |
| Binary name | `ralph-workflow` | `ralph` |
| Config path | Not specified | `~/.config/ralph-workflow.toml` |

**Why this matters:** Incorrect installation instructions will break user onboarding—the core purpose of this site.

**Action:** Ask user to confirm which values are correct before proceeding with Phase 2.

---

### Phase 2: Logo SVG Color Correction

**Step 2.1: Update `assets/logo-icon.svg` to Forest Editorial palette**

Replace the following colors:

| Current (Noir) | Replace With (Forest Editorial) | Element |
|----------------|----------------------------------|---------|
| `#CCFF00` (lime) | `#d4a574` (amber) | Gradient start, connection lines |
| `#00FFF0` (cyan) | `#7da58a` (sage) | Gradient end, bottom agent dot |
| `#FF00AA` (magenta) | `#d4a574` (amber) | Top agent dot |
| `#9D4EDD` (purple) | `#5d9e73` (emerald) | Middle agent dot |

The file is 20 lines. Changes are surgical—only `fill`, `stroke`, and `stop-color` attributes.

**File:** `assets/logo-icon.svg`

---

**Step 2.2: Update `assets/logo.svg` to Forest Editorial palette**

Same color replacements as Step 2.1, plus:
- Change background rect fill from `#000000` to `#0d1f18` (deep forest)
- Update outer frame stroke from Noir gradient to Forest Editorial gradient
- Preserve all `<animate>` elements (pulsing dots) — only change `fill` colors, not animation parameters

**File:** `assets/logo.svg` (40 lines)

---

### Phase 3: OG Image Generator Color Correction

**Step 3.1: Update `og-generator.js` color palette**

Replace the color object (lines 14-24):

```javascript
// BEFORE (Noir palette)
const colors = {
    background: '#000000',
    lime: '#CCFF00',
    cyan: '#00FFF0',
    pink: '#FF006E',
    amber: '#FF9500',
    violet: '#9D00FF',
    white: '#FFFFFF',
    gray: '#1A1A1A',
    border: '#2A2A2A'
};

// AFTER (Forest Editorial palette)
const colors = {
    background: '#0d1f18',      // Deep forest
    amber: '#d4a574',           // Primary accent
    sage: '#7da58a',            // Secondary accent
    emerald: '#5d9e73',         // Tertiary
    rose: '#c97878',            // Accent (error/warning aesthetic)
    white: '#f5f2ed',           // Warm white
    gray: '#1a3a2f',            // Card background
    border: '#234f3d'           // Border color
};
```

Update all color references in the generator:
- Gradient glows: lime → amber, cyan → sage, pink → emerald
- Border frames: lime → amber, cyan → sage
- Corner accents: Use amber, sage, emerald, rose
- Text colors: RALPH title → amber, subtitle → sage

**File:** `og-generator.js` (185 lines)

---

**Step 3.2: Update `og-image.html` hardcoded colors**

Replace all Noir hex values with Forest Editorial equivalents:

| Line Range | Current | Replace With |
|------------|---------|--------------|
| 19, 27-28 | `#000000` (background) | `#0d1f18` |
| 42 | `rgba(204, 255, 0, ...)` (lime glow) | `rgba(212, 165, 116, ...)` (amber) |
| 50 | `rgba(0, 255, 240, ...)` (cyan glow) | `rgba(125, 165, 138, ...)` (sage) |
| 58 | `rgba(255, 0, 110, ...)` (pink glow) | `rgba(93, 158, 115, ...)` (emerald) |
| 78 | `#CCFF00` (frame border) | `#d4a574` |
| 86 | `#00FFF0` (inner frame) | `#7da58a` |
| 94 | `#CCFF00` (corner) | `#d4a574` |
| 106-108 | `#FF006E`, `#00FFF0`, `#FF9500` | `#c97878`, `#7da58a`, `#5d9e73` |
| 137-140 | Logo text color | `#d4a574` |
| 156 | Subtitle color | `#7da58a` |
| 175, 200 | `#9D00FF` (violet diagonal) | `#c97878` (rose) |
| 214-236 | Dot colors | amber, sage, emerald, rose |
| 245 | Scanline color | `rgba(212, 165, 116, 0.02)` |

**File:** `og-image.html` (304 lines)

---

**Step 3.3: Regenerate `og-image.png`**

After updating the colors, regenerate the OG image using one of:

Option A (Using og-generator.js):
```bash
cd /Users/mistlight/Projects/Ralph-Pages
npm install canvas  # If not already installed
node og-generator.js
```

Option B (Using og-image.html with Playwright):
```bash
npx playwright screenshot og-image.html og-image.png --viewport-size=1200,630
```

**File:** `og-image.png` (regenerated)

---

### Phase 4: Installation Instructions Update (Conditional)

**Step 4.1: Update git clone URL if user confirms change needed**

If the repository URL differs from what's currently in the code:
- Update line 1119 in `index.html`: git clone command
- Update line 1120: directory name in `cd` command
- Update line 1124: package name note if different
- Update footer links (lines 1511-1539) if repository path changed

**File:** `index.html` (targeted lines only)

---

**Step 4.2: Verify all CLI examples match actual Ralph Workflow behavior**

Cross-reference with actual Ralph Workflow README:
- Confirm `-Q`, `-S`, `-T`, `-L` preset flags are accurate
- Confirm `-D` and `-R` iteration flags exist
- Verify config file path (`~/.config/ralph-workflow.toml`)
- Check that `ralph --version` or `ralph-workflow --version` is correct

**File:** `index.html` (CLI examples throughout)

---

### Phase 5: CSS Class Naming Cleanup (Optional Polish)

**Step 5.1: Rename `--magenta` classes to semantic names**

Find and replace in both files:
- `faq-category--magenta` → `faq-category--developer`
- `faq-summary--magenta` → `faq-summary--developer`

These classes already use the correct Forest Editorial amber color. Only the names are inconsistent with the current palette naming convention.

**Files:** `styles.css` (lines 3689, 3739), `index.html` (lines 1452, 1455, 1465)

---

### Phase 6: Visual Verification Pass

**Step 6.1: Full-site visual audit**

1. Serve site locally: `python -m http.server 8000`
2. Take Playwright screenshots at:
   - 1920px (desktop)
   - 1440px (laptop)
   - 768px (tablet)
   - 375px (mobile)
3. Verify:
   - No lime (#CCFF00) visible anywhere
   - No cyan (#00FFF0) visible anywhere
   - No magenta (#FF00AA) visible anywhere
   - Logo uses amber/sage/emerald palette
   - All sections maintain Forest Editorial aesthetic

---

**Step 6.2: Interactive feature testing**

Test all JavaScript functionality:
- [ ] Terminal demo: play/pause/restart/speed controls
- [ ] Copy-to-clipboard: all code blocks show success feedback
- [ ] FAQ accordion: expand/collapse works
- [ ] Audience selector: content filtering works
- [ ] Install mode toggle: Simple/Advanced switches content
- [ ] Mobile hamburger menu: opens/closes correctly
- [ ] Dark/light mode toggle: theme switches correctly
- [ ] Scroll behavior: nav background changes on scroll

---

**Step 6.3: Static file compatibility test**

Open `index.html` directly via `file://` protocol:
- [ ] All sections render correctly
- [ ] Navigation links work (hash-based)
- [ ] No console errors
- [ ] Fonts load (may fallback to system fonts)
- [ ] All interactive features function

---

**Step 6.4: Accessibility verification**

- [ ] Tab through entire site: logical focus order
- [ ] Focus states visible: amber outline on all focusable elements
- [ ] Skip link works: pressing Tab on page load shows skip link
- [ ] Reduced motion: test with `prefers-reduced-motion: reduce` enabled
- [ ] Color contrast: verify text readability in both modes

---

**Step 6.5: Off-palette color audit**

Run grep to confirm no legacy colors remain in deliverable files:

```bash
grep -r "#CCFF00\|#00FFF0\|#FF00AA\|#9D4EDD\|#FF006E\|#9D00FF" \
  --include="*.svg" --include="*.html" --include="*.js" \
  --exclude-dir=".git" --exclude-dir="node_modules" .
```

**Expected result:** Zero matches (or only in the plan file itself).

---

## Critical Files for Implementation

| File | Lines | Change Type | Priority |
|------|-------|-------------|----------|
| `assets/logo-icon.svg` | 20 | Color replacement (5 values) | **HIGH** |
| `assets/logo.svg` | 40 | Color + background replacement | **HIGH** |
| `og-generator.js` | 185 | Color palette object + all color refs | **HIGH** |
| `og-image.html` | 304 | ~25 hardcoded hex values | **HIGH** |
| `og-image.png` | N/A | Regenerate after above fixes | **HIGH** |
| `index.html` | 1550 | Installation URLs (conditional) | **MEDIUM** |
| `styles.css` | 3869 | Class rename (2 declarations) | **LOW** |

**Files that need NO changes:**
- `script.js` — No off-palette colors
- `404.html` — Already uses Forest Editorial palette correctly
- `CLAUDE.md`, `PLAN.md`, `PROMPT.md` — Documentation files

---

## Risks & Mitigations

### Risk 1: Repository URL Uncertainty (HIGH IMPACT)
**Issue:** Installation instructions may be wrong if URL differs from what's documented.
**Mitigation:** Explicitly ask user to confirm the correct clone URL before modifying any installation code. Do not guess.

### Risk 2: Logo Animation Breaks After Color Update (LOW IMPACT)
**Issue:** SVG animations on agent dots could stop working if `<animate>` elements are accidentally modified.
**Mitigation:** Only change `fill` and `stroke` color attributes. Do not touch `<animate>` elements or their timing parameters. Test animation in browser after changes.

### Risk 3: OG Image Generation Fails (LOW IMPACT)
**Issue:** The `canvas` npm package may not be installed, preventing regeneration.
**Mitigation:** Use the HTML template approach with Playwright instead. Both methods produce equivalent results.

### Risk 4: CSS Class Rename Breaks Styling (VERY LOW IMPACT)
**Issue:** If class names are renamed in CSS but not HTML (or vice versa), FAQ styling breaks.
**Mitigation:** Use find-and-replace across both files simultaneously in a single commit. Verify FAQ section renders correctly after changes.

### Risk 5: Social Preview Cache (LOW IMPACT)
**Issue:** Social platforms may cache the old OG image for hours/days after updating.
**Mitigation:** Add a cache-busting query parameter to the OG image URL in meta tags if needed: `og-image.png?v=2`. This is optional.

---

## Verification Strategy

### Acceptance Check 1: Brand Consistency
**Method:** Visual inspection of logo and OG image
**Pass Criteria:**
- Logo uses only amber (#d4a574), sage (#7da58a), emerald (#5d9e73), and forest background (#0d1f18)
- No lime, cyan, magenta, or purple visible
- Logo animation (pulsing dots) still works

### Acceptance Check 2: Zero Off-Palette Colors
**Method:** Grep search across deliverable files
**Command:**
```bash
grep -rn "#CCFF00\|#00FFF0\|#FF00AA\|#9D4EDD\|#FF006E\|#9D00FF" \
  --include="*.svg" --include="*.html" --include="*.js" \
  --exclude-dir=".git" --exclude-dir="node_modules" \
  --exclude="implementation-plan.md" .
```
**Pass Criteria:** Zero matches

### Acceptance Check 3: Installation Instructions Accuracy
**Method:** Follow installation instructions exactly as written
**Pass Criteria:**
1. `git clone` succeeds with documented URL
2. `cd` into correct directory name
3. `cargo install --path .` completes without error
4. Binary runs with documented command name

### Acceptance Check 4: Static Site Compatibility
**Method:** Open `index.html` directly via file:// protocol
**Pass Criteria:**
- All content renders
- Navigation works (hash-based links)
- Interactive features function
- No JavaScript console errors

### Acceptance Check 5: Visual Quality (Dribbble-Level)
**Method:** Full-page screenshots at 4 viewport sizes
**Pass Criteria:**
- Consistent Forest Editorial aesthetic throughout
- Typography hierarchy clear
- Spacing rhythm consistent
- All interactive states polished
- Responsive layouts intentional (not just "fits")

### Acceptance Check 6: Accessibility Basics
**Method:** Manual testing with keyboard
**Pass Criteria:**
- Tab navigation reaches all interactive elements
- Focus states visible with amber outline
- Skip link appears on first Tab press
- Mobile nav accessible via keyboard

---

## Questions Requiring User Clarification

Before implementation can proceed, please confirm:

**1. Repository Clone URL**
Which is the correct clone URL?
- A: `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` (currently in code)
- B: `https://codeberg.org/mistlight/RalphWithReviewer.git` (from Codeberg)
- C: Something else

**2. Binary Name**
What command should users run after installation?
- A: `ralph`
- B: `ralph-workflow`
- C: Both work (alias exists)

**3. Configuration File Path**
Where does Ralph Workflow look for its config file?
- A: `~/.config/ralph-workflow.toml` (per Codeberg README)
- B: `~/.ralph/config.toml`
- C: Something else

---

## Success Criteria

Implementation is complete when ALL of the following are true:

1. **Brand Consistency:** All logo/branding uses Forest Editorial palette only
2. **Zero Off-Palette Colors:** Grep search returns no matches for Noir colors
3. **Installation Accuracy:** Clone URL, cargo command, and binary name verified correct
4. **Visual Quality:** Site passes visual inspection at all breakpoints
5. **Static Compatibility:** Works when opened directly as `file://`
6. **Accessibility Maintained:** Keyboard navigation and focus states functional
7. **OG Image Updated:** Social preview shows Forest Editorial colors

---

## Estimated Scope

| Phase | Files Changed | Lines Modified |
|-------|--------------|----------------|
| Phase 2: Logo SVGs | 2 | ~30 |
| Phase 3: OG Image | 3 | ~100 |
| Phase 4: Installation (if needed) | 1 | ~20 |
| Phase 5: CSS Naming (optional) | 2 | ~10 |
| **Total** | **5-8 files** | **~160 lines** |

This is a targeted refinement—approximately 2-3% of the codebase needs changes.

---

**Plan Status:** Ready for User Approval
**Blocking Question:** Repository URL and binary name confirmation required before Phase 4
