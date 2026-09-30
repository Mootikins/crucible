import { test, expect } from '@playwright/test';
import { appReady } from '../helpers/nav';
import { readState } from './_state';
import {
  apiQuiet,
  captureApiRequests,
  describeRequests,
  installEventSourceSpy,
  sourcesFor,
} from './_requests';
import { closeTab, mountTab, resetStoredLayout } from './_panes';

/**
 * Part C3 against the daemon: plugins, surfaces, skills and the panels that
 * read them.
 *
 * These four entities have the same shape of claim as C2's, and one addition
 * the session entities do not have: the SAME entity is drawn in two different
 * places. The plugin roster appears in the Plugins panel and again in the
 * settings dialog, and each used to fetch it. One key, read twice, is the
 * thing this file can fail on.
 */
const state = readState();

test.describe.configure({ timeout: 180_000 });

// [[Simplification Plan#Step 19]] item 3: `skills.list`/`skills.search` are
// two rows of the one `POST /api/rpc/{method}` route now, not two REST
// routes — the method name in the path is still what tells them apart.
const SKILLS = /^\/api\/rpc\/skills\.list$/;
const SKILLS_SEARCH = /^\/api\/rpc\/skills\.search$/;

test.describe('live C3 entities', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // A persisted layout would leave another spec's panels mounted, and their
  // fetches would land in this one's counts.
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

  test('the plugin panel reads the roster once, and the settings dialog reads it never', async ({
    page,
  }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    // Nothing reads the plugin roster until a panel that draws it mounts.
    expect(log.count('GET', '/api/plugins')).toBe(0);

    await mountTab(page, 'left', { id: 'plugins-tab', title: 'Plugins', contentType: 'plugins' });
    await expect(page.getByTestId('plugins-refresh')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    expect(log.count('GET', '/api/plugins'), describeRequests(log, '/api/plugins')).toBe(1);
    expect(
      log.count('GET', '/api/plugins/options'),
      describeRequests(log, '/api/plugins/options'),
    ).toBe(1);

    // The settings dialog draws the same roster from the same key. Its
    // Plugins section is a SECOND reader, not a second fetch.
    await page.getByTestId('ribbon-cmd-settings').click();
    await expect(page.getByTestId('settings-modal')).toBeVisible({ timeout: 20_000 });
    const pluginsNav = page.getByTestId('settings-nav-plugins');
    if (await pluginsNav.count()) await pluginsNav.click();
    await apiQuiet(log);

    expect(log.count('GET', '/api/plugins'), describeRequests(log, '/api/plugins')).toBe(1);
    expect(log.count('GET', '/api/plugins/options')).toBe(1);
  });

  test('the skills panel reads the roster once and searches only what was typed', async ({
    page,
  }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    await mountTab(page, 'left', { id: 'skills-tab', title: 'Skills', contentType: 'skills' });
    await expect(page.getByTestId('skills-search-input')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    // The roster, once. The SEARCH is a different key and an empty box asks
    // nothing: a panel that searched for "" on mount would fetch a second
    // answer nobody reads.
    expect(log.count('POST', SKILLS), describeRequests(log, SKILLS)).toBe(1);
    expect(log.count('POST', SKILLS_SEARCH), describeRequests(log, SKILLS_SEARCH)).toBe(0);

    // A typed query. The panel debounces, so four keystrokes are one search.
    await page.getByTestId('skills-search-input').fill('note');
    await apiQuiet(log);
    const afterFirst = log.count('POST', SKILLS_SEARCH);
    expect(afterFirst, describeRequests(log, SKILLS_SEARCH)).toBe(1);
    // And the roster was not asked for again behind it.
    expect(log.count('POST', SKILLS)).toBe(1);

    // Clearing and retyping the SAME query answers from the key it already
    // holds, so the daemon is not asked twice for one question.
    await page.getByTestId('skills-search-input').fill('');
    await apiQuiet(log);
    await page.getByTestId('skills-search-input').fill('note');
    await apiQuiet(log);
    expect(log.count('POST', SKILLS_SEARCH), describeRequests(log, SKILLS_SEARCH)).toBe(afterFirst);
  });

  test('the surfaces panel reconciles each open and holds one stream', async ({ page }) => {
    // NOTE: since Simplification Plan step 19 the surfaces panel reads the
    // `system` topic of the ONE shared connection, the same topic a
    // default-mounted file tree also reads. This test's "opened one
    // stream"/"closed its stream" claims, written for a stream the surfaces
    // panel owned alone, need re-verifying against the live tier: if a file
    // tree is already mounted, joining `system` opens no new connection, and
    // leaving does not close it. The occupancy check below (does an open
    // source carry `system`) is what stays true either way.
    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(log);

    expect(log.count('GET', '/api/surfaces')).toBe(0);

    const groupId = await mountTab(page, 'left', {
      id: 'surfaces-tab',
      title: 'Surfaces',
      contentType: 'surfaces',
    });
    await expect(page.getByTestId('edge-tab-left-surfaces-tab')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    const initialReads = log.count('GET', '/api/surfaces');
    // Opening the stream reconciles the initial snapshot: if a read already
    // started, it is cancelled and replaced. There is no timer or per-pane read.
    expect(initialReads, describeRequests(log, '/api/surfaces')).toBeGreaterThanOrEqual(1);
    expect(initialReads, describeRequests(log, '/api/surfaces')).toBeLessThanOrEqual(2);
    expect((await sourcesFor(page, 'system')).filter((s) => s.open).length).toBeGreaterThanOrEqual(1);

    // Nothing changed on the daemon, so nothing refetches. A panel that polled
    // its roster would add a second read here.
    //
    // The window is a QUIET window, not a sleep: `apiQuiet` returns only once
    // five whole seconds have passed with no `/api/*` call at all. That is a
    // stronger claim than the sleep it replaces — a sleep would let a poll
    // fire at four seconds and still pass, as long as the count it happened
    // to move was not this one.
    await apiQuiet(log, 5000);
    expect(log.count('GET', '/api/surfaces'), describeRequests(log, '/api/surfaces')).toBe(initialReads);

    // The panel leaves. The default layout's own file tree still reads the
    // `system` topic (confirmed live: the connection stays open here), so
    // closing the surfaces panel alone does not close it — see the NOTE
    // above. What still holds is the next claim: reopening the panel must
    // refresh the cached roster once, because a change can occur while
    // nobody is reading it, whether or not the underlying connection ever
    // dropped.
    await closeTab(page, groupId, 'surfaces-tab');

    await mountTab(page, 'left', {
      id: 'surfaces-again',
      title: 'Surfaces',
      contentType: 'surfaces',
    });
    await expect(page.getByTestId('edge-tab-left-surfaces-again')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    // Reopening reconciles again: the joiner gets `onOpen` at once when the
    // topic it wants is already carried (the file tree kept `system` open
    // the whole time here), so a fresh roster read happens on every open,
    // not only on a stream that had to physically reconnect.
    expect(log.count('GET', '/api/surfaces'), describeRequests(log, '/api/surfaces')).toBe(initialReads + 1);

    // Whatever the connection did underneath, ONE source is live carrying
    // `system`: the refcount never duplicates it.
    await expect
      .poll(async () => (await sourcesFor(page, 'system')).filter((s) => s.open).length, {
        timeout: 15_000,
        message: 'more than one open source carries the system topic',
      })
      .toBe(1);
  });

  test('reloading a plugin refreshes the roster once', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await mountTab(page, 'left', { id: 'plugins-tab', title: 'Plugins', contentType: 'plugins' });
    await expect(page.getByTestId('plugins-refresh')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);

    const rows = page.locator('[data-testid^="plugin-row-"]');
    const count = await rows.count();
    test.skip(count === 0, 'requires: at least one plugin installed in the tier daemon');

    const name = (await rows.first().getAttribute('data-testid'))!.replace('plugin-row-', '');
    log.reset();
    await rows.first().hover();
    await page.getByTestId(`plugin-reload-${name}`).click();

    await expect
      .poll(() => log.count('POST', `/api/plugins/${encodeURIComponent(name)}/reload`), {
        timeout: 20_000,
        message: 'the reload control sent nothing',
      })
      .toBe(1);
    await apiQuiet(log);

    // ONE refetch of the roster, not one per observer of it. The panel and the
    // settings section both read `keys.pluginList()`, and an invalidation that
    // reached two separate caches would fetch twice.
    expect(log.count('GET', '/api/plugins'), describeRequests(log, '/api/plugins')).toBe(1);
  });
});
