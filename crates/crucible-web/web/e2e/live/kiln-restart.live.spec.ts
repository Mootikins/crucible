import { test, expect, request as playwrightRequest } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { readState } from './_state';

/**
 * A registered kiln is reachable after a daemon restart.
 *
 * RED as written. The daemon restarts cleanly and keeps its registrations —
 * `kiln.list` answers with the kiln, `registered: true` — but nothing re-opens
 * it, and the write admission takes its roots from the kilns the manager holds
 * OPEN (`file_write.rs`, `km.list()`), not from the registry. So the browser is
 * offered a kiln whose every write is refused, with a 404 that says "File not
 * within any open kiln or registered project" about a kiln the same daemon has
 * just listed.
 *
 * This is the whole of the restart cluster: the conflict leg stops the daemon
 * on purpose, and every kiln-addressed write after it in the run fails. The
 * fix belongs to the daemon — open a registered kiln on demand, or re-open the
 * registrations at boot — and this test is the gate for it.
 *
 * It restores what it broke: the kilns are re-opened before it returns, so a
 * later spec in this tier meets the stack the setup established.
 */
const state = readState();

test.describe.configure({ timeout: 120_000 });

function reopenKilns(): void {
  for (const dir of [state.kilnDir!, state.secondKilnDir!]) {
    execFileSync(state.cruBin!, ['process'], {
      cwd: dir,
      env: state.childEnv,
      stdio: 'ignore',
      timeout: 120_000,
    });
  }
}

test.describe('a kiln survives a daemon restart', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('a note write lands in a registered kiln after the daemon restarts', async () => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const kiln = state.kilnDir!;

    // It works before the restart. Without this the test could pass on a
    // stack where note writes never worked at all.
    const before = await api.put('/api/notes/RestartBefore', {
      data: { kiln, content: '# RestartBefore\n\nwritten before the restart\n' },
    });
    expect(before.status(), await before.text()).toBe(200);

    execFileSync(state.cruBin!, ['daemon', 'stop'], {
      env: state.childEnv,
      stdio: 'ignore',
      timeout: 20_000,
    });
    // The web process starts a fresh daemon on its next call. Wait for that,
    // not for a fixed delay.
    await expect
      .poll(async () => (await api.get('/api/kilns')).status(), {
        timeout: 30_000,
        message: 'the web process never reconnected to a fresh daemon',
      })
      .toBe(200);

    try {
      // The fresh daemon still has the registration — this is what makes the
      // write's refusal a contradiction rather than a missing kiln.
      const listed = (await (await api.get('/api/kilns')).json()) as {
        kilns: { path: string; name: string | null; registered?: boolean }[];
      };
      const row = listed.kilns.find((k) => k.path === kiln);
      expect(row, `the daemon stopped listing ${kiln}: ${JSON.stringify(listed)}`).toBeTruthy();
      expect(row!.registered, 'the registration did not survive the restart').toBe(true);

      // And the write must land, because the kiln is registered. Today this is
      // 404 "File not within any open kiln or registered project".
      const after = await api.put('/api/notes/RestartAfter', {
        data: { kiln, content: '# RestartAfter\n\nwritten after the restart\n' },
      });
      expect(after.status(), await after.text()).toBe(200);
    } finally {
      reopenKilns();
      await api.dispose();
    }
  });
});
