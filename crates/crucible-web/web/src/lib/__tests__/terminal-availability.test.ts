import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const getConfigMock = vi.fn();

vi.mock('../api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getConfig: () => getConfigMock(),
}));

/**
 * `remoteShell` and the in-flight handle are module state that outlives a single
 * test, so every case imports a fresh copy. Without this the first test's answer
 * decides the rest.
 */
async function freshModule() {
  vi.resetModules();
  return import('../terminal-availability');
}

/** Non-loopback, so the remote branch actually runs. */
function browsingFromTheLan() {
  vi.stubGlobal('location', { ...window.location, hostname: 'box.example.test' });
}

beforeEach(() => {
  vi.clearAllMocks();
  browsingFromTheLan();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('terminal availability', () => {
  it('does not latch as denied when the config request fails', async () => {
    // On a LAN page load the first /api/config is a 401 — the API group sits
    // behind bearer auth. Collapsing that onto "denied" made the terminal claim
    // to be localhost-only for the life of the page.
    getConfigMock.mockRejectedValueOnce(new Error('HTTP 401'));
    const { terminalAllowed, terminalDenied } = await freshModule();

    expect(terminalAllowed()).toBe(false); // fail-closed while unknown
    await vi.waitFor(() => expect(getConfigMock).toHaveBeenCalledTimes(1));

    // The distinction that matters: unknown, NOT denied. `denied` is what
    // renders "only available from the host machine".
    await vi.waitFor(() => expect(terminalDenied()).toBe(false));
    expect(terminalAllowed()).toBe(false);
  });

  it('does not re-ask on every read while the answer is unknown', async () => {
    // These accessors are read from render paths. Retrying without a throttle
    // turned one failed check into a request per read.
    getConfigMock.mockRejectedValue(new Error('HTTP 401'));
    const { terminalAllowed } = await freshModule();

    for (let i = 0; i < 20; i++) terminalAllowed();
    await vi.waitFor(() => expect(getConfigMock).toHaveBeenCalledTimes(1));
    expect(getConfigMock).toHaveBeenCalledTimes(1);
  });

  it('re-checks when a sign-in succeeds', async () => {
    // `mockResolvedValue`, not `...Once`: see the note below about earlier
    // module copies also reacting — a one-shot value gets consumed by one of
    // them and the module under test sees nothing.
    getConfigMock
      .mockRejectedValueOnce(new Error('HTTP 401'))
      .mockResolvedValue({ remote_shell: true });
    const { terminalAllowed } = await freshModule();

    terminalAllowed();
    await vi.waitFor(() => expect(getConfigMock).toHaveBeenCalledTimes(1));

    // What `login()` dispatches on success. Covers the case the token prompt's
    // reload does not: dismissed prompt, authenticated later or in another tab.
    window.dispatchEvent(new CustomEvent('crucible:auth-ok'));

    // Asserted as "more than before" rather than an exact count: the listener is
    // registered at module scope, and `vi.resetModules()` leaves each earlier
    // copy's listener attached to the same `window`, so every module instance
    // this file has imported also reacts. That is a harness artifact — in the
    // app the module is a singleton.
    await vi.waitFor(() => expect(getConfigMock.mock.calls.length).toBeGreaterThan(1));
    await vi.waitFor(() => expect(terminalAllowed()).toBe(true));
  });

  it('takes a real "no" as an answer and stops asking', async () => {
    // `remote_shell: false` IS a denial — unlike a transport failure — and the
    // panel should say so rather than retrying in a loop.
    getConfigMock.mockResolvedValue({ remote_shell: false });
    const { terminalAllowed, terminalDenied } = await freshModule();

    terminalAllowed();
    await vi.waitFor(() => expect(terminalDenied()).toBe(true));
    expect(terminalAllowed()).toBe(false);
  });

  it('never asks at all from localhost', async () => {
    vi.stubGlobal('location', { ...window.location, hostname: 'localhost' });
    const { terminalAllowed, terminalDenied } = await freshModule();

    expect(terminalAllowed()).toBe(true);
    expect(terminalDenied()).toBe(false);
    expect(getConfigMock).not.toHaveBeenCalled();
  });
});
