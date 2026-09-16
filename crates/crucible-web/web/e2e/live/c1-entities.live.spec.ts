import { test, expect, request as playwrightRequest, type APIRequestContext } from '@playwright/test';
import path from 'node:path';
import { appReady } from '../helpers/nav';
import { readState } from './_state';
import { apiQuiet, captureApiRequests, describeRequests } from './_requests';
import {
  centerGroupIds,
  mountChat,
  mountTab,
  openFileTree,
  resetStoredLayout,
  selectRoot,
} from './_panes';

/**
 * Part C1 against the daemon: the kiln roster, the app config, the project
 * roster and the runtime targets.
 *
 * These four are the app's ROOT entities — every panel that needs to name a
 * kiln, a project or a setting reads one of them, so they are the four most
 * often fetched twice. The claim is the same one throughout: a page load asks
 * for each of them once, and every later reader of the same entity reads the
 * answer rather than asking again.
 *
 * The tier's two kilns are what make the browse-root claims real. `alpha` and
 * `beta` share no substring, so a locator that matches one can never also
 * match the other.
 */
const state = readState();

test.describe.configure({ timeout: 180_000 });

/** The tier's kilns, by the name the daemon's roster gives them. */
const ALPHA = 'alpha';
const BETA = 'beta';

/** `/api/notes` for one kiln, as a predicate over the recorded query. */
function notesFor(log: ReturnType<typeof captureApiRequests>, kilnDir: string): number {
  return log
    .matching('/api/notes')
    .filter((r) => r.method === 'GET' && r.query === `kiln=${encodeURIComponent(kilnDir)}`).length;
}

