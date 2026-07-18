import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';
import { VitePWA } from 'vite-plugin-pwa';
import path from 'path';

export default defineConfig({
  plugins: [
    solid(),
    VitePWA({
      registerType: 'autoUpdate',
      // Dev flow stays untouched: no SW or manifest in `bun run dev`.
      devOptions: { enabled: false },
      manifest: {
        name: 'Crucible',
        short_name: 'Crucible',
        description: 'Knowledge-grounded agent runtime',
        start_url: '/',
        scope: '/',
        display: 'standalone',
        // neutral-950 — matches the dark UI (`bg-neutral-950` on <body>)
        theme_color: '#0a0a0a',
        background_color: '#0a0a0a',
        icons: [
          { src: '/pwa-192x192.png', sizes: '192x192', type: 'image/png' },
          { src: '/pwa-512x512.png', sizes: '512x512', type: 'image/png' },
          {
            src: '/pwa-maskable-512x512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'maskable',
          },
        ],
      },
      workbox: {
        // Precache the app shell only. Oversized vendor chunks (shiki,
        // transformers) exceed the 2 MiB default and are intentionally
        // skipped — they load from network exactly as before.
        globPatterns: ['**/*.{js,css,html,svg,png,ico,woff2}'],
        // A new SW must take over immediately, or the old one keeps serving
        // the previous precache and users see stale bundles until every tab
        // is closed. registerType 'autoUpdate' reloads the page once the new
        // SW activates — these make that activation immediate.
        skipWaiting: true,
        clientsClaim: true,
        cleanupOutdatedCaches: true,
        // SPA navigation fallback, but NEVER for API paths. /api/* (including
        // SSE chat event streams) must hit the network untouched. generateSW
        // adds no other fetch routes since we define no runtimeCaching, so
        // this denylist closes the only path where the SW could respond to
        // an /api request.
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/api\//],
      },
    }),
  ],
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
    // Reset mock call-history and any vi.stubGlobal between tests so a mock or
    // global stubbed in one test can't leak into the next (several suites relied
    // on execution order for this). clearMocks keeps implementations, only
    // wiping recorded calls; unstubGlobals undoes stubGlobal.
    clearMocks: true,
    unstubGlobals: true,
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
        // Solid-consuming deps must be inlined so they share the test's
        // browser build of solid-js — externalized, they'd load the SSR build
        // and their reactivity silently dies (Key rows never updating).
        // @solidjs/* (testing-library) does NOT match /solid-js/ — leaving it
        // externalized breaks top-level structural reactivity in every test:
        // a component whose ROOT is a conditional <Show> mounts through the
        // SSR copy's insert and never re-renders when the signal flips.
        inline: [/solid-js/, /@solidjs\//, /@solid-primitives/],
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
        // Dev/test-only editor harness (served only in dev, never in dist).
        'src/test-harness/**',
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
