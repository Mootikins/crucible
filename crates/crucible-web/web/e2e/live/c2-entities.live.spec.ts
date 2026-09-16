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

/** The session record the daemon holds, read straight from it. */
async function sessionRecord(
  api: APIRequestContext,
  id: string,
): Promise<{ state?: string; archived?: boolean; agent?: { model?: string | null } }> {
  const res = await api.get(`/api/session/${id}`);
  expect(res.status(), await res.text()).toBe(200);
  return (await res.json()) as { state?: string; archived?: boolean; agent?: { model?: string | null } };
}

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
    await apiQuiet(log);

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
    await apiQuiet(log);

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
    await apiQuiet(log);

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
    await apiQuiet(log);

    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 15_000 });
    await apiQuiet(log);

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
    await apiQuiet(log);

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
    await apiQuiet(log);
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
    await apiQuiet(log);
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
    await apiQuiet(log);
    log.reset();

    // The control is `opacity-0` at rest, so the hover is part of the test.
    await row.hover();
    const archive = row.getByTitle('Archive session');
    await expect(archive).toBeVisible({ timeout: 10_000 });
    await archive.click();

    await expect(row).toHaveCount(0, { timeout: 20_000 });
    await apiQuiet(log);

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

  test('switching the model writes the choice and re-reads the session once', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Switch my model');

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    // A SECOND pane on the same session, so the refetch below has two readers
    // to serve and the count can tell one cache from two.
    await openInNewPane(page, chatTab(id, `tab-chat-${id}-second`));
    await expect(page.getByTestId('chat-input')).toHaveCount(2, { timeout: 20_000 });
    await apiQuiet(log);

    // Two panes read the session's model LIST once between them.
    expect(log.count('GET', MODELS), describeRequests(log, MODELS)).toBe(1);
    log.reset();

    // The picker, as a user reaches it. The tier's fake model server
    // advertises ONE model, so the pick re-selects the model the session is
    // already on — `ChipSelect.pick` calls `onSelect` for any row, and
    // `SessionContext.switchModel` has no equality guard, so the write is
    // real. The claim is about the write and about the list key, neither of
    // which depends on the value changing.
    await page.getByTestId('model-picker-button').first().click();
    const option = page.locator('[data-testid^="model-option-"]').first();
    await expect(option).toBeVisible({ timeout: 10_000 });
    const model = (await option.getAttribute('data-testid'))!.replace('model-option-', '');
    const label = (await option.innerText()).trim();
    await option.click();

    await expect
      .poll(() => log.count('POST', `/api/session/${id}/model`), {
        timeout: 20_000,
        message: 'the picker sent no model write',
      })
      .toBe(1);
    await apiQuiet(log);

    // The daemon took the choice. The picker names a model by the id the
    // provider list gives it (`ollama/live-model`); the daemon records the
    // bare name that provider knows it by, so the record is the tail of what
    // was clicked rather than the whole of it.
    const recorded = (await sessionRecord(api, id)).agent?.model ?? '';
    expect(recorded, 'the daemon recorded no model').not.toBe('');
    expect(
      model.endsWith(recorded),
      `the picker sent ${model} and the daemon recorded ${recorded}`,
    ).toBe(true);
    // And the chip names it, from the cache the mutation patched.
    await expect(page.getByTestId('model-picker-button').first()).toContainText(label);

    // The list is re-read ONCE, and once is the whole claim.
    //
    // `useSwitchModel` invalidates `keys.session(id)`, which is a PREFIX of
    // `keys.sessionModels(id)`, so one call reaches the row and the list
    // together — on purpose, because an agent that accepts a model may offer
    // a different set of them afterwards. Two panes are bound here and two
    // caches would make that two reads, which is the defect this counts for.
    expect(log.count('GET', MODELS), describeRequests(log, MODELS)).toBe(1);

    await api.dispose();
  });

  test('a paused session is resumed by the rail that opens it', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Pause me');

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    await expect(page.getByTestId(`session-item-${id}`)).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);
    log.reset();

    // The shell ships no pause control — not a button, not a context-menu row
    // (`SessionTree` offers Archive and Delete, and nothing calls the
    // context's `pauseSession`). So the write goes through the page's own
    // request context, which carries the same session cookie the app does.
    // Everything asserted after it is the product's.
    const paused = await page.request.post(`${state.baseURL}/api/session/${id}/pause`);
    expect(paused.status(), await paused.text()).toBe(200);
    await expect
      .poll(async () => (await sessionRecord(api, id)).state, {
        timeout: 20_000,
        message: 'the daemon never paused the session',
      })
      .toBe('paused');

    // The rail still believes the session is active: nothing in this browser
    // made that write, so no cache entry was told about it. Give it the
    // product's own refresh first — a mounting Sessions panel asks for the
    // roster again in `onMount` — because `selectSession` decides whether to
    // resume from the ROW it holds, not from a fresh read.
    await mountTab(page, 'left', {
      id: 'sessions-second',
      title: 'Sessions again',
      contentType: 'sessions',
    });
    await expect(page.getByTestId('edge-tab-left-sessions-second')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(log);

    // Opening a paused session from the rail resumes it, transparently, so
    // the composer is never a dead end (`SessionContext.selectSession`). That
    // is the state change reaching the UI with no page reload.
    await page.getByTestId(`session-item-${id}`).first().click();
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 30_000 });
    await expect
      .poll(() => log.count('POST', `/api/session/${id}/resume`), {
        timeout: 30_000,
        message: 'the rail opened a paused session without resuming it',
      })
      .toBe(1);
    await apiQuiet(log);

    expect((await sessionRecord(api, id)).state).toBe('active');
    // ONE status read for the pane that bound, not one per chip that draws it.
    expect(log.count('GET', STATUS), describeRequests(log, STATUS)).toBe(1);

    await api.dispose();
  });

  test('an unarchived session comes back to the rail on one roster read', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Bring me back');

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    const row = page.getByTestId(`session-item-${id}`);
    await expect(row).toBeVisible({ timeout: 20_000 });

    // Archive through the control a user has. The hover is part of it: the
    // button is `opacity-0` at rest.
    await row.hover();
    await row.getByTitle('Archive session').click();
    await expect(row).toHaveCount(0, { timeout: 20_000 });
    await apiQuiet(log);
    log.reset();

    // The shell ships no unarchive control either — the context's
    // `unarchiveSession` has no caller in any component — so this write also
    // goes through the page's request context.
    const back = await page.request.post(`${state.baseURL}/api/session/${id}/unarchive`);
    expect(back.status(), await back.text()).toBe(200);
    expect((await sessionRecord(api, id)).archived ?? false).toBe(false);

    // The rail does not know yet, and must not pretend to: nothing in this
    // browser made that write, so no cache entry was told about it.
    await expect(row).toHaveCount(0);

    // The product's own refresh is a mounting Sessions panel, which asks for
    // the archived roster again in `onMount`. One read brings the row back.
    await mountTab(page, 'left', {
      id: 'sessions-second',
      title: 'Sessions again',
      contentType: 'sessions',
    });
    await expect(page.getByTestId('edge-tab-left-sessions-second')).toBeVisible({ timeout: 15_000 });
    await expect(row.first()).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    const archivedReads = log
      .matching('/api/session/list')
      .filter((r) => r.query === 'include_archived=true').length;
    expect(archivedReads, describeRequests(log, '/api/session/list')).toBe(1);
    // The unarchived roster was never asked for: the row returned from the
    // one list the rail is showing, not from a second fetch beside it.
    expect(log.matching('/api/session/list').filter((r) => r.query === '').length).toBe(0);

    await api.dispose();
  });
});
