# Ralph Workflow Website - Implementation Plan

## Summary

The Ralph Workflow website has a solid existing foundation with a "Forest Editorial" design system (~3,800 lines CSS, ~1,450 lines JS, ~1,550 lines HTML). After thorough exploration of the codebase, current screenshots, design documentation, and the source product (Ralph Workflow CLI), this plan outlines targeted refinements needed to achieve commercial-grade, Dribbble-level polish while maintaining clarity for all three target audiences (software developers, vibe coders, and CLI newcomers).

**Current State Assessment:**
- ✅ Strong design foundation: Forest Editorial theme with distinctive typography (Instrument Serif, Space Grotesk, IBM Plex Mono)
- ✅ Comprehensive content covering all audience segments with audience selector
- ✅ Full static site compatible with Codeberg Pages (no build step required)
- ✅ Accessibility features: skip links, ARIA labels, keyboard navigation, reduced motion support
- ✅ Interactive features: terminal demo, dark/light mode, copy buttons, audience selector
- ⚠️ Logo SVG uses off-palette colors (#CCFF00, #FF00AA, #9D4EDD) not matching Forest Editorial design system
- ⚠️ Installation instructions need verification (current repo URL vs. PROMPT.md specification)
- ⚠️ Some visual refinements needed for pixel-perfect polish

**Approach:** Targeted iteration and refinement, not a rebuild. The existing work is substantial and high-quality. Focus on:
1. Design system consistency (fix off-palette colors)
2. Content alignment with product truth (correct installation commands)
3. Visual polish passes (responsive edge cases, interaction states)
4. Accessibility verification

---

## Implementation Steps

### Phase 1: Critical Content & Branding Fixes

**Step 1.1: Update Logo Assets to Forest Editorial Palette**
- Current `assets/logo-icon.svg` uses legacy "Noir" colors (#CCFF00, #FF00AA, #9D4EDD, #00FFF0)
- Replace with Forest Editorial palette:
  - Gradient: `--color-primary` (#d4a574) → `--color-accent` (#7da58a)
  - Remove off-palette magenta/purple dots
- Update any inline SVG logos in `index.html` to match
- Recreate favicon with Forest Editorial colors
- **Files:** `assets/logo-icon.svg`, `assets/logo.svg`, `index.html` (favicon data URI), `404.html` (favicon)

**Step 1.2: Verify and Correct Installation Instructions**
- PROMPT.md specifies: `git clone ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` and cargo name `ralph-workflow`
- Codeberg shows actual repo: `https://codeberg.org/mistlight/RalphWithReviewer.git` and CLI is `ralph`
- **Resolution needed:** Confirm correct repository URL and cargo package name
- Ensure all installation code blocks use the correct commands:
  ```bash
  git clone https://codeberg.org/mistlight/RalphWithReviewer.git
  cd RalphWithReviewer
  cargo install --path .
  ```
- Update prerequisite requirements (Rust/cargo, Claude Code/Codex)
- **Files:** `index.html` (install section), potentially all code-block content

**Step 1.3: Verify PROMPT.md Feature Section Accuracy**
- Ensure code examples in the website match actual PROMPT.md format from the Ralph Workflow repo
- Update if any syntax or structure has changed
- **Files:** `index.html` (PROMPT.md section)

### Phase 2: Design System Consistency Audit

**Step 2.1: Search and Replace Off-Palette Colors**
- Audit `index.html` for any hardcoded hex colors not in the CSS variable system
- Check for legacy Noir colors (#00D4FF electric cyan, #CCFF00 neon lime, etc.)
- Replace all with CSS variables
- Verify workflow diagram SVG (if any) uses design system colors
- **Files:** `index.html`, `styles.css`

**Step 2.2: Inline Style Extraction Audit**
- PLAN.md notes ~300+ lines of inline styles were identified
- Verify these have been extracted or extract remaining inline styles
- Focus on comparison section, glossary, FAQ areas
- Convert to proper CSS component classes in `styles.css`
- **Files:** `index.html`, `styles.css`

**Step 2.3: Typography Consistency Check**
- Verify all text uses proper CSS variable references (`--font-display`, `--font-body`, `--font-mono`)
- Check for any hardcoded font-family declarations
- Verify type hierarchy creates clear visual impact
- **Files:** `styles.css`, `index.html`

### Phase 3: Visual Polish Pass

**Step 3.1: Hero Section Enhancement**
- Verify page load animation orchestration is smooth and impressive
- Check staggered entrance timing (badges → title → description → CTA)
- Test terminal demo animation quality
- Ensure parallax/blob effects are subtle, not distracting
- **Files:** `styles.css`, `script.js`

**Step 3.2: Interactive States Completeness Audit**
- Audit all buttons for complete states: default, hover, active/pressed, focus-visible, disabled, loading
- Audit all cards for hover lift and shadow transitions
- Verify magnetic effects are enabled only for pointer devices
- Check copy button success/error feedback states
- **Files:** `styles.css`, `script.js`

**Step 3.3: Section-by-Section Visual Review**
For each section, verify:
- Typography hierarchy (passes "3-second scan test")
- Spacing consistency (8px grid alignment)
- Card/component styling consistency
- Scroll reveal animations trigger correctly

Sections to audit:
1. Hero (terminal demo, CTAs)
2. "What is Ralph" (workflow explanation)
3. Before/After Comparison
4. How It Works (step cards)
5. PROMPT.md Feature (code example)
6. Comparison Table
7. Interactive Demo (tabs, terminal output, generated code)
8. Install Section (tabs, code blocks, copy buttons, simple/advanced toggle)
9. Features Grid (expandable cards)
10. Audience Personas (three cards)
11. Glossary (for CLI newcomers)
12. FAQ (accordion)
13. Footer

**Files:** `index.html`, `styles.css`, `script.js`

### Phase 4: Responsive & Mobile Polish

**Step 4.1: Mobile Navigation Testing**
- Verify hamburger menu animation is smooth
- Ensure menu items meet 44px touch target minimum
- Test dark mode toggle accessibility on mobile
- Check menu close behavior (outside click, escape key, link click)
- **Files:** `styles.css`, `script.js`

**Step 4.2: Responsive Breakpoint Verification**
- Test at key viewport widths:
  - Desktop: 1920px, 1440px, 1280px
  - Tablet: 768px
  - Mobile: 375px (iPhone SE), 414px (iPhone 12/13)
- Verify all layouts are intentional (not just "fits")
- Check terminal demo usability on small screens
- Verify audience selector wraps gracefully
- **Files:** `styles.css`

**Step 4.3: Touch Device Optimization**
- Disable magnetic button effects on touch devices (verify `pointer: fine` check)
- Reduce parallax complexity on mobile for performance
- Ensure all interactive elements have sufficient touch targets
- **Files:** `script.js`, `styles.css`

### Phase 5: Accessibility Verification

**Step 5.1: Keyboard Navigation Testing**
- Tab through entire site and verify logical focus order
- Ensure all interactive elements are keyboard accessible
- Verify focus states are visible and beautiful
- Test FAQ accordion keyboard operation
- Test demo tab panel keyboard navigation
- **Files:** `index.html`, `script.js`

**Step 5.2: Screen Reader Testing**
- Verify all ARIA labels are correct and complete
- Ensure dynamic content updates (terminal demo, copy button feedback) are announced
- Check for any missing alt text on decorative vs. meaningful elements
- **Files:** `index.html`

**Step 5.3: Color Contrast Verification**
- Verify all text meets WCAG AA contrast requirements
- Check both dark mode and light mode
- Pay attention to:
  - `--color-text-muted` on background
  - `--color-text-dim` (should be decorative only)
  - Disabled button states
- **Files:** `styles.css`

**Step 5.4: Reduced Motion Verification**
- Test site with `prefers-reduced-motion: reduce` enabled
- Verify all animations are disabled/minimized
- Confirm smooth scroll becomes instant
- Ensure site is fully usable without motion
- **Files:** `styles.css`, `script.js`

### Phase 6: Performance Audit

**Step 6.1: Animation Performance**
- Verify animations use GPU-accelerated properties (transform, opacity only)
- Check for any animations using width/height/top/left that cause layout thrashing
- Ensure parallax scroll handlers use requestAnimationFrame throttling
- **Files:** `styles.css`, `script.js`

**Step 6.2: Asset Loading**
- Verify font preconnect/preload is optimal
- Check if critical CSS could be inlined
- Ensure no render-blocking resources
- **Files:** `index.html`

**Step 6.3: File Size Check**
- Review final CSS size (~3,800 lines = ~85KB unminified)
- Review final JS size (~1,450 lines = ~55KB unminified)
- Consider minification for production (can be done locally, committed minified)
- **Files:** `styles.css`, `script.js`

### Phase 7: Final Visual Verification (Playwright)

**Step 7.1: Desktop Screenshots**
- Navigate to local site (serve via `python -m http.server` or `npx serve .`)
- Take full-page screenshot at 1920px width (dark mode)
- Toggle to light mode and take full-page screenshot
- Take targeted screenshots of key sections
- **Verification:** Look for alignment issues, spacing inconsistencies, off-palette colors

**Step 7.2: Mobile Screenshots**
- Resize to 375px width
- Take full-page mobile screenshot
- Open hamburger menu and take screenshot
- Verify touch targets appear adequate

**Step 7.3: Tablet Screenshots**
- Resize to 768px width
- Take full-page tablet screenshot
- Verify intermediate layouts

**Step 7.4: Interactive Feature Testing**
- Test terminal demo play/pause/restart/speed controls
- Test copy-to-clipboard buttons
- Test FAQ accordion expand/collapse
- Test feature card expansion
- Test audience selector content filtering
- Test dark/light mode toggle
- Test simple/advanced install toggle

**Step 7.5: Premium UI Checklist (per CLAUDE.md)**
- [ ] Hierarchy scan: Can I tell what matters in 3 seconds?
- [ ] Spacing scan: Consistent 8px grid across sections?
- [ ] State scan: All interactive states implemented?
- [ ] Contrast scan: No hard-to-read text?
- [ ] Responsive scan: Intentional at all sizes?
- [ ] Edge cases: Long text, empty states handled?
- [ ] Polish pass: Alignments, radii, shadows consistent?

---

## Critical Files for Implementation

| File | Size | Purpose | Priority |
|------|------|---------|----------|
| `index.html` | ~1,550 lines | Main website structure, content, semantic markup | Critical |
| `styles.css` | ~3,800 lines | Complete design system, components, animations, responsive | Critical |
| `script.js` | ~1,450 lines | All interactions, terminal demo, navigation, effects | Critical |
| `assets/logo-icon.svg` | ~20 lines | Logo mark - needs palette update | High |
| `assets/logo.svg` | ~40 lines | Full logo - needs palette update | High |
| `404.html` | ~550 lines | Error page - consistent with main theme | Medium |

### Key Areas in Each File

**index.html:**
- Lines ~1-50: Meta tags, OG tags, favicon (check favicon colors)
- Lines ~100-300: Hero section with terminal demo
- Lines ~400-600: What is Ralph section
- Lines ~700-900: How it works, PROMPT.md feature
- Lines ~1000-1200: Install section (verify URLs)
- Lines ~1200-1500: Features, audience, FAQ, footer

**styles.css:**
- Lines 1-250: CSS custom properties (design tokens) - verify completeness
- Lines 250-500: Base styles, typography
- Lines 500-1000: Navigation, buttons
- Lines 1000-1600: Hero section, terminal
- Lines 1600-2500: Content sections
- Lines 2500-3800: Components, utilities, animations, responsive

**script.js:**
- Lines 1-100: Magnetic effects, parallax
- Lines 100-250: Navigation, scroll effects
- Lines 250-500: Terminal demo controls
- Lines 500-750: Mobile nav, install tabs
- Lines 750-1000: Copy buttons, smooth scroll
- Lines 1000-1456: Demo simulation, audience selector, reduced motion

---

## Risks & Mitigations

### Risk 1: Repository URL Mismatch
**Likelihood:** High | **Impact:** High
**Issue:** PROMPT.md specifies `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` but actual repo appears to be `RalphWithReviewer`
**Mitigation:**
- Verify correct URL with user before updating
- If URL changed, update all instances in install section

### Risk 2: Breaking Existing Functionality During Refinement
**Likelihood:** Medium | **Impact:** Medium
**Mitigation:**
- Make incremental changes, test after each
- Keep backup of working state
- Use Playwright screenshots to verify visual consistency

### Risk 3: Logo/Branding Color Changes Affect OG Image
**Likelihood:** Medium | **Impact:** Low
**Mitigation:**
- After updating logo colors, regenerate `og-image.png`
- Use `og-generator.js` or `og-image.html` to create new version

### Risk 4: Animation Performance Issues on Mobile
**Likelihood:** Low | **Impact:** Medium
**Mitigation:**
- Test on throttled CPU in DevTools
- Verify touch device detection disables complex effects
- Use `will-change` sparingly and appropriately

### Risk 5: Accessibility Regression During Visual Changes
**Likelihood:** Low | **Impact:** High
**Mitigation:**
- Run accessibility audit before and after changes
- Maintain focus states and ARIA labels
- Test with keyboard-only navigation

---

## Verification Strategy

### Acceptance Check 1: Visual Quality at Dribbble Level
**How to verify:**
1. Serve site locally and take Playwright screenshots
2. Visual inspection for:
   - Premium, intentional aesthetic (not "template-y")
   - Consistent visual language throughout
   - Distinctive typography (Instrument Serif stands out)
   - Atmospheric depth (gradient orbs, noise texture)
   - Smooth, meaningful animations
3. Compare to Dribbble shots of similar products

### Acceptance Check 2: Clear for All Audiences
**How to verify:**
1. Read through site as a software developer:
   - Can I quickly understand CLI options (-Q, -S, -T, -L)?
   - Are installation steps clear and complete?
   - Is PROMPT.md format explained?
2. Read through as a vibe coder:
   - Is the value proposition ("let AI cook overnight") clear?
   - Is setup approachable?
3. Read through as CLI newcomer:
   - Is glossary helpful?
   - Are terminal concepts explained?
4. User should NOT leave with unanswered questions about what Ralph does

### Acceptance Check 3: No Design Flaws
**How to verify:**
- Run CLAUDE.md Premium UI Checklist
- Check alignment (all on 8px grid)
- Check spacing consistency between similar elements
- Check interaction states are complete
- Check contrast passes WCAG AA
- Check responsive layouts are intentional

### Acceptance Check 4: Static Site Compatibility
**How to verify:**
1. Open `index.html` directly in browser (file:// protocol)
2. Navigate through all sections
3. Test all interactive features
4. Verify no console errors about missing resources
5. Confirm no build step required

### Acceptance Check 5: Installation Instructions Correct
**How to verify:**
1. Follow installation instructions exactly as written
2. Verify repository clones successfully
3. Verify cargo install works
4. Verify ralph CLI is available
5. Compare with actual Ralph Workflow README

### Playwright Test Sequence
```
1. python -m http.server 8000 (in project directory)
2. mcp__playwright__browser_navigate to http://localhost:8000
3. mcp__playwright__browser_snapshot (accessibility tree)
4. mcp__playwright__browser_take_screenshot (desktop-dark-full.png, fullPage=true)
5. Click dark mode toggle
6. mcp__playwright__browser_take_screenshot (desktop-light-full.png, fullPage=true)
7. mcp__playwright__browser_resize to 768x1024
8. mcp__playwright__browser_take_screenshot (tablet.png)
9. mcp__playwright__browser_resize to 375x812
10. mcp__playwright__browser_take_screenshot (mobile.png)
11. Click hamburger menu
12. mcp__playwright__browser_take_screenshot (mobile-menu.png)
13. Click terminal "Run Full Demo" button
14. Wait for demo completion
15. Test copy button on install code block
16. Click through FAQ accordion items
17. Test audience selector buttons
```

---

## Design Notes

### Current Aesthetic: Forest Editorial
- **Deep forest green** (#0d1f18 to #1a3a2f) creates sophisticated, premium feel
- **Warm amber** (#d4a574) provides distinctive accent that's not typical "tech blue"
- **Sage green secondary** (#7da58a) adds natural, calming complement
- **Instrument Serif** headings create editorial gravitas
- **Space Grotesk** body maintains excellent readability
- **IBM Plex Mono** code has humanist feel for terminal demos

### Visual Differentiators
1. Terminal visualization as hero centerpiece (immediate product context)
2. Before/After comparison showing transformation
3. Audience-adaptive content with selector
4. Dark mode default with rich atmospheric depth
5. Warm amber accent instead of typical tech-blue

### What Must NOT Change
- Forest Editorial color palette (signature identity)
- Static-only implementation (Codeberg Pages compatible)
- AGPL-3.0 license notice in footer
- Three-audience content strategy
- Terminal demo as hero focus

### What Should Be Enhanced
- Logo assets (update to Forest Editorial palette)
- Installation URL accuracy
- Any remaining off-palette colors
- Visual polish edge cases

---

## Success Criteria

Implementation is complete when:

1. **Brand Consistency:** All logo/branding uses Forest Editorial palette (#d4a574, #7da58a, #1a3a2f)
2. **Content Accuracy:** Installation instructions verified against actual repository
3. **Visual Quality:** Would be considered for Dribbble showcase
4. **Audience Clarity:** Users from all three audiences understand Ralph without confusion
5. **Technical Quality:** No console errors, smooth animations, responsive at all sizes
6. **Accessibility:** WCAG AA compliance, keyboard navigation works, reduced motion respected
7. **Cross-Browser:** Works in Chrome, Firefox, Safari, Edge
8. **Static Compatibility:** Works when opened directly as file:// without server
9. **Manual Review:** Passes CLAUDE.md Premium UI Checklist

---

## Questions for User Clarification

Before implementation, clarification needed on:

1. **Repository URL:** The PROMPT.md specifies `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` but Codeberg shows `https://codeberg.org/mistlight/RalphWithReviewer.git`. Which is correct for the website?

2. **Cargo Package Name:** PROMPT.md says `ralph-workflow` but the CLI appears to be just `ralph`. Which should be documented?

3. **Logo Design Direction:** The current logo uses a stylized "R" with agent dots in off-palette colors. Should we:
   - A) Keep the design concept but update colors to Forest Editorial (amber/sage gradient)
   - B) Create a simpler logo mark that better fits the refined aesthetic
   - C) Use text-only logo "Ralph" in Instrument Serif

---

**Plan Status:** Ready for User Approval
**Estimated Files Modified:** 5-7
**Major Changes:** Logo palette, installation URLs, visual polish
**Approach:** Refinement iteration, not rebuild
