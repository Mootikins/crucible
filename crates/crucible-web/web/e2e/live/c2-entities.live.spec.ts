import { test, expect, request as playwrightRequest, type APIRequestContext } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { appReady, openSessionsList } from '../helpers/nav';
import { readState } from './_state';
import { apiQuiet, captureApiRequests, describeRequests, installEventSourceSpy, sourcesFor } from './_requests';
import { centerGroupIds, chatTab, mountChat, mountTab, openInNewPane, resetStoredLayout } from './_panes';

/**
 * Part C2 against the daemon: sessions, history, modes, models, status and
 * scope, counted as FETCHES.
 *
 * Every claim here is a count, and a count is the only thing that can fail
 * when a cache stops being one cache. The mocked tier cannot make these
 * claims: a `page.route` handler answers whatever asks, so two observers
 * fetching the same entity twice looks exactly like one observer fetching it
 * once. Here the browser's own outbound requests are the evidence, and the
 * daemon is what answers them.
 *
 * `fakeLogPath` appears once, at the end, and only for what it is evidence of:
 * that a turn reached the model. It never sees a `/api/*` call.
 */
const state = readState();

// Two page loads, a real turn and a rail interaction each need more than the
// file-wide 30s budget.
test.describe.configure({ timeout: 180_000 });

async function createSession(api: APIRequestContext, title: string): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  const id = ((await created.json()) as { session_id: string }).session_id;
  const titled = await api.put(`/api/session/${id}/title`, { data: { title } });
  expect(titled.status(), await titled.text()).toBe(200);
  return id;
}

/** The per-session routes a bound chat pane reads, as regular expressions. */
const HISTORY = /^\/api\/session\/[^/]+\/history$/;
const MODES = /^\/api\/session\/[^/]+\/modes$/;
const MODELS = /^\/api\/session\/[^/]+\/models$/;
const STATUS = /^\/api\/session\/[^/]+\/status$/;

