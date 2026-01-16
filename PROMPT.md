## Design Stabilization (Frontend-Design Required)

### Mandatory tool usage

You **must use the `claude` skill `frontend-design`** for this task.

This task is fundamentally about **visual system correction, layout coherence, and design consistency**, not code correctness alone. All decisions must be validated against **rendered visual output**, not abstract reasoning.

Failure to use `frontend-design` for diagnosis and implementation is considered a failure of the task.

---

### Context

All sections **after Section 3 (Getting Started)** currently look visually “off” in rendered output. Treat this as a **system-level design problem**, not a set of isolated bugs.

Earlier sections (1–3) establish the intended quality bar and serve as the **reference baseline**.

---

### Objective

Using the `frontend-design` skill, bring **all post–Section 3 sections/pages** to the same visual and interaction quality as Sections 1–3 by fixing **fundamental design issues**, including (but not limited to):

* Layout model inconsistencies
* Broken vertical rhythm and spacing cadence
* Typography hierarchy collapse or drift
* Component inconsistency across pages
* Docs pages lacking a coherent reading layout
* Visual density issues (too cramped or too loose)
* Inconsistent use of color, accents, or emphasis

---

### Guardrails

* Keep the site **fully static**, file-open compatible, and Codeberg Pages compatible.
* Do not rewrite Sections 1–3 unless a **system-level correction** requires it.
* Global changes are allowed **only** if they:

  * Fix a root cause affecting post–Section 3 pages, and
  * Do not degrade Sections 1–3 when re-rendered.

---

### Required approach (design-first, not surgical)

Using `frontend-design`, follow this flow:

#### 1. Rendered diagnosis (required)

Visually review post–Section 3 pages and explicitly identify the **top 2–4 fundamental causes** of the “off” feeling. Examples:

* Inconsistent container widths / gutters vs earlier pages
* Section spacing that breaks established rhythm
* Headings, body text, and code blocks lacking clear hierarchy
* Ad-hoc components instead of shared canonical ones
* Docs content missing a consistent reading pattern

You must name these causes before fixing anything.

---

#### 2. Fundamental correction passes (limited, cohesive)

Make **cohesive, system-level fixes** in **no more than 3–4 passes**:

**Pass A — Layout model unification**
Standardize page containers, section padding, reading widths, and grid usage to match Sections 1–3.

**Pass B — Typography & rhythm restoration**
Re-establish heading scale, spacing ladder, line-length, and code readability across all post–Section 3 pages.

**Pass C — Component normalization**
Ensure cards, callouts, code blocks, lists, steps, FAQs, etc. use the same canonical styles everywhere.

**Pass D (optional) — Docs-specific polish**
Improve scannability and flow (intros, callouts, examples, checklists, next-steps blocks) without introducing new visual styles.

Avoid unbounded tweaking. Each pass should clearly reduce visible inconsistency.

---

### Verification (required after each pass)

Using rendered output, confirm:

* Post–Section 3 pages now visually match Sections 1–3
* Spacing, typography, and components are consistent
* Mobile and tablet layouts are clean and readable
* Focus states, keyboard navigation, and reduced motion still work
* All pages still function when opened directly as files

---

### Reporting format (agent)

After each pass, report:

* **Diagnosis:** underlying design issues identified
* **Changes made:** system-level corrections applied
* **Visual impact:** what now looks correct
* **Regression check:** confirmation Sections 1–3 are unchanged or improved
* **Remaining issues:** if any, and which pass addresses them

---

### Completion criteria

You are finished only when:

* All post–Section 3 sections feel like part of the same product
* Visual rhythm, hierarchy, and components are consistent site-wide
* The site reads as a polished documentation + marketing experience
* No “late-stage appended” or “unstyled docs” feeling remains
