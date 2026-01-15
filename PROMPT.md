# [Feature Name] — Ralph Workflow Website (Static, Codeberg Pages)

Create a **commercial-grade** marketing + docs website for **Ralph Workflow** (open source, **AGPL**) that works as a **fully static** site and can be hosted on **Codeberg Pages**. The site must be useful beyond a README: it should **teach**, **orient**, and **get users to success** without creating new questions.

---

## Core Goal

You **must use the `claude` skill `frontend-design`** to design and implement this website for Ralph Workflow.

**Quality bar:** Dribbble-level polish, professional micro-details, consistent typography, spacing, and interaction states across **every page and section**—not only the hero.

**Conceptual anchor:** Ralph Workflow is inspired by Geoffrey Huntley’s “Ralph” concept: a structured loop that lets an AI agent work autonomously while you step away (e.g., overnight). Users must provide a **detailed product specification** for good results.
Reference: [https://ghuntley.com/ralph/](https://ghuntley.com/ralph/)

---

## Non-Negotiable Requirements (Read Carefully)

### 1) Whole-site quality and defect-free execution

You must audit and iterate on **every section** of **every page**:

* No “small” defects: alignment, spacing rhythm, inconsistent radii, mismatched icon weights, awkward line breaks, broken hover/focus, visual jitter, inconsistent shadows, off-by-one padding, etc.
* The design must remain cohesive across:

  * Desktop, tablet, mobile
  * Light/dark (if implemented)
  * Keyboard-only navigation
  * Reduced motion preference

### 2) Audience clarity (3 personas)

The website must be crystal clear for:

* **Software developers** (care about architecture, reproducibility, CLI accuracy)
* **New to the command line** (need handholding and safe defaults)
* **“Vibe coders”** (want quick wins, examples, approachable language)

### 3) Static-only + zero runtime build dependency on hosting

* Site must be **fully static**, compatible with **Codeberg Pages**
* Repository must contain **final compiled output only** (HTML/CSS/JS/images/fonts)
* **No server-side code**
* **No runtime build step required** on Codeberg Pages
* Must work when opened as a file: **`index.html` opened directly** (no dev server)

### 4) Installation instructions are tightly constrained

If you include installation instructions, they must:

* Use **only** cloning from: `ssh://git@codeberg.org/mistlight/Ralph-Workflow.git`
* Use the cargo name: **`ralph-workflow`**
* Avoid alternative install methods (no brew, no curl pipe, no cargo install from crates.io unless the repo explicitly supports it—assume it does not)
* Be correct, copy/paste-ready, and OS-aware where appropriate

---

## Deliverable Scope

### Pages to build (minimum)

1. **Home** (marketing + concept + outcomes)
2. **How it Works** (the “Ralph loop” explained with a simple visual + step flow)
3. **Getting Started** (install + first run + “your first spec” guide)
4. **Docs / Guides**

   * Writing a great product spec (templates + examples)
   * Running “overnight” safely (timeouts, cost control guidance, logs)
   * Common workflows (examples)
5. **Open Source / AGPL**

   * License summary (plain language)
   * Contribution guidelines link/section
   * Project values and community expectations
6. **FAQ / Troubleshooting**
7. **Changelog or Releases link-out** (if available) + repo CTA

> If you believe a different IA is better, you may revise it—but you must still cover these information needs.

---

## Content Requirements (Do not skip)

### Explain the product in plain language

* What Ralph Workflow is
* What problem it solves (“walk away for 8 hours; come back to progress”)
* Why specs matter (quality depends on input spec)
* What you can expect it to do (and what it won’t do)

### Teach users how to succeed

* Provide a **Product Spec Template**
* Provide **two examples**:

  * A developer-oriented spec (technical)
  * A “vibe coder” spec (plain language with guardrails)
* Provide a “spec checklist” (must-haves vs nice-to-haves)

### Distinguish website vs README

The site must not simply rephrase README content. It should add:

* Better onboarding
* Better mental model
* Real examples and templates
* Troubleshooting and “next steps”

### Project identity assets (logo/favicon)

You do not have an existing logo. You must:

* Create a simple, clean **text-first brand** (typographic mark)
* Use a minimal generated icon (e.g., a stylized “R” loop/arrow) as favicon
* Ensure the chosen mark is consistent across nav, footer, social preview (if added)

---

## Design & UX Requirements (Commercial-grade)

### Visual system

* Establish a clear design system:

  * Type scale
  * Spacing scale
  * Color tokens
  * Elevation/shadows
  * Border radii
  * Component library (buttons, cards, callouts, code blocks, tabs)
* Use consistent, high-quality iconography (one set)

### Interaction details

* Hover/active/focus states for all interactives
* Smooth but respectful motion (honor `prefers-reduced-motion`)
* Scroll behavior should not break opening as file (avoid route-dependent JS assumptions)

### Accessibility basics (required)

* WCAG-ish contrast (practical, readable)
* Keyboard navigability end-to-end
* Visible focus rings (not removed)
* Semantic headings and landmarks
* Responsive layouts (no horizontal scrolling, no tiny tap targets)

---

## Technical Constraints & Implementation Guidance

### Static architecture

* Must run from file system:

  * Avoid SPA routers that rely on server rewrites
  * Use either:

    * Multi-page static (`/index.html`, `/getting-started.html`, etc.), **or**
    * Hash-based navigation if truly needed (prefer multi-page for reliability)
* All links must work in both:

  * Codeberg Pages hosting
  * Local “open index.html” mode

### Assets

* Self-host fonts if used (or system font stack to keep it simple)
* Optimize images and include proper `alt` text
* Include a lightweight CSS strategy (compiled Tailwind is allowed **only** as committed output)

---

## Implementation Process (Must Follow)

1. **Information architecture + wire-level plan**
   Decide page list, nav structure, and per-page sections tailored to the 3 personas.

2. **Design system first**
   Lock tokens/components, then apply consistently across all pages.

3. **Build page-by-page**
   Validate in rendered output at common breakpoints.

4. **Polish pass across the entire site**
   You must do a full-site QA pass:

   * Typography rhythm
   * Spacing consistency
   * Code block styling and readability
   * Contrast and focus
   * Mobile nav behavior
   * Footer and secondary sections (not neglected)

5. **Final acceptance verification**
   Explicitly confirm each acceptance check is met.

---

## Canonical Resources (Use These)

* Concept article: [https://ghuntley.com/ralph/](https://ghuntley.com/ralph/)
* Repository: [https://codeberg.org/mistlight/Ralph-Workflow](https://codeberg.org/mistlight/Ralph-Workflow)

---

## Acceptance Checks (Strict)

* **Commercial-grade** visual quality (Dribbble-level polish)
* Decisions validated from **rendered output**, not code aesthetics
* Users can navigate and understand:

  * what Ralph Workflow is
  * how it works
  * how to install
  * how to write a strong spec
  * what to do next
* Clear to:

  * developers
  * CLI newcomers
  * vibe coders
* Accessibility basics:

  * readable contrast
  * keyboard navigation
  * focus states
  * responsive behavior
* Fully static:

  * works on Codeberg Pages
  * works from local file open (`index.html`)
  * repo contains compiled assets only

---

## Extra Credit (If it improves clarity without adding bloat)

* Social preview meta tags (OpenGraph)
* “Copy” buttons for code blocks (must still work offline)
* Printable “Spec Template” page
* Lightweight search (pure client-side, no build reliance; optional)

