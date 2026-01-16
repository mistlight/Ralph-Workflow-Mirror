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
