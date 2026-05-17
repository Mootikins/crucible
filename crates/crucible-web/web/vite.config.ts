import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';
import path from 'path';

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
    conditions: ['development', 'browser'],
    dedupe: ['solid-js', 'solid-js/web', 'solid-js/store'],
  },
  server: {
    port: 5173,
    host: true, // Listen on all interfaces
    allowedHosts: ['localhost'],
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      },
    },
  },
  optimizeDeps: {},
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
  test: {
    environment: 'jsdom',
    globals: true,
    exclude: ['e2e/**', 'node_modules/**'],
    setupFiles: ['./src/test/setup.ts'],
    deps: {
      optimizer: {
        web: {
          include: ['solid-js'],
        },
      },
    },
    server: {
      deps: {
        inline: [/solid-js/],
      },
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'json-summary'],
      reportsDirectory: './coverage',
      exclude: [
        'e2e/**',
        'src/test/**',
        'src/test-utils/**',
        'src/**/*.d.ts',
        'src/**/index.ts',
        '*.config.ts',
        '*.config.js',
        'postcss.config.js',
        'src/solid-jsx.d.ts',
        // NOTE: do NOT exclude src/types/** — those files contain pure type
        // definitions today (v8 reports 0/0, no cost) but if runtime helpers
        // ever land there we want them measured.
      ],
      // No-regression gate. Values are floor() of the 2026-05-17 baseline.
      // Raise these as new tests land; never lower without a written reason.
      // See thoughts/shared/research/2026-05-17-web-coverage-baseline.md.
      thresholds: {
        statements: 27,
        branches: 22,
        functions: 21,
        lines: 31,
      },
    },
  },
});
