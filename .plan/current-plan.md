# Ralph Workflow Website - Implementation Plan

## Summary

This plan addresses the implementation of a **commercial-grade, multi-page static website** for Ralph Workflow that works on Codeberg Pages and as a local file open. The existing codebase provides an excellent foundation with a sophisticated "Forest Editorial" design system (~3,800 lines CSS, ~1,450 lines JS), but it's currently a **single-page application** that needs to be expanded into a **multi-page static site** with additional content sections as specified in the requirements.

**Key changes needed:**
1. Expand from single-page to multi-page static architecture (7 pages minimum)
2. Add missing content: Product Spec templates, overnight running guide, contribution guidelines
3. Fix installation instruction discrepancies
4. Ensure all pages have consistent Dribbble-level polish
5. Verify static file:// compatibility across all pages

**Current state assessment:**
- ✅ Strong design foundation: Forest Editorial theme is distinctive and well-executed
- ✅ Design system tokens are comprehensive and consistent
- ✅ Interactive features work well (terminal demo, copy buttons, audience selector)
- ✅ Accessibility features present (skip links, ARIA, keyboard nav, reduced motion)
- ⚠️ Single-page structure doesn't match PROMPT.md's multi-page requirement
- ⚠️ Missing content: spec templates, overnight guide, contribution section
- ⚠️ Minor installation instruction issues (crates.io references to remove)
- ⚠️ 404.html uses off-palette colors in ambient glows

---

## Implementation Steps

### Phase 1: Multi-Page Architecture Setup

**Step 1.1: Create Page Template System**
- Extract common elements from `index.html` into reusable patterns:
  - Navigation (with active state logic)
  - Footer
  - Meta tags structure
  - Font loading
  - CSS/JS includes
- Create a consistent head section for all pages with proper canonical URLs
- **Files created:** Template patterns documented in comments

**Step 1.2: Create Additional HTML Pages**
Create the following pages as specified in PROMPT.md:

1. **`getting-started.html`** - Installation + first run + "your first spec" guide
   - Move install section content from index.html
   - Add expanded "Your First Spec" tutorial
   - Prerequisites section (Rust, API keys)

2. **`guides.html`** - Docs/Guides hub
   - Link to sub-guide sections (can use hash navigation within page)
   - Writing great specs section
   - Running overnight section
   - Common workflows section

3. **`specs.html`** - Writing a Great Product Spec
   - Product Spec Template (structured format)
   - Developer-oriented spec example
   - Vibe coder spec example
   - Spec checklist (must-haves vs nice-to-haves)

4. **`open-source.html`** - Open Source / AGPL
   - Plain-language AGPL summary
   - Contribution guidelines
   - Project values and community expectations
   - How to contribute section

5. **`faq.html`** - FAQ / Troubleshooting
   - Move/expand FAQ content from index.html
   - Add troubleshooting section
   - Common errors and solutions

6. **`changelog.html`** - Changelog / Releases (optional, can link out)
   - Can be a simple page linking to Codeberg releases
   - Or display recent notable changes

**Files:** Create 5-6 new HTML files

**Step 1.3: Update Navigation Across All Pages**
- Update nav links to point to actual page files
- Add active state styling for current page
- Ensure mobile navigation works on all pages
- Update all internal anchor links to be page-relative

**Files:** All HTML files

### Phase 2: Content Development

**Step 2.1: Create Product Spec Templates**
Write comprehensive PROMPT.md template content:

**Template A: Developer Spec (Technical)**
```markdown
# Feature: [Name]

## Problem Statement
[What problem are we solving?]

## Technical Requirements
- [ ] Requirement 1
- [ ] Requirement 2

## Acceptance Criteria
- [ ] Test case 1 passes
- [ ] Test case 2 passes

## Constraints
- Must work with existing [X] system
- Performance budget: [X]ms
```

**Template B: Vibe Coder Spec (Plain Language)**
```markdown
# What I Want to Build

## The Idea
[Describe in plain language what you're trying to create]

## What Success Looks Like
[How will you know it's done?]

## Guardrails
- Don't break existing [X]
- Keep it simple
- Make it look like [reference]
```

**Step 2.2: Write "Running Overnight Safely" Guide**
Content covering:
- Setting iteration limits (`-L` flag)
- Timeout configuration
- API cost estimation and control
- Log monitoring
- What to expect when you wake up
- Recovery from failed runs

**Step 2.3: Write Contribution Guidelines Section**
- How to report issues
- How to submit PRs
- Code style expectations
- Testing requirements
- Community code of conduct

**Files:** `specs.html`, `guides.html`, `open-source.html`

### Phase 3: Installation Instructions Fix

**Step 3.1: Audit and Correct Installation Commands**
Per PROMPT.md requirements:
- Use **only** SSH clone URL: `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`
- Cargo name: `ralph-workflow` (as specified in PROMPT.md)
- Remove crates.io references

