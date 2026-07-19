import { defineConfig, devices } from '@playwright/test';

/**
 * Live-tier Playwright config (WS-201/202/205/206).
 *
 * Boots a REAL `cru web` + auto-spawned daemon against an isolated Unix socket
 * and a TempDir kiln (globalSetup), then tears the process tree down
 * (globalTeardown). No Vite webServer — the specs hit the live server whose URL
 * is published by globalSetup into e2e/live/.live-state.json.
 *
 * If no `cru` binary is found, globalSetup writes { skip: true } and every spec
 * skips cleanly (see e2e/live/_state.ts).
 *
 * Deterministic by construction: these specs exercise the kiln/notes endpoints
 * (daemon → filesystem), which need no LLM. Live streaming/permission (WS-101/
 * 104) are intentionally NOT here — the web session route hardcodes the internal
 * agent, so the mock-acp-agent is unreachable and there is no deterministic
 * in-tree provider; those flows are covered at the mock tier.
 */
export default defineConfig({
  testDir: './e2e/live',
  testMatch: '**/*.live.spec.ts',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: 'line',
  timeout: 30_000,
  globalSetup: './e2e/live/global-setup.ts',
  globalTeardown: './e2e/live/global-teardown.ts',

  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    ...devices['Desktop Chrome'],
  },
});
