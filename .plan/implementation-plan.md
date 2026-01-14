# Ralph Workflow Website - Implementation Plan

## Summary

The Ralph Workflow website has a substantial foundation with a "Noir Editorial" design system (dark #0A0A0B backgrounds, electric cyan #00D4FF accent, violet #A78BFA secondary), comprehensive content targeting three audiences (developers, vibe coders, CLI newcomers), and full Codeberg Pages static hosting compatibility. Existing QA documentation claims the site is "production ready" and "portfolio-grade," but to meet the strict **frontend-design skill criteria** requiring verification via **visual output** (not code inspection), we must conduct fresh Playwright visual evaluation. The implementation will: (1) Capture current visual state via Playwright screenshots at multiple breakpoints, (2) Evaluate against frontend-design skill criteria (distinctive typography, bold color palette, high-impact motion, atmospheric backgrounds, NOT generic AI aesthetics), (3) Apply targeted refinements where gaps are identified, (4) Verify installation instructions use the correct git clone URL, and (5) Ensure the site creates clear user understanding without unanswered questions.

**Key Insight**: The actual CSS uses "Noir Editorial" with Clash Display + General Sans + JetBrains Mono fonts and #00D4FF cyan accent. Some existing reports mention different palettes (forest green + amber), indicating possible documentation staleness. We must trust visual output verification, not documentation claims.

---

## Implementation Steps

### Phase 1: Visual Baseline Assessment with Playwright

**Step 1.1: Start Local Server and Capture Current State**
- Serve the site locally using `python3 -m http.server 8766`
- Navigate Playwright to `http://localhost:8766`
- Capture full-page screenshots at 4 breakpoints:
  - Desktop 1920px (dark mode - default)
  - Desktop 1920px (light mode - after toggle)
  - Tablet 768px
  - Mobile 375px
- Files: None modified; screenshots captured for visual review

**Step 1.2: Evaluate Against Frontend-Design Skill Criteria**
Using captured screenshots, assess each criterion:
- **Typography**: Are Clash Display (display) + General Sans (body) distinctive and NOT on the "generic" list (Inter, Roboto, Arial)?
- **Color**: Is the Noir (#0A0A0B) + Electric Cyan (#00D4FF) palette bold with "dominant colors and sharp accents"?
- **Motion**: Does the hero have "one well-orchestrated page load with staggered reveals"?
- **Spatial Composition**: Is there "unexpected layouts, asymmetry, grid-breaking elements"?
- **Backgrounds**: Do hero orbs + noise texture + grid pattern create "atmosphere and depth"?
- **NOT Generic AI**: No purple gradients on white, no cookie-cutter patterns?

**Step 1.3: Document Gap Analysis**
- If any criterion fails, document specific issues with screenshots
- Prioritize fixes by visual impact
- Create targeted refinement list

### Phase 2: Critical Visual Refinements (If Gaps Identified)

**Step 2.1: Hero Section Enhancement** (if needed)
- Verify terminal demo creates visual impact as hero centerpiece
- Check hero title word-by-word animation (slideUp with stagger delays)
- Ensure gradient orbs + grid pattern create premium atmosphere
- Timing should be tasteful: 120-260ms for transitions per CLAUDE.md
- Files: `styles.css` (hero section ~945-1574), `script.js` (hero animations)

**Step 2.2: Typography Hierarchy Verification** (if needed)
- Confirm fluid typography (clamp() functions) scales correctly at all breakpoints
- Verify display vs. body size contrast creates clear hierarchy
- Check line lengths stay within 60-80 characters (max-width: 65ch applied)
- Files: `styles.css` (typography tokens ~93-150)

**Step 2.3: Color System Consistency** (known issue)
- **Known Issue**: Workflow diagram SVG (lines ~379-444) reportedly uses #CCFF00, #FF00AA, #9D4EDD instead of design system colors
- Audit and replace hardcoded colors with CSS variables where possible
- Verify dark/light mode toggle works smoothly
- Files: `index.html` (SVG workflow diagram), `styles.css`

**Step 2.4: Interactive State Completeness**
- Verify ALL buttons have: default, hover, active/pressed, focus-visible, disabled states
- Check copy buttons show visual feedback (success/error)
- Test terminal controls (play/pause/restart/speed buttons)
- Verify audience selector button states (aria-pressed="true")
- Files: `styles.css`, `script.js`

### Phase 3: Content & UX Verification

**Step 3.1: Installation Instructions Accuracy**
- **CRITICAL**: Verify clone URL is exactly: `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`
- Verify cargo crate name is `ralph-workflow`
- Check troubleshooting section provides helpful guidance
- Files: `index.html` (install section ~1072-1228)

**Step 3.2: Audience Content Coverage**
- Test Developer content path: technical details, workflow automation focus
- Test Vibe Coder content path: creative freedom, time-saving focus
- Test CLI Newcomer content path: should show Glossary section, beginner-friendly language
- Verify each audience gets appropriate depth without leaving questions unanswered
- Files: `index.html`, `script.js` (audience filtering logic)

**Step 3.3: Navigation & Flow Testing**
- Test all anchor links (#what-is-ralph, #how-it-works, #install, #features, #glossary)
- Verify mobile hamburger menu opens/closes smoothly
- Check scroll progress indicator works
- Verify skip-to-content link is functional
- Files: `script.js` (navigation handlers)

### Phase 4: Final Visual Polish Pass

**Step 4.1: Alignment & Spacing Audit**
- Verify 8-point grid (--space-* tokens) applied consistently
- Check optical alignment (icons aligned with text)
- Ensure consistent border radius (4px/8px/12px/16px/24px tier system)
- Files: `styles.css`

**Step 4.2: Shadow & Depth Consistency**
- Verify glow shadows (--shadow-glow-sm/md/lg) on accent elements
- Check card elevation on hover (translateY + shadow enhancement)
- Ensure dark mode shadows maintain visual hierarchy
- Files: `styles.css`

**Step 4.3: Edge Cases**
- Review button labels for action-orientation
- Verify terminal "Ready to run" state looks intentional
- Check error states in copy functionality work
- Test with long text content
- Files: `index.html`, `script.js`

### Phase 5: Comprehensive Playwright Verification

**Step 5.1: Full Visual Regression at All Breakpoints**
- Capture final screenshots:
  - Mobile 375px (portrait)
  - Tablet 768px
  - Desktop 1280px
  - Desktop 1920px
- Document visual quality at each breakpoint
- Save screenshots for verification evidence

**Step 5.2: Interactive Feature Testing via Playwright**
- Test dark mode toggle: `mcp__playwright__browser_click` on toggle button
- Test audience selector: click each of 3 audience buttons
- Test terminal demo: play/pause/restart/speed controls
- Test copy buttons: click and verify feedback
- Test FAQ accordions: expand/collapse
- Test navigation: smooth scroll to sections
- Document pass/fail for each interaction

**Step 5.3: Frontend-Design Skill Final Checklist**
Verify against ALL skill criteria:
- [ ] Typography: Distinctive fonts (Clash Display, General Sans, JetBrains Mono) ✓/✗
- [ ] Color: Dominant Noir + sharp Cyan accents ✓/✗
- [ ] Motion: High-impact page load animation with staggered reveals ✓/✗
- [ ] Spatial: Terminal centerpiece creates interesting composition ✓/✗
- [ ] Backgrounds: Gradient orbs + noise + grid create atmosphere ✓/✗
- [ ] NOT generic AI aesthetics (no Inter, no purple-on-white) ✓/✗

**Step 5.4: Premium UI Checklist (CLAUDE.md)**
- [ ] Hierarchy scan: Clear in 3 seconds?
- [ ] Spacing scan: 8-point grid consistent?
- [ ] State scan: All button/link states implemented?
- [ ] Contrast scan: No hard-to-read text?
- [ ] Responsive scan: Intentional design at all sizes?
- [ ] Edge cases: Handled gracefully?
- [ ] Polish pass: Alignments, radii, shadows, icons consistent?

### Phase 6: Codeberg Pages Compliance Verification

**Step 6.1: Static-Only Confirmation**
- Confirm no server-side processing required
- Verify all assets are inline or CDN (fonts from Fontshare/Google Fonts)
- Check combined file sizes are reasonable (<3MB total)
- Files: All files (size audit)

**Step 6.2: Progressive Enhancement**
- Test with JavaScript disabled (noscript fallback present)
- Verify content remains accessible without JS
- Files: `index.html` (noscript section ~66-95)

---

## Critical Files for Implementation

1. **`index.html`** (~1,606 lines)
   - Main page structure with all content sections
   - SVG workflow diagram (potential color fix needed ~379-444)
   - Install section with clone URL to verify (~1072-1228)
   - Inline styles in some sections (comparison, FAQ) to potentially extract

2. **`styles.css`** (~2,718 lines)
   - Complete "Noir Editorial" design system
   - CSS custom properties for colors, typography, spacing, shadows
   - Dark/light mode support
   - Component styles and animations

3. **`script.js`** (~1,456 lines)
   - All interactions: terminal demo, navigation, dark mode toggle
   - Scroll animations with Intersection Observer
   - Audience selector and content filtering
   - Copy-to-clipboard functionality

4. **`assets/logo.svg`** and **`assets/logo-icon.svg`** (~40 and ~20 lines)
   - Logo assets (verify colors match design system)
   - Uses gradient #CCFF00 to #00FFF0 (may need alignment with #00D4FF primary)

5. **`CLAUDE.md`** (~222 lines)
   - Design guidelines and Premium UI checklist
   - 8-point spacing grid specification
   - Component state requirements
   - Quality bar definition

---

## Risks & Mitigations

### Risk 1: Documentation vs. Reality Mismatch
**Concern**: Existing reports claim "forest green + amber" palette but actual CSS shows "Noir + Cyan". Documentation may be stale.
**Mitigation**: Use Playwright visual verification as source of truth. Trust what we SEE in screenshots, not what documents claim.

### Risk 2: Over-Refinement
**Concern**: The site may already meet quality bar; unnecessary changes could introduce regressions.
**Mitigation**: Phase 1 visual assessment FIRST. Only proceed to refinements if specific gaps are identified against frontend-design skill criteria.

### Risk 3: SVG Color Inconsistency
**Concern**: Workflow diagram and logo use colors (#CCFF00, #FF00AA, #9D4EDD) outside the CSS design system.
**Mitigation**: If visual impact is acceptable, document as acceptable variation. If jarring, update SVG colors to match design system tokens.

### Risk 4: Codeberg Pages Constraints
**Concern**: Adding features that require build steps or exceed storage limits.
**Mitigation**: Current implementation is static-only with no build process. Maintain this constraint throughout.

### Risk 5: Frontend-Design "Generic AI" Test
**Concern**: Skill explicitly warns against "Inter, Roboto, Arial" and "purple gradients on white".
**Mitigation**: Current fonts (Clash Display, General Sans) are NOT on generic list. Noir + Cyan is NOT purple-on-white. Should pass criteria.

### Risk 6: Installation URL Accuracy
**Concern**: Requirements specify exact git clone URL must be used.
**Mitigation**: Verify `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git` appears in install section. Do NOT substitute alternative URLs.

---

## Verification Strategy

### Acceptance Check 1: Working Professional Website
**How to verify**:
- Serve locally, navigate all sections
- All internal anchor links work (#what-is-ralph, #install, etc.)
- External Codeberg links open in new tabs
- Dark mode toggle functions and persists
- Mobile navigation works (hamburger menu)
- Terminal demo plays, pauses, restarts
- Copy buttons work with feedback
- FAQ accordions expand/collapse

### Acceptance Check 2: Dribbble-Showcase Quality
**How to verify**:
- Playwright screenshots at 375px, 768px, 1280px, 1920px
- Visual inspection for:
  - Premium, intentional aesthetic
  - Consistent visual language across sections
  - No "template" or "generic AI" feeling
  - Distinctive typography (Clash Display hero)
  - Bold color usage (Noir + Electric Cyan)
  - Atmospheric backgrounds (orbs, noise, grid)
  - Smooth, meaningful animations

### Acceptance Check 3: No Design Flaws / Good Usability
**How to verify**:
- Run CLAUDE.md Premium UI Checklist (7 points)
- Check alignment at all breakpoints
- Verify spacing follows 8-point grid
- Test all interactive states
- Keyboard-only navigation test

### Acceptance Check 4: Clear User Understanding
**How to verify**:
- Read as Developer: Can I understand how to use Ralph?
- Read as Vibe Coder: Is the value proposition clear?
- Read as CLI Newcomer: Can I follow installation steps?
- Are there unanswered questions about what Ralph does?
- Is the workflow (PROMPT.md → agents → commits) clear?

### Acceptance Check 5: Frontend-Design Skill Criteria
**How to verify (from skill document)**:
- Typography: Clash Display + General Sans are distinctive ✓
- Color: Noir (#0A0A0B) + Cyan (#00D4FF) is bold and cohesive ✓
- Motion: Hero word animation + scroll reveals ✓
- Spatial: Terminal demo as asymmetric focal point ✓
- Backgrounds: Gradient orbs + noise texture + grid pattern ✓
- NOT generic: No Inter, no purple-on-white ✓

### Playwright Verification Commands
```
1. Start server: python3 -m http.server 8766
2. mcp__playwright__browser_navigate → http://localhost:8766
3. mcp__playwright__browser_resize → 1920x1080
4. mcp__playwright__browser_take_screenshot → desktop-dark.png
5. mcp__playwright__browser_click → dark mode toggle
6. mcp__playwright__browser_take_screenshot → desktop-light.png
7. mcp__playwright__browser_resize → 375x812
8. mcp__playwright__browser_take_screenshot → mobile.png
9. mcp__playwright__browser_snapshot → accessibility tree audit
10. mcp__playwright__browser_click → test interactions
```

---

## Design Notes

### Current Aesthetic: Noir Editorial
- **Background**: Near-black #0A0A0B with blue undertones
- **Primary Accent**: Electric Cyan #00D4FF (hero, links, CTAs)
- **Secondary Accent**: Warm Violet #A78BFA (supporting highlights)
- **Typography**:
  - Clash Display: Bold geometric display face (headlines)
  - General Sans: Clean humanist sans-serif (body)
  - JetBrains Mono: Modern monospace (code blocks)
- **Texture**: Subtle noise overlay at 1.5% opacity
- **Atmosphere**: Gradient orbs (cyan, violet, emerald), grid pattern

### What Makes This Distinctive (Per Frontend-Design Skill)
1. **NOT Inter/Roboto**: Uses Clash Display and General Sans
2. **NOT purple-on-white**: Uses electric cyan on noir background
3. **NOT cookie-cutter**: Terminal demo as memorable centerpiece
4. **Bold palette**: Near-black with striking cyan accent
5. **Atmospheric depth**: Noise texture + animated gradient orbs

### Key Content Structure
- Hero: Value proposition + audience selector + terminal demo
- What is Ralph: 4-step workflow explanation + diagram
- Before/After: Time savings comparison
- Glossary: CLI terms for newcomers (conditional)
- How it Works: Step-by-step guide
- PROMPT.md: Example prompt file
- Install: Simple/Advanced modes with verification
- Features: 6 key capabilities
- Audience Cards: Developer, Vibe Coder, CLI Newcomer
- FAQ: Categorized questions with accordions
- Footer: Links and license

### Installation Instructions (Required Format)
```bash
# Clone
git clone ssh://git@codeberg.org/mistlight/Ralph-Workflow.git

# Install
cd Ralph-Workflow && cargo install --path .

# Verify
ralph --version
```