Update in all locations:
```bash
git clone ssh://git@codeberg.org/mistlight/Ralph-Workflow.git
cd Ralph-Workflow
cargo install --path .
```

**Files:** `index.html`, `getting-started.html`, `faq.html`, footer links

**Step 3.2: Remove crates.io Reference**
- Remove the crates.io link from footer
- Update FAQ answer about "no cargo installed"

**Files:** `index.html` (footer), `faq.html`

### Phase 4: Design Consistency Pass

**Step 4.1: Fix 404.html Off-Palette Colors**
Current ambient glow uses:
- `rgba(0, 212, 255, 0.08)` - cyan
- `rgba(167, 139, 250, 0.06)` - violet

Replace with Forest Editorial palette:
- `rgba(212, 165, 116, 0.08)` - warm amber
- `rgba(125, 165, 138, 0.06)` - sage green

**Files:** `404.html`

**Step 4.2: Ensure New Pages Match Design System**
For each new page:
- Use same typography scale (Instrument Serif headings, Space Grotesk body, IBM Plex Mono code)
- Apply 8px spacing grid
- Use CSS variables for all colors
- Include scroll reveal animations
- Implement dark/light mode support

**Files:** All new HTML files

**Step 4.3: Create Page-Specific Styles**
Add any page-specific styles needed:
- Spec template code block styling
- Contribution guidelines formatting
- Changelog list styling

**Files:** `styles.css`

### Phase 5: Cross-Page Functionality

**Step 5.1: Update JavaScript for Multi-Page**
Ensure script.js works correctly on all pages:
- Navigation scroll detection (handle absence of sections)
- Dark mode toggle persistence across pages
- Copy button functionality
- Mobile navigation

**Files:** `script.js`

**Step 5.2: Link Integrity Check**
Verify all links work:
- Internal page links (relative paths)
- Anchor links within pages
- External links (open in new tab)
- File:// protocol compatibility

**Files:** All HTML files

### Phase 6: Responsive & Accessibility Polish

**Step 6.1: Test All Pages Responsively**
For each new page, verify at:
- 1440px (desktop)
- 768px (tablet)
- 375px (mobile)

Check:
- Typography scales properly
- Navigation works
- Content readable
- No horizontal scroll

**Step 6.2: Keyboard Navigation Testing**
- Tab through each page
- Verify focus states visible
- Skip link works on each page
- Interactive elements accessible

**Step 6.3: Screen Reader Audit**
- Proper heading hierarchy (h1 → h2 → h3)
- ARIA labels where needed
- Alt text for any images
- Semantic HTML structure

### Phase 7: Visual Quality Assurance

**Step 7.1: Premium UI Checklist Per Page**
For each page, verify:
- [ ] Hierarchy scan: Can I tell what matters in 3 seconds?
- [ ] Spacing scan: Consistent 8px grid?
- [ ] State scan: All interactive states implemented?
- [ ] Contrast scan: No hard-to-read text?
- [ ] Responsive scan: Intentional at all sizes?
- [ ] Polish pass: Alignments, radii, shadows consistent?

**Step 7.2: Screenshot Verification**
Take Playwright screenshots of:
- Each page at desktop, tablet, mobile
- Dark mode and light mode
- Interactive states (hover, focus)

