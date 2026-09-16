import { test, expect, request as playwrightRequest } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import path from 'node:path';
import { readState } from './_state';

/**
 * The lane's own gate: proof that what ran is what this tree built.
 *
 * Every other live spec asserts about the product. These assert about the
 * STAND: that the daemon is the binary the recipe compiled, that the page is
 * the bundle the recipe built, that the daemon's environment is the tier's own
 * and not the developer's, and that the model turns landed on the fake server.
 *
 * Without this file the tier can pass while proving nothing. It passed against
 * an installed `cru` on PATH for as long as the setup was willing to look
 * there, and it would pass against last week's bundle any time a build step
 * was skipped — a green run for code that was never executed.
 */
const state = readState();

/** Linux only: the assertions read the process table through /proc. */
const HAS_PROC = existsSync('/proc/self/environ');

/** Every pid whose environment names this run's daemon socket. */
function pidsOnSocket(socket: string): number[] {
  const found: number[] = [];
  for (const entry of readdirSync('/proc')) {
    if (!/^\d+$/.test(entry)) continue;
    let environ: string;
    try {
      environ = readFileSync(`/proc/${entry}/environ`, 'utf-8');
    } catch {
      continue; // another user's process, or one that exited mid-scan
    }
    if (environ.split('\0').includes(`CRUCIBLE_SOCKET=${socket}`)) found.push(Number(entry));
  }
  return found;
}

function environOf(pid: number): Map<string, string> {
  const map = new Map<string, string>();
  for (const pair of readFileSync(`/proc/${pid}/environ`, 'utf-8').split('\0')) {
    if (!pair) continue;
    const eq = pair.indexOf('=');
    if (eq > 0) map.set(pair.slice(0, eq), pair.slice(eq + 1));
  }
  return map;
}

test.describe('live lane guard', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('the page served is the bundle this tree built', async () => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });

    // The stamp exists only in the dist the setup verified and wrote into.
    // The bundle rust-embed bakes into the binary carries no such file, so a
    // server that fell back to it answers 404 here.
    const stamp = await api.get('/live-stamp.json');
    expect(stamp.status()).toBe(200);
    const served = (await stamp.json()) as { stampId?: string; cruBin?: string };
    expect(served.stampId).toBe(state.stampId);
    expect(served.cruBin).toBe(realpathSync(state.cruBin!));

    // And the HTML itself is that directory's, byte for byte.
    const page = await api.get('/index.html');
    expect(page.status()).toBe(200);
    expect(await page.text()).toBe(readFileSync(path.join(state.distDir!, 'index.html'), 'utf-8'));

    await api.dispose();
  });

  test('the web server and the daemon are the binary this tree built', async () => {
    test.skip(!HAS_PROC, 'requires: /proc (Linux process table)');

    const pids = pidsOnSocket(state.socket!);
    expect(pids).toContain(state.webPid);

    const expected = realpathSync(state.cruBin!);
    expect(realpathSync(`/proc/${state.webPid}/exe`), 'the web server is a different binary').toBe(
      expected,
    );

    // Every `cru` on this socket is THIS `cru`. The filter is not a loophole:
    // an installed binary on PATH resolves to its own path and fails here. It
    // is there because the daemon also spawns helpers that inherit its
    // environment — an embedding host, a plugin process — and those are not
    // `cru` and are not what this asserts.
    const cruPids = pids.filter((pid) => {
      try {
        return path.basename(realpathSync(`/proc/${pid}/exe`)) === 'cru';
      } catch {
        return false;
      }
    });
    // Two at least: `cru web` and the daemon it auto-started.
    expect(cruPids.length, 'no daemon process found beside the web server').toBeGreaterThanOrEqual(2);
    for (const pid of cruPids) {
      expect(realpathSync(`/proc/${pid}/exe`), `pid ${pid} runs a different cru`).toBe(expected);
    }
  });

  test('the daemon runs in the tier\'s own environment, not the developer\'s', async () => {
    test.skip(!HAS_PROC, 'requires: /proc (Linux process table)');

    const pids = pidsOnSocket(state.socket!).filter((p) => p !== state.webPid);
    expect(pids.length).toBeGreaterThanOrEqual(1);
    const tmpDir = state.tmpDir!;

    for (const pid of pids) {
      const env = environOf(pid);
      // The four roots that decide where state lands. Each must sit inside
      // this run's temp directory: a leak here writes the developer's real
      // ~/.crucible, and the next run inherits whatever this one wrote.
      for (const key of ['HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_RUNTIME_DIR', 'CRUCIBLE_HOME']) {
        const value = env.get(key);
        expect(value, `${key} is unset in the daemon`).toBeTruthy();
        expect(value!.startsWith(tmpDir), `${key}=${value} is outside ${tmpDir}`).toBe(true);
      }
      // No credential can reach a provider that is not the fake.
      for (const key of [
        'ANTHROPIC_API_KEY', 'OPENAI_API_KEY', 'GEMINI_API_KEY', 'GOOGLE_API_KEY',
        'OPENROUTER_API_KEY', 'GLM_AUTH_TOKEN', 'ZAI_API_KEY', 'GITHUB_TOKEN',
        'OLLAMA_HOST', 'HTTP_PROXY', 'HTTPS_PROXY', 'http_proxy', 'https_proxy',
      ]) {
        expect(env.has(key), `${key} leaked into the daemon`).toBe(false);
      }
    }
  });

  test('a real turn lands on the fake model server and nowhere else', async () => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });

    // The daemon's own answer about which provider it resolved. The setup
    // asserts this too; repeating it here means a spec failure names the cause
    // instead of leaving "the reply never arrived".
    const shown = execFileSync(state.cruBin!, ['config', 'show', '-f', 'json'], {
      cwd: state.kilnDir,
      env: state.childEnv,
      timeout: 60_000,
      stdio: ['ignore', 'pipe', 'pipe'],
    }).toString();
    const llm = (JSON.parse(shown) as { llm?: { default?: string; providers?: Record<string, { endpoint?: string }> } }).llm;
    expect(llm?.default).toBe('ollama');
    expect(llm?.providers?.ollama?.endpoint).toBe(`http://127.0.0.1:${state.fakeOllamaPort}`);

    const created = await api.post('/api/session', {
      data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
    });
    expect(created.status(), await created.text()).toBe(200);
    const sessionId = ((await created.json()) as { session_id: string }).session_id;

    const marker = `hermetic guard ${Date.now()}`;
    const sent = await api.post('/api/chat/send', {
      data: { session_id: sessionId, content: marker },
    });
    expect(sent.status(), await sent.text()).toBe(200);

    // The fake writes one line per request it answered. The marker appearing
    // there is the proof that the turn reached IT — a turn that had gone to a
    // real provider would leave this file untouched.
    await expect
      .poll(
        () => (existsSync(state.fakeLogPath!) ? readFileSync(state.fakeLogPath!, 'utf-8') : ''),
        { timeout: 30_000, message: 'the fake model server never saw the turn' },
      )
      .toContain(marker);

    await api.dispose();
  });
});
