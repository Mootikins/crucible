import { test, expect } from '@playwright/test';
import { readdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { appReady } from '../helpers/nav';
import { readState } from './_state';
import { apiQuiet, captureApiRequests, describeRequests, installEventSourceSpy } from './_requests';
import { mountTab, openFileTree, resetStoredLayout, selectRoot } from './_panes';

/**
 * Part C4 against the daemon: the note list, the search answers, the canvas,
 * the layout and the terminal socket.
 *
 * Two of these claims can only be made here. The note list refreshes because
 * the DAEMON's file watcher saw a write and pushed an event — a mocked tier
 * has no watcher and no daemon, so it can only assert that a handler it wrote
 * was called. And the terminal is a WebSocket upgrade, which no `page.route`
 * handler sees at all.
 *
 * The file this spec writes goes into the tier's own TempDir kiln and is
 * removed again, so the corpus the other live specs read is the one they left.
 */
const state = readState();

test.describe.configure({ timeout: 180_000 });

/**
 * The note this spec writes, named afresh for every run.
 *
 * A fixed name is a path the daemon has already seen: a run that writes the
 * same bytes to the same path as the run before it gives the watcher nothing
 * to report, and the spec then fails for the absence of an event that was
 * correctly not sent. A new name each time makes every write a new file.
 */
const PROBE_PREFIX = 'C4Probe-';
let probeNote = '';

function notesFor(log: ReturnType<typeof captureApiRequests>, kilnDir: string): number {
  return log
    .matching('/api/notes')
    .filter((r) => r.method === 'GET' && r.query === `kiln=${encodeURIComponent(kilnDir)}`).length;
}

test.describe('live C4 entities', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // A persisted layout would leave another spec's trees mounted, each adding
  // a fetch to a count this spec makes a claim about.
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

  test.afterEach(() => {
    // Every probe note this file ever wrote, so a failed run leaves the kiln
    // as the other live specs expect to read it.
    for (const name of readdirSync(state.kilnDir!)) {
      if (name.startsWith(PROBE_PREFIX)) {
        rmSync(path.join(state.kilnDir!, name), { force: true });
      }
    }
  });

  test('two file trees on one kiln read its notes once', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openFileTree(page);
    await selectRoot(page, 'alpha');
    await apiQuiet(log);

    expect(notesFor(log, state.kilnDir!), describeRequests(log, '/api/notes')).toBe(1);

    // A second tree, mounted on the other rail, drawing the same corpus.
    await mountTab(page, 'left', { id: 'files-left', title: 'Files', contentType: 'files' });
    await expect(page.getByTestId('edge-tab-left-files-left')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(log);

    expect(notesFor(log, state.kilnDir!), describeRequests(log, '/api/notes')).toBe(1);
  });

  // The tree redraws for a file written into the kiln it browses, without a
  // page reload and without a poll. Only the daemon can make this claim: the
  // write below is made by nobody in the browser, and what turns it into a
  // refetch is the daemon's own file watcher.
  //
  // It was `test.fixme` for one run of the suite, because it fails after
  // `c2-entities` attaches a kiln and passes on its own. The order was not the
  // cause; a SESSION existing was. A root picked before any session is current
  // is pinned under `NO_SESSION_PIN_KEY`, and `treeRootStore.prune` — which
  // forgets the pins of sessions that are gone — dropped that pin as soon as a
  // non-empty session list arrived. The tree fell back to the roots of a
  // session it did not have, drew "No project or kiln to browse", and the note
  // index lost its only reader, so the event had nothing to refresh.
  test('a file written on disk refreshes every mounted tree, once, over the stream', async ({
    page,
  }) => {
    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openFileTree(page);
    await selectRoot(page, 'alpha');
    await mountTab(page, 'left', { id: 'files-left', title: 'Files', contentType: 'files' });
    await expect(page.getByTestId('edge-tab-left-files-left')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(log);
    expect(notesFor(log, state.kilnDir!), describeRequests(log, '/api/notes')).toBe(1);

    // Count from zero across the write, so an event still in flight from an
    // earlier spec cannot be mistaken for this one's.
    log.reset();

    // Nobody in the browser did this. The daemon's watcher sees it, sends an
    // `fs` event down the one stream both trees share, and the route turns
    // that event into ONE invalidation of the note list.
    probeNote = `${PROBE_PREFIX}${Date.now()}.md`;
    writeFileSync(
      path.join(state.kilnDir!, probeNote),
      `# ${probeNote}\n\nwritten from the live spec\n`,
    );

    await expect
      .poll(() => notesFor(log, state.kilnDir!), {
        timeout: 30_000,
        message: 'the write on disk never reached the browser',
      })
      .toBe(1);

    // The new note is drawn, with no page reload.
    await expect(page.getByText(probeNote.replace(/\.md$/, ''), { exact: false }).first())
      .toBeVisible({ timeout: 30_000 });

    // ONE refetch for two mounted trees, not one each. Two caches would make
    // two, and a tree that polled would keep making more.
    await apiQuiet(log, 3000);
    expect(notesFor(log, state.kilnDir!), describeRequests(log, '/api/notes')).toBe(1);
  });

  test('a repeated search asks the daemon once', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await mountTab(page, 'left', { id: 'search-tab', title: 'Search', contentType: 'search' });
    const input = page.getByTestId('search-input');
    await expect(input).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    // An empty box asks nothing: there is no question yet.
    expect(log.count('POST', '/api/search/grep')).toBe(0);
    expect(log.count('POST', '/api/search/semantic')).toBe(0);

    await input.fill('seeded');
    await expect
      .poll(() => log.count('POST', '/api/search/grep'), {
        timeout: 20_000,
        message: 'the typed query reached no search',
      })
      .toBeGreaterThanOrEqual(1);
    await apiQuiet(log);
    const asked = log.count('POST', '/api/search/grep');

    // Clear, then ask the SAME question again. The answer is held under the
    // query that produced it, so the daemon is not asked twice for one
    // question — the debounce alone could not do that, because the box was
    // emptied in between.
    await input.fill('');
    await apiQuiet(log);
    await input.fill('seeded');
    await apiQuiet(log);
    expect(log.count('POST', '/api/search/grep'), describeRequests(log, '/api/search/grep')).toBe(
      asked,
    );
  });

  test('the layout is read once and written once per change', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    // One read, on load. The layout is client display state everywhere else,
    // so a second read would be a second owner of it.
    expect(log.count('GET', '/api/layout'), describeRequests(log, '/api/layout')).toBe(1);
    const writesBefore = log.count('POST', '/api/layout');

    // One user change — a tab added to a rail. The store updates several
    // times while that settles (the tab, the active tab, the pane), and the
    // writer must fold those into one durable save.
    await mountTab(page, 'left', { id: 'layout-probe', title: 'Files', contentType: 'files' });
    await expect(page.getByTestId('edge-tab-left-layout-probe')).toBeVisible({ timeout: 15_000 });
    await apiQuiet(log, 3000);

    const writes = log.count('POST', '/api/layout') - writesBefore;
    expect(writes, describeRequests(log, '/api/layout')).toBeGreaterThanOrEqual(1);
    expect(writes, describeRequests(log, '/api/layout')).toBeLessThanOrEqual(2);
    // And no reader re-read what it had just written.
    expect(log.count('GET', '/api/layout')).toBe(1);
  });

  test('the canvas mounts without asking for a board it has not been given', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    await page.getByTestId('layout-menu').click();
    await page.getByTestId('layout-readd').click();
    await page.getByTestId('layout-readd-canvas').click();
    await expect(page.getByTestId('canvas-surface')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    // The board is keyed by its path (`keys.canvas(path)`), so a panel with no
    // path has no key and asks nothing. The version that fetched on mount read
    // one shared board for every canvas tab.
    expect(log.count('GET', '/api/canvas'), describeRequests(log, '/api/canvas')).toBe(0);
    expect(log.count('PUT', '/api/canvas')).toBe(0);
  });

  test('the terminal opens one socket and keeps it', async ({ page }) => {
    await page.addInitScript(() => {
      const sockets: string[] = [];
      (globalThis as Record<string, unknown>).__sockets = sockets;
      const Real = globalThis.WebSocket;
      class Spy extends Real {
        constructor(url: string | URL, protocols?: string | string[]) {
          super(url, protocols as string[]);
          sockets.push(String(url));
        }
      }
      globalThis.WebSocket = Spy as unknown as typeof WebSocket;
    });
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await page.getByTestId('ribbon-toggle-right').click();
    const tab = page.getByTestId('edge-tab-right-terminal-tab-1');
    await expect(tab).toBeVisible({ timeout: 20_000 });
    await tab.click();
    await expect(page.getByTestId('terminal-panel')).toBeVisible({ timeout: 20_000 });

    const sockets = async (): Promise<string[]> =>
      page.evaluate(() => ((globalThis as Record<string, unknown>).__sockets as string[]) ?? []);

    await expect
      .poll(async () => (await sockets()).filter((u) => u.includes('/api/terminal/ws')).length, {
        timeout: 30_000,
        message: 'the terminal opened no socket',
      })
      .toBe(1);

    // Typing is carried by the socket that is already open. A panel that
    // reconnected per command would hold a new PTY, and the shell would lose
    // its working directory between two lines.
    await page.getByTestId('terminal-panel').click();
    await page.keyboard.type('echo live-tier');
    await page.keyboard.press('Enter');

    // Wait on the shell, not on the clock. The PTY echoes what was typed and
    // then answers it, so the word appearing twice in the panel is proof that
    // the command reached a shell and came back — and that is the moment the
    // socket count below means anything. A sleep would have asserted the same
    // count before the command had left the browser.
    await expect(page.getByTestId('terminal-panel')).toContainText('live-tier', {
      timeout: 30_000,
    });
    expect((await sockets()).filter((u) => u.includes('/api/terminal/ws')).length).toBe(1);

    // And the socket is an upgrade, not a fetch: it never appears in the
    // `/api/*` request log the other claims are counted from.
    expect(log.count('GET', '/api/terminal/ws')).toBe(0);
  });
});
