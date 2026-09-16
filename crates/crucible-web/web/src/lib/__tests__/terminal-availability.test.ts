import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetConfigForTests } from '@/lib/query/config';
import { login } from '@/lib/api';
import {
  installAuthOkListener,
  terminalAllowed,
  terminalDenied,
} from '../terminal-availability';

/**
 * The module holds no state of its own any more: it reads the shared config
 * query, so a case starts from a fresh client and an empty local cache rather
 * than from a fresh copy of the module.
 *
 * The real `/api/config` runs against the mock, so these cases prove what the
 * daemon's own answer does to the two accessors.
 */
let env: TestQueryEnv;

/** Non-loopback, so the remote branch actually runs. */
function browsingFromTheLan() {
  vi.stubGlobal('location', { ...window.location, hostname: 'box.example.test' });
}

beforeEach(() => {
  localStorage.clear();
  resetConfigForTests();
  browsingFromTheLan();
  // The previous case's `restore()` cleared the bus, which took the handler
  // the module subscribed at import.
  installAuthOkListener();
});

afterEach(() => {
  env?.restore();
  resetConfigForTests();
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe('terminal availability', () => {
  it('does not latch as denied when the config request fails', async () => {
    // On a LAN page load the first /api/config is a 401 — the API group sits
    // behind bearer auth. Collapsing that onto "denied" made the terminal claim
    // to be localhost-only for the life of the page.
    env = createTestQueryEnv({
      'GET /api/config': apiError(401, 'sign in with the API key'),
    });

    expect(terminalAllowed()).toBe(false); // fail-closed while unknown
    await vi.waitFor(() => expect(env.fetch.calls('GET /api/config')).toBe(1));

    // The distinction that matters: unknown, NOT denied. `denied` is what
    // renders "only available from the host machine".
    await vi.waitFor(() => expect(terminalDenied()).toBe(false));
    expect(terminalAllowed()).toBe(false);
  });

  it('does not re-ask on every read while the answer is unknown', async () => {
    // These accessors are read from render paths. The query answers every one
    // of them from its cache; the module holds no timer that asks again.
    env = createTestQueryEnv({
      'GET /api/config': apiError(401, 'sign in with the API key'),
    });

    for (let i = 0; i < 20; i++) terminalAllowed();
    await vi.waitFor(() => expect(env.fetch.calls('GET /api/config')).toBe(1));
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });

  it('re-checks when a sign-in succeeds', async () => {
    let signedIn = false;
    env = createTestQueryEnv({
      'GET /api/config': () =>
        signedIn
          ? { kiln_path: '/kilns/main', remote_shell: true }
          : new Response(JSON.stringify({ error: { code: 401, message: 'sign in' } }), {
              status: 401,
              headers: { 'Content-Type': 'application/json' },
            }),
      'POST /api/auth/login': { status: 200 },
    });

    // The refusal has to LAND before the sign-in, which is the order the app
    // has: the 401 is what raises the token prompt the user then answers.
    terminalAllowed();
    await vi.waitFor(() => expect(terminalAllowed()).toBe(false));
    await vi.waitFor(() => expect(env.client.getQueryState(['config'])?.status).toBe('error'));

    // The real `login()`, not a hand-made event: the emit site and this
    // listener must agree on one name, and only the real call proves it.
    // Covers the case the token prompt's reload does not: dismissed prompt,
    // authenticated later or in another tab.
    signedIn = true;
    expect(await login('the-key')).toBe(true);

    await vi.waitFor(() => expect(terminalAllowed()).toBe(true));
    expect(env.fetch.calls('GET /api/config')).toBe(2);
  });

  it('takes a real "no" as an answer and stops asking', async () => {
    // `remote_shell: false` IS a denial — unlike a transport failure — and the
    // panel should say so rather than retrying in a loop.
    env = createTestQueryEnv({
      'GET /api/config': () => ({ kiln_path: '/kilns/main', remote_shell: false }),
    });

    terminalAllowed();
    await vi.waitFor(() => expect(terminalDenied()).toBe(true));
    expect(terminalAllowed()).toBe(false);
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });

  it('never asks at all from localhost', async () => {
    vi.stubGlobal('location', { ...window.location, hostname: 'localhost' });
    env = createTestQueryEnv({
      'GET /api/config': () => ({ kiln_path: '/kilns/main', remote_shell: false }),
    });

    expect(terminalAllowed()).toBe(true);
    expect(terminalDenied()).toBe(false);
    expect(env.fetch.calls('GET /api/config')).toBe(0);
  });
});
