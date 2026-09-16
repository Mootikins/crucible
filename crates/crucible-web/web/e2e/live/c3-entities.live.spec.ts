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

const SKILLS = /^\/api\/skills$/;
const SKILLS_SEARCH = /^\/api\/skills\/search$/;

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
    await apiQuiet(page, log);

    // Nothing reads the plugin roster until a panel that draws it mounts.
    expect(log.count('GET', '/api/plugins')).toBe(0);

    await mountTab(page, 'left', { id: 'plugins-tab', title: 'Plugins', contentType: 'plugins' });
    await expect(page.getByTestId('plugins-refresh')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(page, log);

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
    await apiQuiet(page, log);

    expect(log.count('GET', '/api/plugins'), describeRequests(log, '/api/plugins')).toBe(1);
    expect(log.count('GET', '/api/plugins/options')).toBe(1);
  });

  test('the skills panel reads the roster once and searches only what was typed', async ({
    page,
  }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);

    await mountTab(page, 'left', { id: 'skills-tab', title: 'Skills', contentType: 'skills' });
    await expect(page.getByTestId('skills-search-input')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(page, log);

    // The roster, once. The SEARCH is a different key and an empty box asks
    // nothing: a panel that searched for "" on mount would fetch a second
    // answer nobody reads.
    expect(log.count('GET', SKILLS), describeRequests(log, SKILLS)).toBe(1);
    expect(log.count('GET', SKILLS_SEARCH), describeRequests(log, SKILLS_SEARCH)).toBe(0);

    // A typed query. The panel debounces, so four keystrokes are one search.
    await page.getByTestId('skills-search-input').fill('note');
    await apiQuiet(page, log);
    const afterFirst = log.count('GET', SKILLS_SEARCH);
    expect(afterFirst, describeRequests(log, SKILLS_SEARCH)).toBe(1);
    // And the roster was not asked for again behind it.
    expect(log.count('GET', SKILLS)).toBe(1);

    // Clearing and retyping the SAME query answers from the key it already
    // holds, so the daemon is not asked twice for one question.
    await page.getByTestId('skills-search-input').fill('');
    await apiQuiet(page, log);
    await page.getByTestId('skills-search-input').fill('note');
    await apiQuiet(page, log);
    expect(log.count('GET', SKILLS_SEARCH), describeRequests(log, SKILLS_SEARCH)).toBe(afterFirst);
  });

  test('the surfaces panel reads the roster once and holds one stream', async ({ page }) => {
    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await apiQuiet(page, log);

    expect(log.count('GET', '/api/surfaces')).toBe(0);

    const groupId = await mountTab(page, 'left', {
      id: 'surfaces-tab',
      title: 'Surfaces',
      contentType: 'surfaces',
    });
    await expect(page.getByTestId('edge-tab-left-surfaces-tab')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(page, log);

    expect(log.count('GET', '/api/surfaces'), describeRequests(log, '/api/surfaces')).toBe(1);
    expect(log.count('GET', '/api/surfaces/events')).toBe(1);
    expect((await sourcesFor(page, '/api/surfaces/events')).filter((s) => s.open).length).toBe(1);

    // Nothing changed on the daemon, so nothing refetches. A panel that polled
    // its roster would add a second read here.
    await page.waitForTimeout(5000);
    expect(log.count('GET', '/api/surfaces'), describeRequests(log, '/api/surfaces')).toBe(1);

    // The panel leaves and takes its stream. A second mount opens a fresh one,
    // and reads the roster again only because the first mount's entry is the
    // one it shares — so the count is what the cache decides, never more than
    // one per mount.
    await closeTab(page, groupId, 'surfaces-tab');
    await expect
      .poll(async () => (await sourcesFor(page, '/api/surfaces/events')).some((s) => s.open), {
        timeout: 15_000,
      })
      .toBe(false);

    await mountTab(page, 'left', {
      id: 'surfaces-again',
      title: 'Surfaces',
      contentType: 'surfaces',
    });
    await expect(page.getByTestId('edge-tab-left-surfaces-again')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(page, log);

    // The roster came from the cache the first mount filled: still one read.
    expect(log.count('GET', '/api/surfaces'), describeRequests(log, '/api/surfaces')).toBe(1);

    // The stream did NOT come from a cache, because the last subscriber closed
    // it, so the second mount opened a second one.
    //
    // At LEAST two, not exactly two. `subscribeToSurfaceEvents` reconnects a
    // dropped stream on a backoff, and a reconnect is a third request for the
    // same one stream. Counting reconnects would make this assertion about the
    // transport's retry rather than about the refcount, and it failed that way
    // once on a loaded box.
    expect(
      log.count('GET', '/api/surfaces/events'),
      describeRequests(log, '/api/surfaces/events'),
    ).toBeGreaterThanOrEqual(2);
    // However many were opened, ONE is live: the refcount holds.
    await expect
      .poll(async () => (await sourcesFor(page, '/api/surfaces/events')).filter((s) => s.open).length, {
        timeout: 15_000,
        message: 'the second mount did not settle on exactly one open stream',
      })
      .toBe(1);
  });

  test('reloading a plugin refreshes the roster once', async ({ page }) => {
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await mountTab(page, 'left', { id: 'plugins-tab', title: 'Plugins', contentType: 'plugins' });
    await expect(page.getByTestId('plugins-refresh')).toBeVisible({ timeout: 20_000 });
    await apiQuiet(page, log);

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
    await apiQuiet(page, log);

    // ONE refetch of the roster, not one per observer of it. The panel and the
    // settings section both read `keys.pluginList()`, and an invalidation that
    // reached two separate caches would fetch twice.
    expect(log.count('GET', '/api/plugins'), describeRequests(log, '/api/plugins')).toBe(1);
  });
});
