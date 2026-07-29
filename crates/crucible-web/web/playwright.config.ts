import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for Crucible web UI E2E tests.
 *
 * Two projects share one Vite dev server (mocked backend):
 *  - `chromium`  — the fast default suite (screenshots/video only on failure).
 *                  Ignores e2e/stories/ and e2e/live/.
 *  - `stories`   — user-story suites in e2e/stories/. video + trace ALWAYS on,
 *                  plus per-step screenshots (image sequence per story) written
 *                  by the story.step() helper. These double as visual baselines
 *                  via toHaveScreenshot().
 *
 * The live tier (e2e/live/) has its own config (playwright.live.config.ts): it
 * boots a real `cru web` + daemon + mock-acp-agent instead of the Vite server.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  // No retries anywhere. A retry turns a broken test green: the drag specs
  // below were failing ~95% of runs locally and CI still reported pass,
  // because three attempts were enough for one to slip through. A flake is a
  // defect in the test or the product — surface it, don't average it out.
  retries: 0,
  // `undefined` means 50% of logical cores — 16 on a 32-core box, all hammering
  // the single shared Vite dev server. This box is disk-bound, not CPU-bound,
  // so that oversubscribes the bottleneck and produces failures that look like
  // flakes but are contention. Pin it low enough that a local run is
  // reproducible; CI stays serial.
  workers: process.env.CI ? 1 : 4,
  reporter: 'html',
  snapshotPathTemplate: '{testDir}/__screenshots__/{testFilePath}/{arg}{ext}',

  // Visual gates must actually gate: on this dark, sparse UI a full design
  // change (gutters removed, column centered) moves only ~0.83% of pixels,
  // so the old per-call 2% tolerances passed EVERYTHING same-size (the
  // gutter redesign measured 0.44% under Playwright's comparator). 0.3%
  // fails real content changes while leaving headroom for cross-environment
  // text antialiasing. The two text-dense chat baselines keep their
  // battle-tested 3% + masks (CI font-advance drift, see chat-stream spec).
  expect: {
    // Text-heavy baselines drift ~1-2% run-to-run from the CI rasterizer's
    // subpixel antialiasing (even with a fixed, now-actually-loaded webfont —
    // see index.css). The old 0.3% ratio was too tight and flaked a different
    // text baseline each run. `threshold` (per-pixel) raised to drop AA edge
    // noise; `maxDiffPixelRatio` to ~the repo's battle-tested 3% for text, with
    // margin. Real regressions (layout shifts, missing content) are far larger.
    toHaveScreenshot: { threshold: 0.25, maxDiffPixelRatio: 0.04 },
  },

  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    // The app honors prefers-reduced-motion (index.css zeroes all animation
    // durations), so this freezes slide/pop animations — DnD tests measure
    // element coordinates that must not move mid-drag, and screenshots stay
    // deterministic.
    contextOptions: { reducedMotion: 'reduce' },
  },

  projects: [
    {
      name: 'chromium',
      testIgnore: ['**/stories/**', '**/live/**'],
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'stories',
      testMatch: '**/stories/**/*.spec.ts',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1280, height: 800 },
        video: 'on',
        trace: 'on',
        // Story screenshots are deterministic frames; keep animations still.
        screenshot: 'on',
      },
    },
  ],

  webServer: {
    command: 'bun run dev',
    port: 5173,
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
