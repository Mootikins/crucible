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
      // No-regression gate. Values are floor() of the post-A2 baseline.
      // Raise these as new tests land; never lower without a written reason.
      // See thoughts/shared/research/2026-05-17-web-coverage-baseline.md.
      //
      // Per-file thresholds policy: add a file here ONLY after it has
      // organically achieved the level. Do not aspire here — per-file gates
      // failing in CI for a new file someone is still developing creates
      // friction without value. The global floor catches accidental
      // regressions everywhere; per-file gates pin specific hard-won wins.
      thresholds: {
        // Post-Phase C baseline (May 2026). Floor() of the measured run.
        statements: 49,
        branches: 42,
        functions: 46,
        lines: 51,
        'src/contexts/chatEventReducer.ts': {
          lines: 95,
          branches: 90,
          functions: 95,
          statements: 95,
        },
        'src/lib/api.ts': {
          lines: 95,
          branches: 85,
          functions: 95,
          statements: 95,
        },
        // Phase C-1 component backfill — pin the wins so future PRs can't
        // silently regress these chat-path components.
        'src/components/ToolCard.tsx': {
          lines: 95,
          branches: 90,
          functions: 90,
          statements: 95,
        },
        'src/components/DiffViewer.tsx': {
          lines: 95,
          branches: 85,
          functions: 95,
          statements: 95,
        },
        'src/components/NotificationCenter.tsx': {
          lines: 95,
          branches: 80,
          functions: 90,
          statements: 95,
        },
        'src/components/CommandPalette.tsx': {
          lines: 90,
          branches: 85,
          functions: 90,
          statements: 90,
        },
        'src/components/Message.tsx': {
          lines: 90,
          branches: 85,
          functions: 90,
          statements: 90,
        },
      },
    },
  },
});
