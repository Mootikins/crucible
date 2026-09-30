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
 * Part E5, after Simplification Plan step 19: one connection for the whole
 * page, whoever is reading it.
 *
 * Four topics reach this browser over it — a session's chat events, surface
 * changes, filesystem changes and plugin publications (the last three all on
 * the `system` topic) — and before `lib/query/sse.ts` each opened its own
 * `EventSource`. Then step 11 shared one source per topic; step 19 went
 * further and put every topic on ONE physical connection,
 * `GET /api/events?topics=...`, rebuilt whenever the set of topics a page
 * needs changes (a pane opens a new session, a panel that reads the `system`
 * topic mounts or unmounts) and left alone when a reader merely joins or
 * leaves a topic the connection already carries.
 *
 * Counting `GET /api/events` is half the claim. The other half is whether a
 * connection is open NOW: a leaked source leaves exactly the same single
 * request behind as a shared one, and only the last topic's last reader
 * leaving may close it. So this spec patches `EventSource` in an init script
 * and reads both — how many connections were opened, and which are still
 * open — and reads each one's `topics` query to know which resource it
 * still carries.
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

/** The topics a source's `?topics=` query names, or `[]` for a non-matching url. */
function topicsOf(url: string): string[] {
  const match = /[?&]topics=([^&]*)/.exec(url);
  return match ? decodeURIComponent(match[1]).split(',') : [];
}

/** The open sources whose `topics` query names `topic`. */
async function sourcesCarrying(page: Parameters<typeof eventSources>[0], topic: string) {
  return (await eventSources(page)).filter((s) => topicsOf(s.url).includes(topic));
}

test.describe('live SSE routing', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // A persisted layout would leave another spec's panels mounted, holding
  // topics this one is about to count.
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

  test('two panes on one session share the connection, and the last one out drops its topic', async ({
    page,
  }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'One stream');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    const firstGroup = (await centerGroupIds(page))[0];
    await mountChat(page, firstGroup, id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    const secondGroup = await openInNewPane(page, chatTab(id, `tab-chat-${id}-second`));
    await expect(page.getByTestId('chat-input')).toHaveCount(2, { timeout: 20_000 });
    await apiQuiet(log);

    // One open source names this session's topic, whichever other topics
    // (the default layout's own `system` topic reader) ride along with it.
    let carrying = (await sourcesCarrying(page, id)).filter((s) => s.open);
    expect(carrying.length, describeRequests(log, /\/api\/events\?/)).toBe(1);

    // The first pane leaves. The refcount on this topic is still 1 (the
    // second pane), so it stays on the connection: the pane that remains is
    // mid-conversation and must not lose its events.
    await closeTab(page, firstGroup, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input')).toHaveCount(1, { timeout: 20_000 });
    // A quiet window, so a close that was going to rebuild the connection has
    // had its chance to. `apiQuiet` returns on the condition — no new call
    // for a second and a half — rather than after a fixed wait that would
    // pass whether or not the page had finished reacting.
    await apiQuiet(log, 1500);
    carrying = (await sourcesCarrying(page, id)).filter((s) => s.open);
    expect(carrying.length, 'closing one pane dropped the session topic').toBe(1);

    // The last pane leaves. Now nothing reads this session's topic; the
    // connection rebuilds without it (or closes outright, if nothing else
    // reads any topic).
    await closeTab(page, secondGroup, `tab-chat-${id}-second`);
    await expect(page.getByTestId('chat-input')).toHaveCount(0, { timeout: 20_000 });
    await expect
      .poll(async () => (await sourcesCarrying(page, id)).some((s) => s.open), {
        timeout: 15_000,
        message: 'the last pane left and the session topic stayed on an open source',
      })
      .toBe(false);

    await api.dispose();
  });

  test('one connection delivers a turn to both panes at once', async ({ page }) => {
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

    // Both transcripts fill, from one connection, with no page reload. The
    // pane that did not send has no request of its own to make: it reads the
    // cache entry the shared route writes.
    await expect(page.getByTestId('message-assistant').nth(0)).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );
    await expect(page.getByTestId('message-assistant').nth(1)).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );

    expect((await sourcesCarrying(page, id)).filter((s) => s.open).length).toBe(1);

    // The transcript is read ONCE for the whole turn, and once is the number
    // for one pane as much as for two. The read binds the pane; the ops of
    // the stream keep the daemon's transcript current after it, so the end of
    // the turn reads nothing. Both panes share the one session store — a pane
    // with a store of its own would make the count two.
    await apiQuiet(log, 3000);
    expect(
      log.count('GET', /^\/api\/session\/[^/]+\/history$/),
      describeRequests(log, /^\/api\/session\/[^/]+\/history$/),
    ).toBe(1);

    await api.dispose();
  });

  test('the filesystem, surfaces and chat topics all ride the one connection', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'One connection, three topics');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    // The default layout mounts one file tree, so the `system` topic (the
    // filesystem watcher, surfaces and publications all ride it) is already
    // on an open connection.
    let open = (await eventSources(page)).filter((s) => s.open);
    expect(open.length, describeRequests(log, /\/api\/events\?/)).toBe(1);
    expect(topicsOf(open[0].url)).toContain('system');

    // A chat pane joins a second topic: the connection rebuilds to carry
    // both, rather than opening a second `EventSource`.
    const firstGroup = (await centerGroupIds(page))[0];
    await mountChat(page, firstGroup, id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    open = (await eventSources(page)).filter((s) => s.open);
    expect(open.length, 'a second topic opened a second connection instead of rebuilding the one').toBe(1);
    expect(topicsOf(open[0].url).sort()).toEqual(['system', id].sort());

    // A surfaces panel joins the SAME `system` topic; still one connection.
    const groupId = await mountTab(page, 'left', {
      id: 'surfaces-tab',
      title: 'Surfaces',
      contentType: 'surfaces',
    });
    await expect(page.getByTestId('edge-tab-left-surfaces-tab')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(log);
    open = (await eventSources(page)).filter((s) => s.open);
    expect(open.length, 'a second reader of an already-carried topic opened a new connection').toBe(1);

    // The surfaces panel leaves; the `system` topic still has the file tree
    // reading it, so the connection is untouched.
    await closeTab(page, groupId, 'surfaces-tab');
    await expect(page.getByTestId('edge-tab-left-surfaces-tab')).toHaveCount(0, { timeout: 15_000 });
    await apiQuiet(log, 1500);
    open = (await eventSources(page)).filter((s) => s.open);
    expect(open.length).toBe(1);
    expect(topicsOf(open[0].url)).toContain('system');

    // The chat pane leaves too. Only the file tree's `system` topic remains.
    await closeTab(page, firstGroup, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input')).toHaveCount(0, { timeout: 20_000 });
    await expect
      .poll(async () => (await sourcesCarrying(page, id)).some((s) => s.open), {
        timeout: 15_000,
        message: 'the chat pane left and the session topic stayed on an open source',
      })
      .toBe(false);
    open = (await eventSources(page)).filter((s) => s.open);
    expect(open.length, 'the file tree lost its connection when an unrelated topic left').toBe(1);
    expect(topicsOf(open[0].url)).toEqual(['system']);

    await api.dispose();
  });
});
