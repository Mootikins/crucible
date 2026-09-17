import { test, expect, request as playwrightRequest, type APIRequestContext, type Page } from '@playwright/test';
import { spawn } from 'node:child_process';
import net from 'node:net';
import { appReady, openSessionsList } from '../helpers/nav';
import { readState } from './_state';
import { apiQuiet, captureApiRequests, describeRequests, installEventSourceSpy, sourcesFor } from './_requests';
import { centerGroupIds, mountChat, resetStoredLayout } from './_panes';

/**
 * Task G5, live: the seq cursor turns a broken connection and a page reload
 * into a replay instead of a loss.
 *
 * Both specs drive a REAL turn — the fake model streams a long reply one word
 * every 120ms (`spin the slow wheel`, `global-setup.ts`), so the turn is still
 * running across the disconnect.
 *
 * The disconnect in (a) is the server dying: the spec runs its own `cru web`
 * against the tier's daemon and kills it mid-turn, so the browser's
 * EventSource gets a real connection reset (an `onerror`, not a silent stall —
 * `setOffline` exempts loopback and never fires one). The pane stays mounted,
 * the transcript store survives, and everything the outage swallows must come
 * back through the replay. The outage is a MEASURED DURATION, not a wait for
 * a condition: its length is the parameter that says how much the cursor has
 * to recover.
 *
 * WHAT THESE SPECS CATCH, measured by breaking the route on purpose:
 *
 * - Reading the replay BEFORE subscribing the broker receiver (the ordering
 *   `chat.rs` calls load-bearing) fails (a): the turn's remaining events are
 *   emitted into the window where no channel exists, nothing replays them,
 *   and the reply never reaches its last words. This is the mutation these
 *   specs exist for — no cheaper tier can see it, because it needs a real
 *   producer running against a real outage.
 * - Deleting the `id:` stamping does NOT fail either spec, and that is not a
 *   hole to paper over with a cleverer assertion. The store's identity dedup
 *   absorbs a replayed turn, and a history refetch arms the cursor to a
 *   nonzero seq on its own, so the DOM is identical. The stamping is gated
 *   where it is observable instead: `tests/route_contract_tests/chat.rs`
 *   (three cases) reads the frame ids off the wire.
 */
const state = readState();

test.describe.configure({ timeout: 240_000 });

const OUTAGE_MS = 4_000;
const SLOW_REPLY_END = 'the day is done';

/** The spec's own web server, so the connection can be killed for real. */
class OwnServer {
  private child: ReturnType<typeof spawn> | null = null;

  constructor(readonly port: number) {}

  get baseURL(): string {
    return `http://127.0.0.1:${this.port}`;
  }

  async start(): Promise<void> {
    this.child = spawn(
      state.cruBin!,
      [
        'web', '--host', '127.0.0.1', '--port', String(this.port),
        '--static-dir', state.distDir!,
      ],
      // The tier's isolated env, so the server lands on the tier's daemon
      // socket and serves the bundle this run built.
      { cwd: state.kilnDir!, env: state.childEnv, stdio: 'ignore', detached: true },
    );
    this.child.unref();
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      try {
        const probe = await fetch(`${this.baseURL}/api/config`);
        if (probe.ok) return;
      } catch {
        /* not up yet */
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    throw new Error(`own cru web never became ready on ${this.port}`);
  }

  /** Kills the web process alone — never the process group, never the daemon. */
  kill(): void {
    if (this.child?.pid) {
      try {
        process.kill(this.child.pid, 'SIGKILL');
      } catch {
        /* already gone */
      }
    }
    this.child = null;
  }
}

async function freePort(): Promise<number> {
  const { promise, resolve } = Promise.withResolvers<number>();
  const server = net.createServer();
  server.listen(0, '127.0.0.1', () => {
    const port = (server.address() as net.AddressInfo).port;
    server.close(() => resolve(port));
  });
  return promise;
}

async function createSession(api: APIRequestContext, title: string): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  const id = ((await created.json()) as { session_id: string }).session_id;
  await api.put(`/api/session/${id}/title`, { data: { title } });
  return id;
}

/** Sends a turn from the composer of the first pane. */
async function send(page: Page, text: string): Promise<void> {
  const input = page.getByTestId('chat-input').first();
  await input.fill(text);
  await page.getByTestId('send-button').first().click();
}

/** The outage itself: the server stays down for exactly this long. */
async function outage(ms: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  await promise;
}

