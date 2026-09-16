import { test, expect, request as playwrightRequest, type APIRequestContext } from '@playwright/test';
import { appReady } from '../helpers/nav';
import { readState } from './_state';
import {
  apiQuiet,
  captureApiRequests,
  describeRequests,
  eventSources,
  installEventSourceSpy,
  sourcesFor,
} from './_requests';
import { centerGroupIds, chatTab, closeTab, mountChat, mountTab, openInNewPane, resetStoredLayout } from './_panes';

/**
 * Part E5: one stream per resource, whoever is reading it.
 *
 * Four streams reach this browser — a session's chat events, surface changes,
 * filesystem changes and plugin publications — and before `lib/query/sse.ts`
 * each consumer opened its own. Two panes on one session held two sockets to
 * one daemon endpoint, and `review-store.ts` carried a hand-written refcount
 * to stop a third.
 *
 * Counting `GET /api/chat/events/{id}` is half the claim. The other half is
 * whether a stream is open NOW: a leaked source leaves exactly the same single
 * request behind as a shared one, and only the last pane leaving may close it.
 * So this spec patches `EventSource` in an init script and reads both — how
 * many were opened, and which are still open.
 */
const state = readState();

test.describe.configure({ timeout: 180_000 });

async function createSession(api: APIRequestContext, title: string): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  const id = ((await created.json()) as { session_id: string }).session_id;
  await api.put(`/api/session/${id}/title`, { data: { title } });
  return id;
}

