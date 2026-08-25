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
 *
 * TWO projects, because "live" now means two different things:
 *
 *  - `live`   — e2e/live/*.live.spec.ts: the kiln/notes endpoint suite.
 *  - `served` — tests/*.pw.ts: the BUILT bundle as the Rust server hands it
 *               over, headers and all. The mock tier cannot cover these at
 *               all: its baseURL is the Vite dev server, which emits no CSP,
 *               no nosniff and no Referrer-Policy and never touches an axum
 *               route — so a CSP that breaks the product passes there
 *               unchanged. Anything asserting on response headers, on a CSP
 *               violation, or on a file the daemon serves belongs here.
 *
 * `*.pw.ts`, not `*.spec.ts`: vitest's include is `**\/*.{test,spec}.*` with
 * only `e2e/**` excluded, so a `.spec.ts` under tests/ would be swept into
 * `just web-test unit` and fail on the Playwright import.
 */
export default defineConfig({
  testDir: './e2e/live',
  testMatch: '**/*.live.spec.ts',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  // The live tier is the only one that waits on eventual consistency, and
  // WS-206 has a measured ~1-3-in-30 failure on a quiet box (see the
  // NOTE(finding) in kiln-truth.live.spec.ts). Without retries a real
  // regression and that flake produce the same red, so the gate stops
  // distinguishing what it exists to distinguish.
  //
  // These retries DIAGNOSE rather than mask. A genuine break fails all three
  // attempts and CI stays red; a transient race passes one. Playwright
  // reports a retried pass as "flaky", distinct from "passed", so the signal
  // survives in the run output.
  //
  // A "flaky" line for WS-206 is the cue to re-open that finding, not noise
  // to tune away. No other tier gets retries: nothing else here is eventually
  // consistent, so a retry elsewhere would genuinely hide a defect.
  retries: 2,
  reporter: 'line',
  timeout: 30_000,
  globalSetup: './e2e/live/global-setup.ts',
  globalTeardown: './e2e/live/global-teardown.ts',

  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    ...devices['Desktop Chrome'],
  },

  projects: [
    {
      name: 'live',
      // The hero-flow specs also live in e2e/live and match *.live.spec.ts,
      // but they belong to playwright.hero.config.ts: they need hero-setup
      // (fake Ollama + .hero-state.json) and the compiled TUI test binary,
      // neither of which this config provides. Swept in here they read
      // whatever stale .hero-state.json a previous `just web-test hero` run left
      // behind and fail against its dead temp dirs.
      testIgnore: ['**/hero.live.spec.ts', '**/agent-fs.live.spec.ts'],
    },
    { name: 'served', testDir: './tests', testMatch: '**/*.pw.ts' },
  ],
});
