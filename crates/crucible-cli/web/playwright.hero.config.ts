import { defineConfig, devices } from '@playwright/test';

/**
 * Hero-flow Playwright config — the single cross-surface journey (US/WS-HERO).
 *
 * hero-setup boots a fake Ollama server (deterministic turns) + a real isolated
 * daemon + `cru web` + a temp kiln, and publishes state to
 * e2e/live/.hero-state.json. The spec then drives TUI legs (via the compiled
 * tui_e2e_tests binary) and the web console against that one live stack.
 * hero-teardown reaps the process tree and closes the fake server.
 *
 * If no `cru` binary is found, hero-setup writes { skip:true } and the spec
 * skips cleanly. Runs serial (one shared session/VM).
 */
export default defineConfig({
  testDir: './e2e/live',
  testMatch: '**/hero.live.spec.ts',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: 'line',
  timeout: 300_000,
  globalSetup: './e2e/live/hero-setup.ts',
  globalTeardown: './e2e/live/hero-teardown.ts',

  use: {
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
    screenshot: 'only-on-failure',
    ...devices['Desktop Chrome'],
  },
});
