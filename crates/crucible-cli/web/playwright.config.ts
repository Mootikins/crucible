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
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  snapshotPathTemplate: '{testDir}/__screenshots__/{testFilePath}/{arg}{ext}',

  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
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
