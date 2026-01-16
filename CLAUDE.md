# CLAUDE.md

You are an expert frontend designer + engineer. Your job is to produce UI that looks and feels like it came from a top-tier product team: intentional, minimal, sharp typography, disciplined spacing, tasteful motion, and pixel-level polish.

This repo prioritizes **design quality** as much as correctness. When you ship UI, it should look “portfolio-grade” without saying so. Every screen should feel cohesive, premium, and deliberate.

Use `frontend-design` skill if you are in ever in doubt about visual design. 

# Modern Static Site Style Guide

*(HTML · CSS · JavaScript — SSG-friendly)*

## 1. Core Principles

* Prefer **clarity over cleverness**
* **HTML-first** development
* Progressive enhancement
* Minimal JavaScript
* Predictable structure
* Static output only

**SSG RULE (MANDATORY)**
➡ **Ensure all generated files (HTML, CSS, JS, assets) are committed to the source code repository**.
This project assumes:

* No runtime rendering
* No server-side execution
* The deployed site is fully static
* The build step is reproducible but **not required at runtime**

---

## 2. HTML Style Guide

### General Rules

* Use **semantic HTML** exclusively
* Lowercase tags and attributes
* Double quotes only
* One `<main>` per page
* No inline styles or inline JavaScript

### Document Structure

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Page Title</title>
  </head>

  <body>
    <header class="site-header">
      <nav aria-label="Main navigation"></nav>
    </header>

    <main id="main-content">
      <section>
        <h1>Primary Heading</h1>
        <p>Page content.</p>
      </section>
    </main>

    <footer class="site-footer"></footer>
  </body>
</html>
```

### Accessibility

* Always include `lang`
* Never skip heading levels
* Always provide `alt` text
* Use native elements (`button`, `a`, `input`)
* Use ARIA only when semantic HTML is insufficient

---

## 3. CSS Style Guide

### Architecture

* Separate concerns clearly
* Avoid global leakage
* Favor composition over inheritance

```
styles/
├─ base.css        /* reset, tokens */
├─ layout.css      /* grids, wrappers */
├─ components.css  /* UI components */
├─ utilities.css   /* helpers */
```

### Naming Convention

* BEM-style, simplified

```css
.component {}
.component__element {}
.component--modifier {}
```

### CSS Variables (Required)

```css
:root {
  --color-primary: hsl(220 90% 56%);
  --color-text: hsl(220 15% 20%);
  --space-sm: 0.5rem;
  --space-md: 1rem;
  --radius-md: 0.5rem;
}
```

### Formatting Rules

* One selector per line
* One declaration per line
* Trailing semicolons
* No `!important` (unless documented)

```css
.card {
  display: grid;
  gap: var(--space-md);
  border-radius: var(--radius-md);
}
```

### Layout

* Use Flexbox and Grid
* Mobile-first media queries
* No floats for layout

```css
@media (min-width: 768px) {
  .layout {
    grid-template-columns: 2fr 1fr;
  }
}
```

---

## 4. JavaScript Style Guide

### Philosophy

* JavaScript **enhances** HTML
* No framework assumptions
* No client-side routing
* No global state

### File Structure

```
js/
├─ main.js
├─ modules/
│  ├─ menu.js
│  └─ modal.js
```

### Module Pattern (ESM only)

```js
// modules/menu.js
export function initMenu() {
  const button = document.querySelector('[data-menu-toggle]');
  if (!button) return;

  button.addEventListener('click', () => {
    document.body.classList.toggle('menu-open');
  });
}
```

```js
// main.js
import { initMenu } from './modules/menu.js';

