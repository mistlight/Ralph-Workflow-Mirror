# Design Stabilization Implementation Plan

## Summary

This plan completes the styling of the Ralph Workflow static site, which uses a "Terminal Noir" aesthetic with electric cyan (#00d4ff), hot magenta (#ff006e), and lime accents on deep charcoal backgrounds (#0a0a0b), paired with Syne/DM Sans/JetBrains Mono typography.

After comprehensive visual inspection via Playwright, the site is **largely well-styled** with proper dark/light themes, consistent typography, and functional interactive elements. The key changes required are:

1. **Remove the "Try Ralph Now" demo section (Section 03)** - Per user requirements, the demo is unnecessary since users can download the app through cargo/crate
2. **Renumber subsequent sections** - Adjust section numbers from 04→03, 05→04, 06→05, 07→06
3. **Clean up unused demo CSS** - Remove ~225 lines of `.demo-*` styles
4. **Remove demo JavaScript** - Clean up any demo-related JS code
5. **Minor visual polish** - Ensure consistent spacing and styling after demo removal

This is **iterative refinement on an existing solid design**, not a redesign. The Terminal Noir aesthetic is well-executed and should be preserved.

---

## Implementation Steps

### Phase 1: Remove Demo Section from HTML

#### Step 1.1: Delete "Try Ralph Now" Section
**What**: Remove the entire demo section from `index.html`
**Location**: Lines 911-1029 (`<section id="try-ralph">` through `</section>`)
**Content being removed**:
- Section header with "03" label and "Try Ralph Now" title
- Demo container with tabbed interface (PROMPT.md, Terminal Output, Generated Code)
- Demo explanation sidebar with 5 workflow steps
- Run Demo button and status bar

**Files**: `index.html`

#### Step 1.2: Renumber Remaining Sections
**What**: Update section-number spans after demo removal

| Section | Current Number | New Number |
|---------|---------------|------------|
| Install | 04 | 03 |
| Features | 05 | 04 |
| Audience | 06 | 05 |
| FAQ | 07 | 06 |

**Files**: `index.html` (4 `.section-number` elements to update)

#### Step 1.3: Verify Navigation Links
**What**: Ensure anchor links still work correctly
- `#install` - Points to install section (anchor unchanged)
- `#features` - Points to features section (anchor unchanged)
- `#faq` - Points to FAQ section (anchor unchanged)
- Hero CTA "Install Ralph" → `#install` (should still work)

**Files**: `index.html` (verify, likely no changes needed)

---

### Phase 2: Clean Up Demo CSS

#### Step 2.1: Remove Demo-Related Styles from styles.css
**What**: Delete all `.demo-*` class definitions
**Location**: Approximately lines 2787-3011 in `styles.css`

**Classes to remove**:
```css
.demo-steps
.demo-step
.demo-step:hover
.demo-step-number
.demo-step-content
.demo-step-content h4
.demo-step-content p
.demo-explanation
.demo-explanation h3
.demo-section
.demo-container
.demo-header
.demo-run-btn
.demo-tabs
.demo-tab
.demo-tab::after
.demo-tab:hover
.demo-tab.active
.demo-panel
.demo-terminal
/* ... and all related media queries */
```

**Files**: `styles.css` (~225 lines)

---

### Phase 3: Clean Up Demo JavaScript

#### Step 3.1: Remove Demo-Related JavaScript
**What**: Search `script.js` for demo-related code and remove
**Search patterns**:
- `demo-`
- `#try-ralph`
- `#demo-`
- `demoTab`
- `demoPanel`

**Files**: `script.js`

---

### Phase 4: Visual Polish After Demo Removal

#### Step 4.1: Verify Section Transitions
**What**: After demo removal, ensure smooth visual flow from:
- "What is PROMPT.md?" (02) → Install section (now 03)

**Check for**:
- Consistent section padding/margins
- No jarring visual gaps
- Proper background color transitions

#### Step 4.2: Verify Hero Animation Remains Primary
**What**: The hero section has a terminal animation that demonstrates Ralph
- Ensure it remains the primary visual demo
- Verify playback controls work (restart, pause, speed controls)
- Test in both light and dark modes

#### Step 4.3: Mobile Responsiveness Check
**What**: Test the site at key breakpoints after changes
**Breakpoints**: 375px, 640px, 768px, 1024px, 1440px
**Focus**: Install section layout (most affected by demo removal being adjacent)

---

## Critical Files for Implementation

| File | Purpose | Estimated Changes |
|------|---------|-------------------|
| `index.html` | Remove demo section, renumber sections | ~120 lines removed, 4 line edits |
| `styles.css` | Remove demo CSS classes | ~225 lines removed |
| `script.js` | Remove demo JavaScript | Variable, depends on demo code scope |

---

## Risks & Mitigations

### Risk 1: Breaking Hero Terminal Animation
**Likelihood**: Low
**Impact**: High
**Mitigation**: The hero terminal is a separate component (`#terminal-demo`). Verify it's not affected by demo section removal. The demo section has different element IDs (`#demo-terminal`, `#demo-run-btn`).

### Risk 2: JavaScript Errors After Demo Code Removal
**Likelihood**: Medium
**Impact**: Medium
**Mitigation**:
- Search for all demo-related selectors before removing code
- Test all interactive elements after changes
- Check browser console for errors

### Risk 3: CSS Cascade Issues
**Likelihood**: Low
**Impact**: Low
**Mitigation**: Demo styles are scoped with `.demo-*` prefix and shouldn't affect other components. Remove complete rule blocks, not partial edits.

### Risk 4: Section Navigation Breaking
**Likelihood**: Very Low
**Impact**: Low
**Mitigation**: Section IDs (`#install`, `#features`, `#faq`) remain unchanged. Only visible section numbers change (04→03, etc.).

---

## Verification Strategy

### After Phase 1 (HTML Changes):
- [ ] Demo section no longer visible on page
- [ ] Section numbers show 03, 04, 05, 06 correctly
- [ ] All anchor links work (`#install`, `#features`, `#faq`)
- [ ] Hero "Install Ralph" CTA navigates correctly
- [ ] No visual gaps or layout issues

### After Phase 2 (CSS Cleanup):
- [ ] Page renders without style errors
- [ ] All remaining sections styled correctly
- [ ] No 404 errors for CSS resources in console

### After Phase 3 (JS Cleanup):
- [ ] No JavaScript errors in console
- [ ] Theme toggle works (dark/light)
- [ ] Hero terminal animation works
- [ ] FAQ accordions work
- [ ] Install mode toggle works
- [ ] Copy buttons work on code blocks
- [ ] Navigation dropdown works
- [ ] Audience selector buttons work

### Final Verification Checklist:
- [ ] Demo section completely removed
- [ ] Section numbers correct: 03 (Install), 04 (Features), 05 (Audience), 06 (FAQ)
- [ ] All demo CSS removed from styles.css
- [ ] All demo JS removed from script.js
- [ ] No console errors
- [ ] Dark mode works
- [ ] Light mode works
- [ ] Mobile responsive (375px, 768px, 1024px)
- [ ] Hero terminal animation plays correctly
- [ ] All navigation links work
- [ ] Secondary pages unaffected (getting-started, how-it-works, open-source, faq, docs)

---

## Completion Criteria

The implementation is complete when:

1. **Demo section removed** - "Try Ralph Now" (Section 03) no longer appears
2. **Sections properly numbered** - Install (03), Features (04), Audience (05), FAQ (06)
3. **No dead code** - All demo CSS and JS removed
4. **No errors** - Console clean, all interactivity functional
5. **Visual consistency** - Terminal Noir aesthetic maintained
6. **Responsive design** - Works at all breakpoints
7. **All pages functional** - Secondary pages continue working

---

## Notes on Existing Design Quality

The current Terminal Noir design is **well-executed** and should be preserved:

✓ **Color System**: Deep blacks (#0a0a0b), electric cyan (#00d4ff), hot magenta (#ff006e), lime (#a3ff12)
✓ **Typography**: Syne (headings), DM Sans (body), JetBrains Mono (code)
✓ **Animations**: Terminal animation, hover effects, scroll reveals
✓ **Accessibility**: Skip links, ARIA labels, focus states
✓ **Theme Support**: Light/dark mode toggle works correctly
✓ **Responsive**: Breakpoints implemented properly
✓ **All secondary pages**: Properly styled with consistent theme

This plan is **iterative refinement** - removing unnecessary content while preserving the solid design foundation.