test.describe('live C1 entities', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // The shell persists its layout in the daemon, so the panels one spec mounts
  // are still mounted for the next. Each spec starts from a fresh profile's
  // layout, or its "one page load reads X once" count includes a panel it
  // never opened.
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

  test('one page load reads each root entity exactly once', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);

    expect(log.count('GET', '/api/kilns'), describeRequests(log, '/api/kilns')).toBe(1);
    expect(log.count('GET', '/api/config'), describeRequests(log, '/api/config')).toBe(1);
    expect(
      log.count('GET', '/api/project/list'),
      describeRequests(log, '/api/project/list'),
    ).toBe(1);
    expect(log.count('GET', '/api/providers'), describeRequests(log, '/api/providers')).toBe(1);

    // Nothing asks the branch provider anything on a bare load. The workspace
    // is part of the targets key, so a load with no project selected has no
    // question to ask — the version that keyed only by (plugin, axis) fetched
    // one project's branches and served them to every other.
    expect(log.count('GET', /^\/api\/targets/)).toBe(0);
  });

  test('every later reader of the kiln roster reads the answer, not the daemon', async ({
    page,
  }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);
    expect(log.count('GET', '/api/kilns')).toBe(1);

    // Three more mounted readers of the same roster: the search panel names a
    // kiln to search, the skills panel names the kiln whose skills it lists,
    // and a second file tree names the kiln it browses.
    for (const tab of [
      { id: 'search-tab', title: 'Search', contentType: 'search' },
      { id: 'skills-tab', title: 'Skills', contentType: 'skills' },
      { id: 'files-left', title: 'Files', contentType: 'files' },
    ]) {
      await mountTab(page, 'left', tab);
      await expect(page.getByTestId(`edge-tab-left-${tab.id}`)).toBeVisible({ timeout: 15_000 });
    }
    await apiQuiet(page, log);

    expect(log.count('GET', '/api/kilns'), describeRequests(log, '/api/kilns')).toBe(1);
    expect(log.count('GET', '/api/config'), describeRequests(log, '/api/config')).toBe(1);
  });

  test('switching the browse root reads the new kiln, and the roster never again', async ({
    page,
  }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openFileTree(page);
    await apiQuiet(page, log);

    // The first kiln. One read of ITS notes, and no second read of the roster
    // that named it.
    await selectRoot(page, ALPHA);
    await apiQuiet(page, log);
    expect(notesFor(log, state.kilnDir!), describeRequests(log, '/api/notes')).toBe(1);
    expect(log.count('GET', '/api/kilns'), describeRequests(log, '/api/kilns')).toBe(1);

    // The second kiln. A different key, so a read; the first kiln's answer is
    // untouched.
    await selectRoot(page, BETA);
    await apiQuiet(page, log);
    expect(notesFor(log, state.secondKilnDir!), describeRequests(log, '/api/notes')).toBe(1);
    expect(notesFor(log, state.kilnDir!)).toBe(1);
    expect(log.count('GET', '/api/kilns')).toBe(1);

    // And BACK. The answer for `alpha` is still held under its own key, so
    // returning to it asks nothing. A single-slot cache would refetch here,
    // because the second kiln overwrote the first.
    await selectRoot(page, ALPHA);
    await apiQuiet(page, log);
    expect(notesFor(log, state.kilnDir!), describeRequests(log, '/api/notes')).toBe(1);
    expect(notesFor(log, state.secondKilnDir!)).toBe(1);
    expect(log.count('GET', '/api/kilns')).toBe(1);
  });

  test('attaching a kiln to a session refreshes the scope, not the kiln registry', async ({
    page,
  }) => {
    const api: APIRequestContext = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const created = await api.post('/api/session', {
      data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
    });
    expect(created.status(), await created.text()).toBe(200);
    const id = ((await created.json()) as { session_id: string }).session_id;
    await api.put(`/api/session/${id}/title`, { data: { title: 'Attach from the tree' } });

    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    await openFileTree(page);

    // A kiln this session does not hold: the tree offers to attach it, which
    // is the one gesture in the file tree that widens what the agent can read.
    await selectRoot(page, BETA);
    await apiQuiet(page, log);
    const attach = page.getByTestId('root-attach');
    await expect(attach).toBeVisible({ timeout: 15_000 });
    log.reset();
    await attach.click();

    // The daemon took the write.
    await expect
      .poll(
        async () =>
          ((await (await api.get(`/api/session/${id}`)).json()) as { kilns?: string[] }).kilns ?? [],
        { timeout: 20_000, message: 'the daemon never attached the kiln' },
      )
      .toContain(BETA);

    // The chip in the composer says so, from the cache the write patched —
    // the pane made no read of its own.
    await expect(page.getByTestId('scope-kiln').first()).toContainText(BETA, { timeout: 20_000 });
    await apiQuiet(page, log);

    expect(log.count('POST', `/api/session/${id}/kilns/connect`)).toBe(1);
    // The KILN REGISTRY did not change, so nothing re-read it. A scope write
    // that invalidated the roster would refetch a list of every kiln on the
    // box for a change to one session's reach.
    expect(log.count('GET', '/api/kilns'), describeRequests(log, '/api/kilns')).toBe(0);
    expect(log.count('GET', '/api/project/list')).toBe(0);
    expect(log.count('GET', '/api/config')).toBe(0);

    await api.dispose();
  });

  test('the roster the daemon serves is the roster the tier built', async ({ page }) => {
    // The counts above are claims about the browser. This one is the claim
    // that makes them worth making: the names being cached are the daemon's
    // own, from the two kilns globalSetup initialised and processed.
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const res = await api.get('/api/kilns');
    expect(res.status()).toBe(200);
    const names = ((await res.json()) as { kilns: { name: string; path: string }[] }).kilns;
    expect(names.map((k) => k.name)).toEqual(expect.arrayContaining([ALPHA, BETA]));
    expect(names.find((k) => k.name === ALPHA)?.path).toBe(state.kilnDir);
    expect(names.find((k) => k.name === BETA)?.path).toBe(state.secondKilnDir);
    expect(path.basename(state.kilnDir!)).toBe(ALPHA);

    await page.goto(state.baseURL!);
    await appReady(page);
    await openFileTree(page);
    await page.getByTestId('root-dropdown').first().click();
    const popout = page.getByTestId('root-dropdown-popout').first();
    await expect(popout).toBeVisible({ timeout: 15_000 });
    await expect(popout.getByText(ALPHA, { exact: true }).first()).toBeVisible();
    await expect(popout.getByText(BETA, { exact: true }).first()).toBeVisible();

    await api.dispose();
  });
});