**Step 7.3: File Protocol Testing**
Open each HTML file directly (file://) and verify:
- All CSS loads
- All JS works
- Navigation between pages works
- No console errors

---

## Critical Files for Implementation

| File | Action | Priority | Description |
|------|--------|----------|-------------|
| `index.html` | Modify | Critical | Update navigation, fix install instructions, remove crates.io link |
| `getting-started.html` | Create | Critical | Installation + first run guide |
| `guides.html` | Create | High | Documentation hub page |
| `specs.html` | Create | High | Spec templates and examples |
| `open-source.html` | Create | High | AGPL info + contribution guidelines |
| `faq.html` | Create | Medium | FAQ + troubleshooting (expand from index) |
| `404.html` | Modify | Medium | Fix off-palette ambient glow colors |
| `styles.css` | Modify | Medium | Add any new page-specific styles |
| `script.js` | Modify | Low | Ensure multi-page compatibility |

---

## Risks & Mitigations

### Risk 1: Link Breakage in Multi-Page Structure
**Likelihood:** High | **Impact:** High
**Issue:** Converting from single-page to multi-page may break internal links
**Mitigation:**
- Create comprehensive link map before starting
- Test file:// protocol after each page created
- Use relative paths consistently

### Risk 2: Design Inconsistency Across Pages
**Likelihood:** Medium | **Impact:** High
**Issue:** New pages may not match the polish level of index.html
**Mitigation:**
- Copy exact structure patterns from index.html
- Use same CSS classes and variables
- Visual comparison screenshots after each page

### Risk 3: Installation URL Confusion
**Likelihood:** Medium | **Impact:** High
**Issue:** PROMPT.md specifies different URLs than what may be the actual repo
**Mitigation:**
- Confirm with user: PROMPT.md says `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`
- Update all instances consistently once confirmed
- Add clear prerequisites section

### Risk 4: Navigation State Complexity
**Likelihood:** Low | **Impact:** Medium
**Issue:** Active nav state needs to work differently on multi-page
**Mitigation:**
- Use body class or data attribute to indicate current page
- CSS-based active state instead of scroll detection for secondary pages

### Risk 5: Duplicate CSS/JS Across Pages
**Likelihood:** Low | **Impact:** Low
**Issue:** Maintaining consistency if CSS/JS changes
**Mitigation:**
- All pages link to same styles.css and script.js
- No inline styles in new pages

---

## Verification Strategy

### Acceptance Check 1: Multi-Page Navigation Works
**How to verify:**
1. Open index.html as file://
2. Click each nav link
3. Verify correct page loads
4. Verify back button works
5. Verify active nav state displays correctly

### Acceptance Check 2: All Content Requirements Met
**How to verify:**
Checklist per PROMPT.md:
- [ ] Home page with marketing + concept
- [ ] How it works explanation with visual
- [ ] Getting started (install + first run + first spec)
- [ ] Spec writing guide with templates
- [ ] Two example specs (developer + vibe coder)
- [ ] Spec checklist
- [ ] Running overnight safely guide
- [ ] AGPL license summary
- [ ] Contribution guidelines
- [ ] FAQ + troubleshooting
- [ ] Changelog/releases link

### Acceptance Check 3: Installation Instructions Correct
**How to verify:**
1. Find all code blocks with install commands
2. Verify SSH URL: `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`
3. Verify cargo name: `ralph-workflow`
4. Verify no crates.io references
5. Copy-paste commands work

### Acceptance Check 4: Visual Quality Bar Met
**How to verify:**
For each page:
1. Take screenshot at 1440px width
2. Compare visual hierarchy to index.html
3. Check spacing consistency
4. Verify typography matches
5. Check interactive states

### Acceptance Check 5: Static File Protocol Works
**How to verify:**
1. cd to project directory
2. For each HTML file: `open [file].html` in browser
3. Navigate between all pages
4. Test all interactive features
5. Check browser console for errors

### Acceptance Check 6: Three Audiences Served
**How to verify:**
Read through site as each persona:

**Developer:**
- Can quickly find CLI flags and options
- Installation is clear and correct
- Technical spec example is useful

**Vibe Coder:**
- Value proposition is clear (overnight automation)
- Setup is approachable
- Plain language spec example is helpful

**CLI Newcomer:**
- Glossary helps with terminology
- Prerequisites are explicit
- Safe defaults are recommended

---

## Questions Requiring User Clarification

Before implementation, please confirm:

1. **Repository URL:** The PROMPT.md specifies `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`. Is this the correct and only URL to use? (Currently index.html uses this URL)

2. **Cargo Package Name:** PROMPT.md says the cargo name is `ralph-workflow`. The actual CLI command appears to be `ralph`. Should installation docs say:
   - A) "Install as `ralph-workflow`, run as `ralph`"
   - B) Something else?

3. **Multi-Page vs Single-Page:** PROMPT.md lists 7 pages. Currently everything is in one page. Should we:
   - A) Split into actual separate HTML files (recommended for file:// compatibility)
   - B) Keep as single page with deep anchor links (current)

4. **Content Depth:** For the spec templates and overnight guide - should these be:
   - A) Comprehensive (500+ words each with multiple examples)
   - B) Concise (200-300 words with one example each)
   - C) Match what's already in the Ralph-Workflow README

---

## Success Criteria

Implementation is complete when:

1. **Multi-Page Structure:** 6-7 static HTML pages covering all PROMPT.md requirements
2. **Navigation:** All pages have consistent nav that works in file:// mode
3. **Content Complete:** Spec templates, examples, overnight guide, AGPL summary, contribution guidelines
4. **Installation Correct:** SSH URL only, correct cargo name, no crates.io
5. **Design Consistent:** All pages match Forest Editorial aesthetic
6. **Dribbble Quality:** Every page passes Premium UI Checklist
7. **Static Compatibility:** All pages work when opened directly as files
8. **Accessibility:** Keyboard nav, focus states, semantic HTML, ARIA labels
9. **Responsive:** Desktop/tablet/mobile intentional layouts on all pages

---

**Plan Status:** Ready for User Review
**Estimated New Files:** 5-6 HTML pages
**Estimated Modifications:** 4-5 existing files
**Major Focus:** Multi-page architecture + content development
