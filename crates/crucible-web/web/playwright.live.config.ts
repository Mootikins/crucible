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
 * THREE projects, because "live" now means three different things:
 *
 *  - `live`   — e2e/live/*.live.spec.ts: the kiln/notes endpoint suite.
 *  - `live-compact` — the conflict leg again, on a phone-shaped viewport.
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
  // Two retries, LIVE tier only — diagnostic, not masking. WS-206 fails a
  // measured 1-to-3 runs in 30 on a quiet box (see the NOTE(finding) in
  // kiln-truth.live.spec.ts), which made every red CI run ambiguous: a real
  // regression and the flake produced the same top-line result, so the gate
  // stopped distinguishing what it exists to distinguish.
  //
  // With retries the failure is CLASSIFIED rather than hidden. A genuine
  // never-indexes bug fails all three attempts and CI stays red; a transient
  // race passes on a retry, and Playwright reports that as "flaky", distinct
  // from "passed", so the signal survives in the run output. A "flaky" line
  // for WS-206 IS this failure firing, and re-opens the investigation — it is
  // not CI hygiene to tune away.
  //
  // The unit, e2e and Rust tiers stay zero-retry: nothing there is eventually
  // consistent, so a retry would genuinely mask a defect.
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
    {
      // The conflict leg again, on a phone-shaped viewport. A phone is where
      // an offline write is made, so a phone is where a conflict is met — and
      // the two shells share no chrome: the compact shell draws its own app
      // bar, its own save affordance and one content surface at a time. The
      // width alone decides the shell (`stores/deviceStore.ts`, 767px), once,
      // at load; `isMobile` is deliberately not set, because the leg types on
      // a keyboard and touch emulation would only take that away.
      name: 'live-compact',
      testMatch: '**/conflict.live.spec.ts',
      use: { ...devices['Desktop Chrome'], viewport: { width: 390, height: 844 } },
    },
    { name: 'served', testDir: './tests', testMatch: '**/*.pw.ts' },
  ],
});
