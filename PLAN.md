# Design Stabilization Implementation Plan

## Summary

This plan addresses the **demo section removal** from the Ralph Workflow static site. The site currently uses a polished "Terminal Noir" aesthetic that is well-implemented. The primary task is to remove the demo section (users can download the app through crate instead) and renumber subsequent sections.

### Visual Inspection Findings

The site design is already complete and polished:
- Hero section: Well-styled with Terminal Noir aesthetic
- All content sections: Properly styled with consistent theme
- Footer: Complete with proper styling
- How-it-Works page: Fully styled
- Getting-Started page: Functional and themed

The only required change is **removing the demo section**.

---

## Implementation Steps

### Step 1: Remove Demo Section from index.html
**Priority: CRITICAL**

Remove the entire demo section from `index.html`. The section is identified by:
- Section number "04" with heading "Try Ralph Now"
- Contains demo tabs (PROMPT.md, Terminal Output, Generated Code)
- Contains "Run Demo" button
- Located approximately between lines 440-497

**HTML to remove**: Everything within the demo section `<section>` tags, including:
- Demo header with "Try Ralph Now" heading
- Demo tabs interface
- Demo panels (prompt, terminal, code)
- Demo steps explanation
- Run Demo button

### Step 2: Renumber Section Numbers
**Priority: HIGH**

After demo removal, update section number badges to maintain sequential ordering:

| Current | Content | New Number |
|---------|---------|------------|
| 01 | What is Ralph? | 01 (unchanged) |
| 02 | How it Works | 02 (unchanged) |
| 03 | Quick Benefits | 03 (unchanged) |
| ~~04~~ | ~~Demo~~ | ~~REMOVED~~ |
| 05 | Install | **04** |
| 06 | Features | **05** |
| 07 | Who's it For | **06** |

**Files to modify**: `index.html`

Search for `.section-number` elements with values "05", "06", "07" and decrement each by 1.

### Step 3: Remove Orphaned Demo CSS (Optional)
**Priority: LOW**

Clean up CSS rules that will no longer be used after demo removal. These styles are located in `styles.css` approximately lines 2783-2937:

```css
/* Demo section styles to remove */
.demo-section
.demo-container
.demo-header
.demo-run-btn
.demo-tabs
.demo-tab
.demo-panel
.demo-terminal
.demo-steps
.demo-step
.demo-step-number
.demo-step-content
```

**Note**: This step is optional. Orphaned CSS doesn't affect functionality, only adds minor file size. Can be deferred.

### Step 4: Verification
**Priority: HIGH**

After changes, verify:
1. Index page renders without the demo section
2. Section numbers are sequential (01, 02, 03, 04, 05, 06)
3. No visual regressions on any section
4. All other pages still function correctly
5. Navigation links work properly
6. Theme toggle still functions

---

## Files to Modify

| File | Changes |
|------|---------|
| `index.html` | Remove demo section, renumber sections |
| `styles.css` | (Optional) Remove orphaned demo styles |

---

## Risks & Mitigations

### Risk 1: Breaking Page Layout
**Likelihood**: Low
**Impact**: Medium
**Mitigation**: Demo section is self-contained. Removal should not affect surrounding sections.

### Risk 2: Missing Section Number Update
**Likelihood**: Medium
**Impact**: Low
**Mitigation**: Search for all `.section-number` elements to ensure none are missed.

---

## Completion Criteria

The implementation is complete when:

1. Demo section is completely removed from index.html
2. Section numbers are sequential (01-06)
3. No visual regressions on the index page
4. All navigation and functionality works
5. Site renders correctly at all breakpoints

---

## Notes

- The existing Terminal Noir design is well-implemented and requires no changes beyond demo removal
- Font stack (Syne, DM Sans, JetBrains Mono) is correctly applied
- Color system (cyan #00d4ff, magenta #ff006e, lime #a3ff12) is consistent
- Responsive design is functioning properly