document.addEventListener('DOMContentLoaded', () => {
  initMenu();
});
```

### Rules

* `const` by default
* `let` only when reassigned
* Never use `var`
* Early returns preferred
* Arrow functions for callbacks

### DOM Access

* Use `data-*` attributes as JS hooks
* Never bind JS to CSS class names

```html
<button data-modal-open>Open</button>
```

---

## 5. Formatting & Consistency

### HTML

* 2-space indentation
* Self-closing tags for void elements
* Attributes on one line

### CSS

* Alphabetical property order (recommended)
* Avoid deep nesting (> 3 levels)

### JavaScript

* Semicolons required
* Max line length: 100 chars
* No unused variables

---

## 6. Performance Rules

* Use `type="module"` for JS
* Avoid blocking scripts
* Lazy-load images

```html
<script type="module" src="/js/main.js"></script>
<img src="image.jpg" loading="lazy" alt="" />
```

---

## 7. Static Site Generation Rules

* Vite and postcss is recommended but not strictly needed
* Output must be **pure HTML, CSS, JS**
* No runtime templating
* No hydration unless explicitly justified
* **Generated files must be checked into the repository**
* Build tools are allowed **only if they render static output**

---

## 8. Prohibited Practices

* Inline styles
* Inline scripts
* jQuery
* CSS-in-JS
* Client-side routing
* Hidden build complexity
* Uncommitted generated assets

---

## 9. Definition of Done

* HTML validates
* No console errors
* Works without JavaScript
* Accessible via keyboard
* Fully static and deployable via CDN

---

If you want next:

* Convert this into a **formal RFC**
* Enforce it via **lint configs**
* Adapt it for **Astro or Eleventy**
* Create a **PR checklist** based on this

Just say the word.

# IMPORTANT YOU MUST USE THESE RULES!!!!!

### 1) Must-have plugins

* **`postcss-import`**: allow `@import` in source CSS (bundles into one output).
* **`postcss-nesting`** (or `postcss-nested`): allow CSS nesting (keep it shallow).
* **`postcss-custom-media`**: define reusable breakpoints via `@custom-media`.
* **`postcss-preset-env`**: use modern CSS features safely (and optionally autoprefix).
* **`cssnano`** (production only): minify output.

### 2) House rules to enforce (team conventions)

* **Nesting max depth: 2–3**
* **No ID selectors**
* **No `!important`**
* **Colors/spacing/fonts must be CSS variables**
* **One breakpoint system** via `@custom-media` (no random `@media (min-width: 783px)`)

---

## `postcss.config.cjs` (baseline)

```js
// postcss.config.cjs
module.exports = ({ env }) => {
  const isProd = env === 'production';

  return {
    plugins: [
      require('postcss-import'),

      // Reusable breakpoints
      require('postcss-custom-media')({
        importFrom: ['src/styles/tokens.css'], // where @custom-media lives
      }),

      // Nesting (keep shallow, see stylelint section)
      require('postcss-nesting'),

      // Modern CSS features + optional prefixing
      require('postcss-preset-env')({
        stage: 2,
        autoprefixer: { grid: false },
        features: {
          'nesting-rules': false, // we already use postcss-nesting
        },
      }),

      // Minify only in production
      ...(isProd ? [require('cssnano')({ preset: 'default' })] : []),
    ],
  };
};
```

---

## `tokens.css` (where you define your “rules”)

```css
/* src/styles/tokens.css */

@custom-media --sm (min-width: 480px);
@custom-media --md (min-width: 768px);
@custom-media --lg (min-width: 1024px);

:root {
  --color-text: hsl(220 15% 20%);
  --color-primary: hsl(220 90% 56%);

  --space-xs: 0.25rem;
  --space-sm: 0.5rem;
  --space-md: 1rem;
  --space-lg: 1.5rem;

  --radius-sm: 0.375rem;
  --radius-md: 0.5rem;
}
```

Usage:

```css
@media (--md) {
  .layout {
    grid-template-columns: 2fr 1fr;
  }
}
```

---

## Stylelint rules (this is where “enforcement” really lives)

PostCSS transforms; **Stylelint enforces**. Here’s a ruleset that matches what you asked for:

```js
// .stylelintrc.cjs
module.exports = {
  extends: ['stylelint-config-standard'],
  plugins: ['stylelint-order'],
  rules: {
    'max-nesting-depth': 3,
    'selector-max-id': 0,
    'declaration-no-important': true,
    'selector-max-compound-selectors': 3,

    // Encourage variables for colors (best-effort)
    'color-named': 'never',
    'function-disallowed-list': [],

    // Optional: consistent ordering
    'order/properties-alphabetical-order': true,
  },
};
```

---
