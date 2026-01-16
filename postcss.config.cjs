// postcss.config.cjs
module.exports = ({ env }) => {
  const isProd = env === 'production';

  return {
    plugins: [
      require('postcss-import'),

      // Reusable breakpoints
      require('postcss-custom-media')({
        importFrom: ['src/styles/tokens.css'],
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
