import { test, expect, type Page } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { readState } from './_state';
import { busEmit } from '../helpers/bus';

/**
 * WS-322 — two writers on one line, settled in the conflict view.
 *
 * Live tier: a real `cru web`, a real daemon, a real kiln on disk. Every other
 * tier mocks the route that decides this, and the route is the decision: the
 * daemon merges a stale write against the disk and answers the spans the two
 * writers changed differently. A mocked answer proves only that the browser
 * reads the shape it was handed.
 *
 * The whole path in one leg: the editor saves, another writer lands the same
 * line, the watcher says so, the save merges, the region that is left opens as
 * a conflict, a person chooses, and the disk holds what they chose. Nothing is
 * copied aside and neither writer's other work is lost.
 *
 * It runs twice, once per viewport (`playwright.live.config.ts`): a phone must
 * settle a conflict, because a phone is where an offline write is made.
 */

const state = readState();

/**
 * The note both writers start from.
 *
 * `a line between them` is load-bearing, not filler. The merge diffs by line
 * and groups CONSECUTIVE changed lines into one hunk, so without an unchanged
 * line between them our two edits would be one cluster, the other writer's
 * change to the first line would collide with the whole of it, and choosing
 * theirs would discard our second edit as well. One unchanged line makes them
 * two clusters: one region to settle, and one change nobody disputes.
 */
const BASE = '# Conflict\n\nthe shared line\na line between them\na line only I touch\n';

/**
 * Settings this leg pins, before any script runs.
 *
 * Autosave off: a note saves two idle seconds after a keystroke by default, so
 * a leg that drives the saves itself would race its own timer and could not say
 * which save produced the answer it reads. Vim off on both shells: the desktop
 * ships vim on and the phone ships it off, and one leg cannot type two ways.
 * `version: 2` is load-bearing — `loadSettings` migrates a stored 0 from
 * version 1 back to the default.
 */
async function pinSettings(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem(
      'crucible:settings',
      JSON.stringify({
        version: 2,
        editor: { autosaveSeconds: 0, vimMode: false, vimModeCompact: false },
      }),
    );
  });
}

/**
 * Resolves once this page's shell has painted, whichever shell it is.
 *
 * `helpers/nav.ts:appReady` waits on the desktop ribbon, which the compact
 * shell does not draw at all, so it cannot serve a leg that runs on both.
 */
async function shellReady(page: Page): Promise<void> {
  await expect(
    page.locator('[data-testid="ribbon-toggle-left"], [data-testid="mobile-shell"]').first(),
  ).toBeVisible({ timeout: 15_000 });
}

/** Open a note in the real editor, through the product's own door. */
async function openNote(page: Page, file: string, name: string): Promise<void> {
  await page.evaluate(
    (args) => busEmit(page, 'openFile', { path: args.p, name: args.n }),
    { p: file, n: name },
  );
  await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 10_000 });
}

/** Put `suffix` at the end of the one line that holds `text`. */
async function appendToLine(page: Page, text: string, suffix: string): Promise<void> {
  const line = page.locator('.cm-line').filter({ hasText: text }).first();
  await expect(line).toBeVisible();
  await line.click();
  await page.keyboard.press('End');
  await page.keyboard.type(suffix);
}

/**
 * Save through whichever save affordance this shell draws.
 *
 * The desktop's corner bar and the phone's editor bar are two controls with
 * one accessible name, and both appear only while the buffer is dirty — which
 * is the only moment this leg saves.
 */
async function saveBuffer(page: Page): Promise<void> {
  const control = page.getByRole('button', { name: 'Save' }).first();
  await expect(control).toBeVisible({ timeout: 10_000 });
  await control.click();
}