test.describe('live seq-cursor replay', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test.beforeEach(async () => {
    await resetStoredLayout(state.baseURL!);
  });

  test.afterAll(async () => {
    if (!state.skip) await resetStoredLayout(state.baseURL!);
  });

  test('a mid-turn disconnect reconnects from the cursor and loses nothing', async ({ page }) => {
    const server = new OwnServer(await freePort());
    await server.start();
    try {
      const api = await playwrightRequest.newContext({ baseURL: server.baseURL });
      const id = await createSession(api, 'Reconnect me');

      await installEventSourceSpy(page);
      const log = captureApiRequests(page);
      await page.goto(server.baseURL);
      await appReady(page);
      await openSessionsList(page);
      const groups = await centerGroupIds(page);
      await mountChat(page, groups[0], id, `tab-chat-${id}`);
      await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
      await apiQuiet(log);
      log.reset();

      // A first turn that completes normally: whatever the reconnect replays
      // later, this turn's events were already applied with their seqs.
      await send(page, 'a hermetic first turn');
      await expect(page.getByTestId('message-assistant').first()).toContainText(
        'The live chain answered.',
        { timeout: 60_000 },
      );
      await apiQuiet(log);
      log.reset();

      // The turn that gets disconnected: it is still streaming when the
      // server dies under the connection.
      await send(page, 'please spin the slow wheel');
      await expect(page.getByTestId('message-assistant').nth(1)).toContainText('The slow wheel', {
        timeout: 30_000,
      });

      server.kill();
      // The pane SAYS the stream is down — the reset reached it.
      await expect(page.getByTestId('chat-connection-banner').first()).toBeVisible({
        timeout: 15_000,
      });

      // The outage: the turn keeps running against the daemon while nothing
      // can reach this browser. Its length is the disconnect being measured.
      await outage(OUTAGE_MS);
      await server.start();

      // The reconnect delivers the turn's own end: the replayed tail (what
      // was persisted while nothing could reach the page) plus the live
      // remainder.
      await expect(page.getByTestId('message-assistant').nth(1)).toContainText(SLOW_REPLY_END, {
        timeout: 90_000,
      });

      // The stream that came back named the last seq the store had APPLIED,
      // and the number is the whole assertion. `after=0` is what an unarmed
      // cursor sends — a client that never read a seq off a frame still asks
      // for a tail, the server still replays the session from the top, and
      // identity dedup still hides the doubles, so every assertion below
      // this one passes with the `id:` stamping deleted. Only the value
      // distinguishes a cursor from its absence: this session was created by
      // this spec, so the first turn's events are the only thing that can
      // have advanced it past zero.
      const streams = await sourcesFor(page, `/api/chat/events/${id}`);
      expect(streams.length, 'the disconnect opened a new source').toBeGreaterThanOrEqual(2);
      const reconnect = streams[streams.length - 1]!.url;
      const cursor = Number(new URL(reconnect, server.baseURL).searchParams.get('after'));
      expect(cursor, `reconnect asked for ${reconnect}`).toBeGreaterThan(0);

      // And nothing drew twice. The counts are the coarse half; the EXACT
      // text is the half that bites. A cursor that names a stale seq (the
      // last history refetch rather than the last applied event) makes the
      // server replay the FIRST turn as well, and a replayed turn lands as
      // repeated text inside a bubble that `toContainText` would still call
      // a match.
      await expect(page.getByTestId('message-user')).toHaveCount(2);
      await expect(page.getByTestId('message-assistant')).toHaveCount(2);
      const first = page.getByTestId('message-assistant').first();
      const second = page.getByTestId('message-assistant').nth(1);
      expect((await first.innerText()).trim()).toBe('The live chain answered.');
      await expect(second).toContainText('the stones remember');

      await api.dispose();
    } finally {
      server.kill();
    }
  });

  test('a reload mid-turn replays: one history read, one stream, one transcript', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Reload me');

    await installEventSourceSpy(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });

    // Mid-turn reload: the turn is streaming when the page goes away.
    await send(page, 'please spin the slow wheel');
    await expect(page.getByTestId('message-assistant').first()).toContainText('The slow wheel', {
      timeout: 30_000,
    });

    await page.reload();
    await appReady(page);
    // A reloaded page is a fresh module context: the restored pane cannot
    // carry a cursor (nothing has been applied yet), so it is re-mounted the
    // way the persisted layout would — one bind, one hydration, one stream.
    await openSessionsList(page);
    const groupsAfter = await centerGroupIds(page);
    const log = captureApiRequests(page);
    await mountChat(page, groupsAfter[0], id, `tab-chat-${id}-reloaded`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });

    // Replay ≡ reload, measured while the turn is STILL RUNNING: the reloaded
    // pane hydrated ONE history document and opened ONE stream — a refetch
    // storm would show as more of either before anything ended.
    await expect(page.getByTestId('message-assistant').first()).toContainText(
      'the stones remember',
      { timeout: 90_000 },
    );
    const HISTORY = new RegExp(`/api/session/${id}/history`);
    expect(log.count('GET', HISTORY), describeRequests(log, HISTORY)).toBe(1);
    expect(
      log.count('GET', `/api/chat/events/${id}`),
      describeRequests(log, `/api/chat/events/${id}`),
    ).toBe(1);

    // The turn ends; the stream's own route invalidates the history document
    // once for it (the sanctioned second read — the document the next bind
    // reads), and the stream count never moves.
    await expect(page.getByTestId('message-assistant').first()).toContainText(SLOW_REPLY_END, {
      timeout: 90_000,
    });
    await apiQuiet(log);
    expect(log.count('GET', HISTORY)).toBeLessThanOrEqual(2);
    expect(log.count('GET', `/api/chat/events/${id}`)).toBe(1);

    // And the reloaded transcript drew the turn exactly once.
    await expect(page.getByTestId('message-user')).toHaveCount(1);
    await expect(page.getByTestId('message-assistant')).toHaveCount(1);
    await expect(page.getByTestId('message-assistant').first()).toContainText('the stones remember');

    await api.dispose();
  });
});
