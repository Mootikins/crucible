import { defineConfig, devices } from '@playwright/test';

/**
 * Hero-flow Playwright config — the single cross-surface journey (US/WS-HERO).
 *
 * hero-setup boots a fake Ollama server (deterministic turns) + a real isolated
 * daemon + `cru web` + a temp kiln, and publishes state to
 * e2e/live/.hero-state.json. The specs then drive TUI legs (via the compiled
 * tui_e2e_tests binary) and the web console against that one live stack.
 * hero-teardown reaps the process tree and closes the fake server.
 *
 * Two specs share this one stack: hero.live.spec (the 3-console session
 * journey) and agent-fs.live.spec (the agent-writes-a-file journey, with a
 * real permission approval, through both consoles). `workers: 1` +
 * `fullyParallel: false` keep them from fighting over the single daemon.
 *
 * If no `cru` binary is found, hero-setup writes { skip:true } and the specs
 * skip cleanly. Runs serial (one shared session/VM).
 */
export default defineConfig({
  testDir: './e2e/live',
  testMatch: ['**/hero.live.spec.ts', '**/agent-fs.live.spec.ts'],
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
