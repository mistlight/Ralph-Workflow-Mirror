import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  // Use relative paths for file:// protocol compatibility
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        '404': resolve(__dirname, '404.html'),
        faq: resolve(__dirname, 'faq.html'),
        'getting-started': resolve(__dirname, 'getting-started.html'),
        'how-it-works': resolve(__dirname, 'how-it-works.html'),
        'open-source': resolve(__dirname, 'open-source.html'),
        'og-image': resolve(__dirname, 'og-image.html'),
        'docs/overnight-runs': resolve(__dirname, 'docs/overnight-runs.html'),
        'docs/workflows': resolve(__dirname, 'docs/workflows.html'),
        'docs/writing-specs': resolve(__dirname, 'docs/writing-specs.html'),
      },
      output: {
        // Use consistent asset names without hashes for committed dist/
        entryFileNames: 'assets/main.js',
        chunkFileNames: 'assets/main.js',
        assetFileNames: (assetInfo) => {
          // CSS files get consistent name
          if (assetInfo.name?.endsWith('.css')) {
            return 'assets/main.css';
          }
          // SVG files keep their original names
          if (assetInfo.name?.endsWith('.svg')) {
            return 'assets/[name][extname]';
          }
          // For other assets
          if (assetInfo.name) {
            return 'assets/[name][extname]';
          }
          return 'assets/[name][extname]';
        },
        // Prevent automatic chunking to avoid hash suffixes
        manualChunks: () => 'main',
      },
    },
    assetsInlineLimit: 4096,
    cssMinify: true,
    sourcemap: false,
  },
  css: {
    postcss: './postcss.config.cjs',
    devSourcemap: false,
  },
  publicDir: 'assets',
  server: {
    port: 3000,
    open: true,
  },
});