test.describe('live SSE routing', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // A persisted layout would leave another spec's panels mounted, holding
  // streams this one is about to count.
  test.beforeEach(async () => {
    await resetStoredLayout(state.baseURL!);
  });

  // And leave the profile as it was found. The shell SAVES its layout, so the
  // panes these specs mount are still in the daemon's copy when the next FILE
  // runs — `session-path.live.spec.ts` opened a draft into a centre that
  // already held two chat tabs of ours, and asserted on a transcript that was
  // not its own. `afterAll`, not `afterEach`: the page fixture is torn down
  // after the per-test hooks, so a save can still land behind one of those.
  test.afterAll(async () => {
    if (!state.skip) await resetStoredLayout(state.baseURL!);
  });

  test('two panes on one session open one stream, and the last one out closes it', async ({
    page,
  }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'One stream');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);

    const firstGroup = (await centerGroupIds(page))[0];
    await mountChat(page, firstGroup, id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    const secondGroup = await openInNewPane(page, chatTab(id, `tab-chat-${id}-second`));
    await expect(page.getByTestId('chat-input')).toHaveCount(2, { timeout: 20_000 });
    await apiQuiet(page, log);

    // One request, for two panes.
    expect(
      log.count('GET', `/api/chat/events/${id}`),
      describeRequests(log, `/api/chat/events/${id}`),
    ).toBe(1);
    let streams = await sourcesFor(page, `/api/chat/events/${id}`);
    expect(streams.length).toBe(1);
    expect(streams[0].open).toBe(true);

    // The first pane leaves. The refcount is still 1, so the stream stays: the
    // pane that remains is mid-conversation and must not lose its events.
    await closeTab(page, firstGroup, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input')).toHaveCount(1, { timeout: 20_000 });
    await page.waitForTimeout(1500);
    streams = await sourcesFor(page, `/api/chat/events/${id}`);
    expect(streams.length, 'closing one pane opened another stream').toBe(1);
    expect(streams[0].open, 'the surviving pane lost its stream').toBe(true);

    // The last pane leaves. Now nothing is reading, so the socket goes.
    await closeTab(page, secondGroup, `tab-chat-${id}-second`);
    await expect(page.getByTestId('chat-input')).toHaveCount(0, { timeout: 20_000 });
    await expect
      .poll(async () => (await sourcesFor(page, `/api/chat/events/${id}`))[0]?.open, {
        timeout: 15_000,
        message: 'the last pane left and the stream stayed open',
      })
      .toBe(false);

    // Still one request in the whole page session: nothing reopened it.
    expect(log.count('GET', `/api/chat/events/${id}`)).toBe(1);

    await api.dispose();
  });

  test('one stream delivers a turn to both panes at once', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Both panes');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);

    const firstGroup = (await centerGroupIds(page))[0];
    await mountChat(page, firstGroup, id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    const secondGroup = await openInNewPane(page, chatTab(id, `tab-chat-${id}-second`));
    await expect(page.getByTestId('chat-input')).toHaveCount(2, { timeout: 20_000 });

    // Sent from ONE pane.
    await page.getByTestId('chat-input').first().fill('a hermetic turn for both panes');
    await page.getByTestId('send-button').first().click();

    // Both transcripts fill, from one stream, with no page reload. The pane
    // that did not send has no request of its own to make: it reads the cache
    // entry the shared route writes.
    await expect(page.getByTestId('message-assistant').nth(0)).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );
    await expect(page.getByTestId('message-assistant').nth(1)).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );

    expect(
      log.count('GET', `/api/chat/events/${id}`),
      describeRequests(log, `/api/chat/events/${id}`),
    ).toBe(1);

    // The transcript is read TWICE for the whole turn, and twice is the
    // number for one pane as much as for two. One read binds the pane; the
    // second is `lib/query/routes/session.ts`, which invalidates
    // `keys.sessionHistory(id)` when the turn ends, so the canonical
    // transcript replaces the one the stream assembled. Both panes share that
    // one entry, so they share that one refetch — a pane with a cache of its
    // own would make the count four.
    await apiQuiet(page, log, 3000);
    expect(
      log.count('GET', /^\/api\/session\/[^/]+\/history$/),
      describeRequests(log, /^\/api\/session\/[^/]+\/history$/),
    ).toBe(2);

    await api.dispose();
  });

  test('the filesystem stream is one stream for every panel that watches it', async ({ page }) => {
    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);

    // The default layout mounts one file tree, so one stream is already up.
    expect(log.count('GET', '/api/fs/events'), describeRequests(log, '/api/fs/events')).toBe(1);
    expect((await sourcesFor(page, '/api/fs/events')).filter((s) => s.open).length).toBe(1);

    // A second and a third watcher of the same stream.
    await mountTab(page, 'left', { id: 'files-left', title: 'Files', contentType: 'files' });
    await expect(page.getByTestId('edge-tab-left-files-left')).toBeVisible({ timeout: 15_000 });
    await mountTab(page, 'left', {
      id: 'backlinks-left',
      title: 'Backlinks',
      contentType: 'backlinks',
    });
    await expect(page.getByTestId('edge-tab-left-backlinks-left')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(page, log);

    expect(log.count('GET', '/api/fs/events'), describeRequests(log, '/api/fs/events')).toBe(1);
    const fs = await sourcesFor(page, '/api/fs/events');
    expect(fs.length).toBe(1);
    expect(fs[0].open).toBe(true);
  });

  test('each stream is its own source, and a panel that leaves takes its own', async ({ page }) => {
    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);

    // The surfaces panel is the only reader of the surface stream, so it is
    // the one that opens it.
    expect(log.count('GET', '/api/surfaces/events')).toBe(0);
    const groupId = await mountTab(page, 'left', {
      id: 'surfaces-tab',
      title: 'Surfaces',
      contentType: 'surfaces',
    });
    await expect(page.getByTestId('edge-tab-left-surfaces-tab')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(page, log);
    expect(
      log.count('GET', '/api/surfaces/events'),
      describeRequests(log, '/api/surfaces/events'),
    ).toBe(1);
    expect((await sourcesFor(page, '/api/surfaces/events')).filter((s) => s.open).length).toBe(1);

    // Two streams, two sources, one each. A single source carrying both would
    // give one of them the other's events.
    const open = (await eventSources(page)).filter((s) => s.open).map((s) => s.url);
    expect(open.filter((u) => u.includes('/api/fs/events')).length).toBe(1);
    expect(open.filter((u) => u.includes('/api/surfaces/events')).length).toBe(1);

    // The surfaces panel leaves. Its stream goes with it — and the filesystem
    // stream, which nobody asked about, stays.
    await closeTab(page, groupId, 'surfaces-tab');
    await expect(page.getByTestId('edge-tab-left-surfaces-tab')).toHaveCount(0, { timeout: 15_000 });
    await expect
      .poll(async () => (await sourcesFor(page, '/api/surfaces/events')).some((s) => s.open), {
        timeout: 15_000,
        message: 'the surfaces panel left and its stream stayed open',
      })
      .toBe(false);
    expect((await sourcesFor(page, '/api/fs/events')).filter((s) => s.open).length).toBe(1);
  });
});