test.describe('live C2 entities', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // The shell PERSISTS its layout in the daemon, so a panel one spec mounts is
  // still mounted for the next one, and its fetches land in a count that spec
  // never asked for. Every spec here starts from the layout a fresh profile
  // gets.
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

  test('one page load reads the session roster once per variant', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    await apiQuiet(page, log);

    // TWO reads, not two of one: the archived and unarchived rosters are
    // different questions under different keys (`keys.sessions(boolean)`),
    // and the rail asks both. A third would be the defect — one key read by
    // two observers that did not share it.
    const roster = log.matching('/api/session/list');
    expect(roster.length, describeRequests(log, '/api/session/list')).toBe(2);
    expect(roster.map((r) => r.query).sort()).toEqual(['', 'include_archived=true']);

    // The whole roster of root entities, once each, in one load.
    expect(log.count('GET', '/api/providers')).toBe(1);
    expect(log.count('GET', '/api/interactions/pending')).toBeLessThanOrEqual(2);
  });

  test('a second sessions panel shares the cache the first one filled', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    await apiQuiet(page, log);

    const plain = (): number => log.matching('/api/session/list').filter((r) => r.query === '').length;
    const archived = (): number =>
      log.matching('/api/session/list').filter((r) => r.query === 'include_archived=true').length;
    expect(plain()).toBe(1);
    expect(archived()).toBe(1);

    // A second mounted reader of the same two keys.
    await mountTab(page, 'left', {
      id: 'sessions-second',
      title: 'Sessions again',
      contentType: 'sessions',
    });
    await expect(page.getByTestId('edge-tab-left-sessions-second')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(page, log);

    // The unarchived roster is untouched. This is the cache claim: a second
    // panel with a cache of its own would have to fill it, and filling it
    // means fetching both variants over again.
    expect(plain(), describeRequests(log, '/api/session/list')).toBe(1);

    // The archived roster is read once more, and once only. That read is not
    // a second cache — it is `SessionsPanel.onMount`, which calls
    // `refreshSessions({ includeArchived: true })` deliberately, and
    // `SessionContext.refreshSessions` turns a call naming the variant that is
    // ALREADY on screen into an explicit `refetch()`. One per panel that
    // mounts, into the one entry both panels then read. A count above this
    // would mean the refresh had become a loop, or that each panel had
    // acquired a cache of its own after all.
    expect(archived(), describeRequests(log, '/api/session/list')).toBe(2);
  });

  test('two panes on one session share one history read and one stream', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Two panes');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    await apiQuiet(page, log);

    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 15_000 });
    await apiQuiet(page, log);

    // The first pane binds: one read of each per-session entity, one stream.
    expect(log.count('GET', HISTORY), describeRequests(log, HISTORY)).toBe(1);
    expect(log.count('GET', MODES)).toBe(1);
    expect(log.count('GET', MODELS)).toBe(1);
    expect(log.count('GET', STATUS)).toBe(1);
    expect(log.count('GET', `/api/chat/events/${id}`)).toBe(1);

    // A SECOND pane, side by side, bound to the same session. This is the
    // claim the whole part exists for: the second pane is a second reader,
    // not a second fetch, and it opens no second `EventSource`.
    await openInNewPane(page, chatTab(id, `tab-chat-${id}-second`));
    await expect(page.getByTestId('chat-input')).toHaveCount(2, { timeout: 15_000 });
    await apiQuiet(page, log);

    expect(log.count('GET', HISTORY), describeRequests(log, HISTORY)).toBe(1);
    expect(log.count('GET', MODES)).toBe(1);
    expect(log.count('GET', MODELS)).toBe(1);
    expect(log.count('GET', STATUS)).toBe(1);
    expect(
      log.count('GET', `/api/chat/events/${id}`),
      describeRequests(log, `/api/chat/events/${id}`),
    ).toBe(1);

    // And one source is OPEN, which the request count alone cannot say: a
    // stream that was opened and leaked leaves the same single request behind
    // as a stream that is shared.
    const streams = await sourcesFor(page, `/api/chat/events/${id}`);
    expect(streams.length).toBe(1);
    expect(streams[0].open).toBe(true);

    await api.dispose();
  });

  test('attaching a kiln refreshes the scope chip in every open pane', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Scope me');

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);

    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await openInNewPane(page, chatTab(id, `tab-chat-${id}-second`));
    await expect(page.getByTestId('scope-kiln')).toHaveCount(2, { timeout: 20_000 });

    // A session created with no kilns says so, in both panes.
    await expect(page.getByTestId('scope-kiln').first()).toContainText('No kiln', {
      timeout: 15_000,
    });
    await apiQuiet(page, log);
    log.reset();

    // Attach one from the chip of the FIRST pane only.
    await page.getByTestId('scope-kiln').first().click();
    const popout = page.getByTestId('scope-kiln-popout');
    await expect(popout).toBeVisible({ timeout: 10_000 });
    await popout.getByText('alpha', { exact: true }).first().click();

    // The daemon took the write.
    await expect
      .poll(
        async () =>
          ((await (await api.get(`/api/session/${id}`)).json()) as { kilns?: string[] }).kilns ?? [],
        { timeout: 20_000, message: 'the daemon never attached the kiln' },
      )
      .toContain('alpha');

    // And BOTH panes redraw from one cache write, with no page reload. The
    // second pane never asked for anything: it shares the entry the mutation
    // patched.
    await expect(page.getByTestId('scope-kiln').nth(1)).toContainText('alpha', { timeout: 20_000 });
    await expect(page.getByTestId('scope-kiln').first()).toContainText('alpha');
    expect(log.count('POST', `/api/session/${id}/kilns/connect`)).toBe(1);

    // The roster the write invalidates is refetched ONCE per variant, not once
    // per pane that displays it.
    await apiQuiet(page, log);
    const roster = log.matching('/api/session/list');
    expect(roster.length, describeRequests(log, '/api/session/list')).toBeLessThanOrEqual(2);

    await api.dispose();
  });

  test('archiving from the rail refreshes the roster and drops the row', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Archive from the rail');

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    const row = page.getByTestId(`session-item-${id}`);
    await expect(row).toBeVisible({ timeout: 20_000 });
    await apiQuiet(page, log);
    log.reset();

    // The control is `opacity-0` at rest, so the hover is part of the test.
    await row.hover();
    const archive = row.getByTitle('Archive session');
    await expect(archive).toBeVisible({ timeout: 10_000 });
    await archive.click();

    await expect(row).toHaveCount(0, { timeout: 20_000 });
    await apiQuiet(page, log);

    expect(log.count('POST', `/api/session/${id}/archive`)).toBe(1);
    // One refresh per roster variant. More than that is the same list fetched
    // once for every observer of it.
    const roster = log.matching('/api/session/list');
    expect(roster.length, describeRequests(log, '/api/session/list')).toBeLessThanOrEqual(2);
    expect(roster.length).toBeGreaterThanOrEqual(1);

    // The daemon agrees, which is what makes this an archive rather than a
    // request that fired.
    const listed = ((await (await api.get('/api/session/list')).json()) as {
      sessions: { session_id: string }[];
    }).sessions.map((s) => s.session_id);
    expect(listed).not.toContain(id);

    await api.dispose();
  });

  test('a turn sent from the composer reaches the model and the transcript', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'A real turn');

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    const input = page.getByTestId('chat-input').first();
    await expect(input).toBeVisible({ timeout: 20_000 });

    await input.fill('a hermetic turn');
    await page.getByTestId('send-button').first().click();

    await expect(page.getByTestId('message-assistant').first()).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );

    // One send, and the stream that was already open carried the reply — the
    // turn opened no second one.
    expect(log.count('POST', '/api/chat/send')).toBe(1);
    expect(
      log.count('GET', `/api/chat/events/${id}`),
      describeRequests(log, `/api/chat/events/${id}`),
    ).toBe(1);

    // The one thing `fakeLogPath` IS evidence of: the turn left the daemon and
    // landed on the fake model server.
    const fakeLog = readFileSync(state.fakeLogPath!, 'utf-8');
    expect(fakeLog).toContain('a hermetic turn');

    await api.dispose();
  });
});
