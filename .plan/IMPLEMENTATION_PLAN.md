# Ralph Workflow Website - Implementation Plan

## Summary

The Ralph Workflow website is a **substantial, professionally-designed single-page marketing site** that already exists with ~3,500 lines of CSS, ~1,450 lines of JS, and ~1,550 lines of HTML implementing a distinctive "Forest Editorial" design system. After thorough exploration, the site requires **targeted refinement and polish** rather than rebuilding. The core issues are: (1) SVG workflow diagram uses legacy colors (#00D4FF, #A78BFA) instead of Forest Editorial palette, (2) heavy inline styles in ~100+ locations that should be CSS classes, (3) installation instructions need verification against the correct Codeberg SSH URL, and (4) visual polish needs verification via Playwright screenshots at all breakpoints. The site has excellent foundations—editorial typography (Instrument Serif, Space Grotesk, IBM Plex Mono), cohesive color palette (forest green + warm amber), audience selector functionality, terminal demo animation, dark/light mode toggle, and strong accessibility features—but needs consistency pass to achieve Dribbble-showcase quality.

---

## Implementation Steps

### Phase 1: Design System Consolidation

**Step 1.1: Fix Workflow Diagram SVG Colors (index.html lines ~383-447)**
- Replace all legacy colors in the SVG diagram with Forest Editorial palette:
  - `#00D4FF` (cyan) → `#d4a574` (warm amber primary)
  - `#A78BFA` (violet) → `#7da58a` (sage accent)
  - `#8B5CF6` (dark violet) → `#5d8c6c` (dark sage)
  - `#34D399` (emerald) → `#5d9e73` (forest emerald)
  - `#111` (background) → `#1a3a2f` (card background)
  - `#71717A` (gray text) → `#8a8478` (dim text)
  - `#F4F4F5` (white text) → `#f5f2ed` (primary text)
  - `rgba(0, 212, 255, ...)` gradient → `rgba(212, 165, 116, ...)` amber gradient
- Update the radialGradient definition to use amber instead of cyan
- Test in both dark and light modes

**Step 1.2: Extract Quick Benefits Inline Styles (index.html lines ~756-788)**
- Create CSS classes in `styles.css`:
  - `.quick-benefits-grid` for the grid container
  - `.quick-benefit-card` for each benefit card
  - `.quick-benefit-header` for the icon + title wrapper
  - `.quick-benefit-title` for h3 with colored text
  - `.quick-benefit-icon` for SVG icons
- Replace inline styles with class references in HTML
- Ensure styles work in both dark and light modes

**Step 1.3: Extract Install Info Inline Styles (index.html lines ~1056-1099)**
- Create CSS classes for audience-specific boxes:
  - `.install-audience-box` base class
  - `.install-newcomer-help` for the newcomer tip box
  - `.install-quick-start` for quick start lists
  - `.install-time-saved` for time savings callout
- Replace ~40 lines of inline styles with semantic classes
- Maintain visual appearance while enabling theming

**Step 1.4: Extract FAQ & Troubleshooting Inline Styles**
- Extract inline styles from FAQ section details/summary elements
- Extract troubleshooting dropdown styles (lines ~1130-1147)
- Create reusable `.troubleshooting-details` and `.troubleshooting-content` classes

---

### Phase 2: Content & Accuracy Verification

**Step 2.1: Verify Installation Instructions**
- Confirm SSH clone URL: `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`
- Confirm cargo package name is `ralph-workflow`
- Verify command `ralph` vs `ralph-workflow` both work
- Update any outdated references

**Step 2.2: Verify External Links**
- Test Codeberg repository links
- Test rustup.rs links
- Test crates.io links (if present)
- Verify all anchor links work correctly

**Step 2.3: Verify Audience Content Toggling**
- Test audience selector localStorage persistence
- Ensure all `audience-developer-content`, `audience-vibe-coder-content`, `audience-newcomer-content` classes show/hide correctly
- Verify nav glossary link visibility for newcomers

---

### Phase 3: Visual Polish & Verification

**Step 3.1: Desktop Dark Mode Verification (1920px, 1440px)**
- Take full-page Playwright screenshots
- Verify Forest Editorial aesthetic is cohesive
- Check typography hierarchy (Instrument Serif headings)
- Verify color consistency after SVG fix
- Check terminal demo animation plays correctly

**Step 3.2: Desktop Light Mode Verification**
- Toggle to light mode via dark mode toggle
- Take screenshots at same widths
- Verify text contrast meets WCAG AA
- Ensure code blocks remain readable
- Check cards and surfaces have proper contrast

**Step 3.3: Tablet Verification (768px)**
- Resize browser and take screenshot
- Verify grid layouts collapse gracefully
- Check navigation adapts correctly
- Ensure feature cards stack properly
- Verify terminal demo is usable at this size

**Step 3.4: Mobile Verification (375px, 414px)**
- Take screenshots at common mobile widths
- Verify hamburger menu appears and functions
- Check touch targets are 44px minimum
- Ensure no horizontal scrolling
- Test mobile menu keyboard accessibility

---

### Phase 4: Interaction State Audit

**Step 4.1: Button State Completeness**
- Verify all button variants have: default, hover, active, focus-visible
- Check `.btn-primary`, `.btn-secondary` states
- Verify `.copy-btn` shows "Copied!" state
- Check `.terminal-control-btn` hover/active states
- Verify `.audience-option` pressed states

**Step 4.2: Card & Interactive Element States**
- Audit `.feature-card` hover transitions
- Check `.audience-card` magnetic effect
- Verify `.comparison-card` states
- Check `.glossary-term` expand/collapse
- Ensure all cards have consistent lift + shadow on hover

**Step 4.3: Focus Ring Enhancement**
- Verify focus-visible states are visible and styled
- Consider adding amber glow using `--shadow-glow-sm`
- Ensure focus rings work in both dark and light modes
- Test tab navigation through all interactive elements

---

### Phase 5: Accessibility & Performance Final Check

**Step 5.1: Keyboard Navigation Flow**
- Tab through entire page without mouse
- Verify logical tab order
- Ensure mobile menu is keyboard accessible
- Test FAQ accordions with Enter/Space
- Verify no keyboard traps

**Step 5.2: Color Contrast Audit**
- Verify `--color-text-muted` (#b8b2a6) passes on `--color-bg` (#0d1f18)
- Check `--color-text-dim` (#8a8478) usage is appropriate
- Verify light mode equivalents meet WCAG AA
- Check code block text contrast

**Step 5.3: Animation Performance**
- Verify animations use GPU-accelerated properties (transform, opacity)
- Check `prefers-reduced-motion` disables animations
- Ensure no layout shifts during scroll
- Test terminal demo restart/play/pause controls

---

## Critical Files for Implementation

| File | Changes Needed | Justification |
|------|----------------|---------------|
| `index.html` | Fix SVG colors (lines ~383-447), extract inline styles (~100 instances), verify installation commands | Main content file with legacy colors and inline styles that violate design system consistency |
| `styles.css` | Add 10-15 new component classes for extracted inline styles | CSS file needs new semantic classes to replace inline styles |
| `script.js` | Verify terminal demo, audience selector, and scroll animation functionality | Interactive features need testing to confirm smooth operation |
| `assets/logo-icon.svg` | Verify exists and matches Forest Editorial palette | Favicon/brand mark should align with design system |

---

## Risks & Mitigations

### Risk 1: SVG Color Changes Break Visual Coherence
- **Likelihood:** Medium | **Impact:** Medium
- **Mitigation:** Test both dark and light modes after SVG color update. The Forest Editorial palette was specifically designed for both modes, but take screenshots to verify contrast and visual harmony.

### Risk 2: Inline Style Extraction Breaks Layout
- **Likelihood:** Low | **Impact:** Medium
- **Mitigation:** Extract styles incrementally, section by section. Take Playwright screenshots after each extraction to verify visual appearance is preserved. Use specific BEM-like class names to avoid CSS conflicts.

### Risk 3: Mobile Touch Targets Too Small
- **Likelihood:** Low | **Impact:** Medium
- **Mitigation:** Audit all buttons and interactive elements for 44px minimum touch targets. Current implementation appears to handle this, but explicit verification via Playwright at mobile sizes is required.

### Risk 4: Light Mode Has Hidden Accessibility Issues
- **Likelihood:** Medium | **Impact:** Low
- **Mitigation:** Light mode CSS exists but may have untested edge cases. Take comprehensive screenshots in light mode and manually verify text contrast on all surfaces.

### Risk 5: Audience Selector State Persists Incorrectly
- **Likelihood:** Low | **Impact:** Low
- **Mitigation:** Test localStorage handling by selecting each audience, refreshing the page, and verifying the selection persists. Clear localStorage and test default behavior.

---

## Verification Strategy

### Acceptance Check 1: Dribbble-Level Visual Quality
**Method:** Playwright screenshots + visual inspection
```
1. Start local server: python -m http.server 8000
2. Navigate to http://localhost:8000
3. Take full-page screenshots at: 1920px, 1440px, 768px, 375px
4. Toggle dark/light modes and capture both
5. Evaluate for:
   - Premium, intentional aesthetic (not "generic AI tool gray")
   - Consistent Forest Editorial visual language throughout
   - Workflow diagram colors match design system (no cyan/violet)
   - Typography creates clear, editorial hierarchy
   - Color palette cohesive (forest green + warm amber)
```

### Acceptance Check 2: Clear User Understanding for All Audiences
**Method:** Walkthrough as each persona
- **Developer:** Can they quickly find technical details? (config options, iteration modes, agent fallback, CLI flags)
- **Vibe Coder:** Is value proposition clear within 10 seconds? (time saved, hands-free operation, before/after comparison)
- **Newcomer:** Can they find CLI glossary? Understand installation steps? Feel guided, not lost?
- Verify audience selector toggles content correctly

### Acceptance Check 3: Technical Quality
**Method:** Browser DevTools + manual testing
- No console errors on page load or during interactions
- Smooth animations (terminal demo, scroll reveals, parallax)
- All interactive elements functional:
  - Dark mode toggle switches themes
  - Mobile navigation opens/closes
  - Terminal demo controls (restart, play/pause, speed)
  - Copy buttons show "Copied!" feedback
  - FAQ accordions expand/collapse
  - Audience selector toggles content

### Acceptance Check 4: Accessibility Basics
**Method:** Keyboard navigation + contrast verification
- Tab through entire page without mouse
- All interactive elements receive visible focus
- Focus order is logical (top to bottom, left to right)
- No keyboard traps
- Text contrast meets WCAG AA (4.5:1 normal, 3:1 large)
- Skip link works correctly
- `prefers-reduced-motion` disables animations

### Playwright Test Sequence
```
1. mcp__playwright__browser_navigate to http://localhost:8000
2. mcp__playwright__browser_snapshot for accessibility tree review
3. mcp__playwright__browser_take_screenshot (desktop-dark-1920.png) at 1920px
4. mcp__playwright__browser_resize to 1440px
5. mcp__playwright__browser_take_screenshot (desktop-dark-1440.png)
6. mcp__playwright__browser_click on dark mode toggle
7. mcp__playwright__browser_take_screenshot (desktop-light-1440.png)
8. mcp__playwright__browser_resize to 768px
9. mcp__playwright__browser_take_screenshot (tablet-light.png)
10. mcp__playwright__browser_resize to 375px
11. mcp__playwright__browser_take_screenshot (mobile-light.png)
12. mcp__playwright__browser_click on hamburger menu
13. mcp__playwright__browser_take_screenshot (mobile-menu.png)
14. Test terminal demo, copy buttons, FAQ accordions via clicks
15. Repeat screenshots in dark mode at mobile
```

---

## CLAUDE.md Premium UI Checklist

Before marking complete, verify all items:

- [ ] **Hierarchy scan**: Can I tell what matters in 3 seconds?
- [ ] **Spacing scan**: Is spacing consistent across sections? (8px grid)
- [ ] **State scan**: Hover/focus/disabled/loading done everywhere?
- [ ] **Contrast scan**: Any gray-on-gray that's hard to read?
- [ ] **Responsive scan**: Phone + tablet + desktop look intentional?
- [ ] **Edge cases**: Long text, empty states, errors handled?
- [ ] **Polish pass**: Alignments, radii, shadows consistent?

---

## Success Criteria

The implementation is complete when:

1. **Visual Quality**: Page would be considered for Dribbble showcase - premium, intentional, distinctive
2. **Design Consistency**: All colors match Forest Editorial palette (no legacy cyan/violet remnants in SVG)
3. **Code Quality**: Inline styles extracted to proper CSS classes
4. **Audience Clarity**: Users from all three audiences understand what Ralph is and how to use it
5. **Technical Quality**: No console errors, smooth animations, responsive at all sizes
6. **Accessibility**: Keyboard navigable, readable contrast, clear focus states
7. **Static Compliance**: Deploys successfully to Codeberg Pages without any build step
