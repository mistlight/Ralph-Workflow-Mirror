# CLAUDE.md

# Modern Static Site Style Guide

*(HTML · CSS · JavaScript — SSG-friendly)*

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
