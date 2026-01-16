// .stylelintrc.cjs
module.exports = {
  extends: ['stylelint-config-standard'],
  plugins: ['stylelint-order'],
  rules: {
    'max-nesting-depth': 3,
    'selector-max-id': 0,
    'declaration-no-important': true,
    'selector-max-compound-selectors': 4, // Increased from 3 to allow more specific selectors

    // Encourage variables for colors (best-effort)
    'color-named': 'never',
    'function-disallowed-list': [],

    // Allow BEM-style modifiers with double hyphen
    'selector-class-pattern': null,
    'keyframes-name-pattern': null,
    'custom-media-pattern': null,

    // Allow single-line declaration blocks with multiple declarations
    'declaration-block-single-line-max-declarations': null,

    // Disable specificity and ordering warnings that can be ignored
    'no-descending-specificity': null,
    'declaration-block-no-duplicate-properties': null, // Allow for browser fallbacks

    // Allow modern CSS properties that may not be fully recognized
    'declaration-property-value-no-unknown': null,

    // Optional: consistent ordering
    'order/properties-alphabetical-order': true,
  },
};