test.describe('live note conflict (WS-322)', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('a lost save reply survives reload, a competing write and a daemon restart during drain', async ({ page }, testInfo) => {
    const name = `LostReply-${testInfo.project.name}.md`;
    const notePath = path.join(state.kilnDir!, name);
    writeFileSync(notePath, BASE);
    await pinSettings(page);
    await page.goto(state.baseURL!);
    await shellReady(page);
    await openNote(page, notePath, name);

    // Read the browser's real durable queue, not component state or a test store.
    const queued = () => page.evaluate(async (file) => {
      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const request = indexedDB.open('crucible-offline');
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      try {
        return await new Promise<any[]>((resolve, reject) => {
          const request = db.transaction('outbox').objectStore('outbox').getAll();
          request.onsuccess = () => resolve(request.result.filter((row) => row.path === file));
          request.onerror = () => reject(request.error);
        });
      } finally { db.close(); }
    }, notePath);

    await page.route('**/api/kiln/file', async (route) => {
      if (route.request().method() !== 'PUT') return route.continue();
      const response = await route.fetch();
      expect(response.status()).toBe(200);
      // The daemon committed, but this browser never receives the successful reply.
      await route.abort('failed');
    });
    await appendToLine(page, 'the shared line', ' plus mine');
    await saveBuffer(page);
    const ours = BASE.replace('the shared line', 'the shared line plus mine');
    await expect.poll(() => readFileSync(notePath, 'utf-8')).toBe(ours);
    await expect.poll(async () => (await queued()).length).toBe(1);
    expect((await queued())[0].baseText).toBe(BASE);

    await page.reload();
    await shellReady(page);
    expect((await queued())[0].body).toBe(ours);
    const combined = ours.replace('a line only I touch', 'a line changed elsewhere');
    writeFileSync(notePath, combined);
    await page.unroute('**/api/kiln/file');
    let restartedDuringDrain = false;
    await page.route('**/api/kiln/file', async (route) => {
      if (route.request().method() !== 'PUT') return route.continue();
      const response = await route.fetch();
      // A drain first compares the stale hash, then retries with the base text.
      if (response.status() === 409) return route.fulfill({ response });
      expect(response.status()).toBe(200);
      // Stop only this fixture's daemon after it has committed the drain.
      // The still-running web process must reconnect to a fresh daemon.
      execFileSync(state.cruBin!, ['daemon', 'stop'], {
        env: { ...process.env, CRUCIBLE_SOCKET: state.socket!, HOME: path.join(state.tmpDir!, 'home'),
          XDG_CONFIG_HOME: path.join(state.tmpDir!, 'cfg'), XDG_DATA_HOME: path.join(state.tmpDir!, 'data'),
          XDG_RUNTIME_DIR: path.join(state.tmpDir!, 'run') },
        stdio: 'ignore', timeout: 20_000,
      });
      await route.abort('failed');
      restartedDuringDrain = true;
    });
    await page.evaluate(() => window.dispatchEvent(new Event('online')));
    await expect.poll(() => restartedDuringDrain, { timeout: 25_000 }).toBe(true);
    expect((await queued())[0].body).toBe(ours);
    expect(readFileSync(notePath, 'utf-8')).toBe(combined);
    await page.unroute('**/api/kiln/file');
    // A safe read exercises production reconnection before replaying a write.
    await expect.poll(async () => {
      const response = await page.request.get(`${state.baseURL}/api/kiln/file`, { params: { path: notePath } });
      return response.status();
    }, { timeout: 15_000 }).toBe(200);
    await page.evaluate(() => window.dispatchEvent(new Event('online')));
    await expect.poll(async () => (await queued()).length, { timeout: 15_000 }).toBe(0);
    expect(readFileSync(notePath, 'utf-8')).toBe(combined);
  });

  test('WS-322: two writers on one line, settled in the conflict view', async (
    { page },
    testInfo,
  ) => {
    // Two viewports run this leg against ONE kiln, so each takes its own note.
    const name = `Conflict-${testInfo.project.name}`;
    const notePath = path.join(state.kilnDir!, `${name}.md`);
    writeFileSync(notePath, BASE);

    await pinSettings(page);
    await page.goto(state.baseURL!);
    await shellReady(page);
    await openNote(page, notePath, `${name}.md`);
    await expect(page.locator('.cm-content')).toContainText('the shared line');

    // 1. Our first writing lands the ordinary way: the base is current.
    await appendToLine(page, 'the shared line', ' plus mine');
    await saveBuffer(page);
    const ours =
      '# Conflict\n\nthe shared line plus mine\na line between them\na line only I touch\n';
    await expect
      .poll(() => readFileSync(notePath, 'utf-8'), { timeout: 10_000 })
      .toBe(ours);

    // 2. We write again, so the buffer holds bytes that exist nowhere else.
    //    This must happen BEFORE the other writer lands: a CLEAN buffer takes
    //    the watcher's news by re-reading, and there would be nothing to merge.
    await appendToLine(page, 'a line only I touch', ' plus mine');
    await appendToLine(page, 'the shared line', ' again');
    await expect(page.locator('.cm-content')).toContainText('the shared line plus mine again');

    // 3. The other writer — an agent, a terminal, a second browser — changes
    //    the SAME line, blind, the way every other caller writes.
    const theirs =
      '# Conflict\n\nthe shared line plus theirs\na line between them\na line only I touch\n';
    const wrote = await page.evaluate(
      async ({ file, body }) => {
        const res = await fetch('/api/kiln/file', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: file, content: body }),
        });
        return res.status;
      },
      { file: notePath, body: theirs },
    );
    expect(wrote).toBe(200);

    // 4. The watcher says the note moved, and Merge is the way out that keeps
    //    both texts. It is the same save, carrying the text it was made from.
    await expect(page.getByTestId('disk-changed-banner')).toBeVisible({ timeout: 10_000 });
    await page.getByTestId('disk-changed-merge').click();

    // 5. The daemon merged what it could and left the one line both changed.
    await expect(page.getByTestId('conflict-view')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('conflict-counter')).toHaveText('0 of 1 region resolved');
    await expect(page.getByTestId('conflict-region-0')).toBeVisible();
    // The change the other writer never touched is in the merged text already:
    // only the line both wrote is in question.
    await expect(page.getByTestId('conflict-view')).toContainText('a line only I touch plus mine');
    // Nothing is written while a region is open.
    await expect(page.getByTestId('conflict-save')).toBeDisabled();
    expect(readFileSync(notePath, 'utf-8')).toBe(theirs);

    // 6. A person chooses. The line goes to the other writer; the line only we
    //    touched was never in question and stays ours.
    await page.getByTestId('keep-theirs-0').click();
    await expect(page.getByTestId('conflict-counter')).toHaveText('1 of 1 region resolved');
    await expect(page.getByTestId('conflict-save')).toBeEnabled();
    await page.getByTestId('conflict-save').click();

    // 7. The disk holds what the person settled — neither writer's text alone.
    const settled =
      '# Conflict\n\nthe shared line plus theirs\na line between them\n' +
      'a line only I touch plus mine\n';
    await expect
      .poll(() => readFileSync(notePath, 'utf-8'), { timeout: 10_000 })
      .toBe(settled);
    expect(settled).not.toBe(theirs);
    expect(settled).not.toBe(ours);

    // Nothing waits any more, and no second note was written beside it.
    await expect(page.getByTestId('conflicts-empty')).toBeVisible({ timeout: 10_000 });
  });
});
