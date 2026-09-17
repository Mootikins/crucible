import { test, expect, request as playwrightRequest, type Page, type APIRequestContext } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { appReady } from '../helpers/nav';
import { readState } from './_state';
import { busEmit } from '../helpers/bus';

/**
 * The session path, end to end, against the real chain.
 *
 * Four defects shipped on this path in one day, and every one of them was
 * invisible to the mocked tier by construction: a mocked route answers 200
 * because the fixture says 200. Each test here is one of those defects, stated
 * as the journey a user takes:
 *
 *  - a session created from the draft composer, its first turn answered, and
 *    its OWN folder listed by the files panel (the daemon refused that folder
 *    as a file root, so the first listing after creation answered 422);
 *  - every kiln the chip offers can be attached (the list offered a name the
 *    attach then refused);
 *  - a session read after a daemon restart, whose modes and model populate
 *    (session.get, list_modes and list_models answered only for a LIVE
 *    session, so a stored one answered 422 until a history read revived it);
 *  - the kiln switched on a live session.
 *
 * Nothing is mocked. `page.route` appears nowhere in this file, on purpose.
 */
const state = readState();

// A whole journey — app boot, a session created, a turn answered and a
// panel listing — does not fit the file-wide 30s budget, which was set for
// endpoint-shaped specs.
test.describe.configure({ timeout: 120_000 });

/** One 4xx/5xx on an `/api/` call, as the page saw it. */
interface ApiFailure {
  status: number;
  method: string;
  url: string;
}

/**
 * Record every failing API response the page receives.
 *
 * Asserting on the network is what makes these regressions visible: the 422 on
 * a session's own folder reached the user as a notification and an empty tree,
 * both of which a patient spec could rationalise away. The status code cannot
 * be rationalised. Only `/api/` is watched — a missing favicon is not the
 * product failing.
 */
function watchApiFailures(page: Page): ApiFailure[] {
  const failures: ApiFailure[] = [];
  page.on('response', (res) => {
    const url = res.url();
    if (!url.includes('/api/')) return;
    if (res.status() < 400) return;
    // A layout nobody has saved is absent, and 404 is what absent means. It is
    // the one status on these journeys that reports a fact rather than a
    // refusal, so it is named here rather than widening the filter.
    if (res.status() === 404 && url.includes('/api/layout')) return;
    failures.push({ status: res.status(), method: res.request().method(), url });
  });
  return failures;
}

function describeFailures(failures: ApiFailure[]): string {
  return failures.map((f) => `${f.status} ${f.method} ${f.url}`).join('\n');
}

/** Every `/api/` response the page received, failing or not. */
function watchApiCalls(page: Page): ApiFailure[] {
  const calls: ApiFailure[] = [];
  page.on('response', (res) => {
    const url = res.url();
    if (!url.includes('/api/')) return;
    calls.push({ status: res.status(), method: res.request().method(), url });
  });
  return calls;
}

/**
 * Open the draft composer the way the product does when no project is named.
 *
 * The sessions rail offers New Session on the PROJECT row, which pre-selects
 * that project. This journey is the other one: a session with no project, which
 * runs in a folder of its own under the daemon's workspaces directory. Its two
 * doors are the rail's empty state and the command palette, and both dispatch
 * this event. The rail's empty state is preferred when it is there; it is only
 * there while the daemon holds no session, and this file shares its daemon with
 * every other live spec.
 */
async function openDraft(page: Page): Promise<void> {
  // Scoped to the sessions rail's own empty state: `empty-state-action` is the
  // shared EmptyState button testid, and the files panel draws one too.
  const emptyAction = page.getByTestId('sessions-empty').getByTestId('empty-state-action');
  if (await emptyAction.count()) {
    await emptyAction.first().click();
  } else {
    await busEmit(page, 'newSession');
  }
  await expect(page.getByTestId('composer-input')).toBeVisible({ timeout: 15_000 });
}

/** Create a session through the daemon, and return its id. */
async function createSession(api: APIRequestContext, kilns: string[]): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns, agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  return ((await created.json()) as { session_id: string }).session_id;
}

/** The kiln names the daemon publishes: every registered kiln, open since boot. */
async function openKilnNames(api: APIRequestContext): Promise<string[]> {
  const read = async (): Promise<string[]> => {
    const listed = await api.get('/api/kilns');
    expect(listed.status()).toBe(200);
    return ((await listed.json()) as { kilns: { name: string | null }[] }).kilns
      .map((k) => k.name)
      .filter((n): n is string => !!n);
  };
  return read();
}

/** Open a session from the sessions rail and wait for its composer. */
async function openSessionFromRail(page: Page, sessionId: string): Promise<void> {
  const row = page.getByTestId(`session-item-${sessionId}`);
  await expect(row).toBeVisible({ timeout: 15_000 });
  await row.click();
  await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 15_000 });
}

test.describe('the live session path', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('a session created from the draft answers, and lists its own folder', async ({ page }) => {
    const failures = watchApiFailures(page);
    const calls = watchApiCalls(page);

    await page.goto(state.baseURL!);
    await appReady(page);
    await openDraft(page);

    // The project axis is left untouched: "Session folder" is the default, and
    // it is the shape this test is about — a session with no project. The chip
    // row folds what does not fit on one line behind a "+N" button, so open
    // the fold first; Escape closes it again, since its popover sits over the
    // send button.
    const fold = page.getByTestId('composer-chip-overflow');
    if (await fold.count()) await fold.click();
    await expect(page.getByTestId('composer-project')).toContainText('Session folder');
    await page.keyboard.press('Escape');

    await page.getByTestId('composer-input').fill('hermetic hello from the draft');
    await page.getByTestId('composer-send').click();

    // The draft closes and the real session's chat tab takes its place.
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId('message-user')).toContainText('hermetic hello from the draft');

    // The reply comes off the real chain: browser → axum → daemon → the fake
    // model server → back through the event stream.
    await expect(page.getByTestId('message-assistant').first()).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );

    // The files panel opens on the right by default and lists the session's
    // own folder. A 200 from `/api/fs/list` IS the assertion: the folder is
    // new, so an empty tree is the correct rendering of a successful listing.
    await expect
      .poll(() => calls.filter((c) => c.url.includes('/api/fs/list')).length, {
        timeout: 30_000,
        message: 'the files panel never listed anything',
      })
      .toBeGreaterThan(0);
    const listings = calls.filter((c) => c.url.includes('/api/fs/list'));
    expect(listings.filter((c) => c.status >= 400)).toEqual([]);

    // No refusal reached the user, and none reached the network either. A
    // success toast ("✓ Session created") is expected here and is not a
    // refusal, so the filter reads the toast's TYPE: its first character is
    // the type icon — '✕' error, '⚠' warning, '✓' success, 'ℹ' info. There is
    // no testid for the type, and a spec does not get to add one to the
    // application.
    const alerts = await page.getByRole('alert').allTextContents();
    expect(
      alerts.filter((text) => text.startsWith('✕') || text.startsWith('⚠')),
      `toasts: ${alerts.join(' | ')}`,
    ).toEqual([]);
    expect(describeFailures(failures)).toBe('');
  });

  test('every kiln the chip offers can be attached, and detached', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const failures = watchApiFailures(page);

    // The names the daemon actually publishes — the same list the chip builds
    // itself from. Attaching by a name this list carries must succeed; the
    // defect was a list and an attach that disagreed about what a name is.
    const names = await openKilnNames(api);
    expect(names.length, 'the daemon published no kiln names').toBeGreaterThanOrEqual(2);

    const sessionId = await createSession(api, []);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionFromRail(page, sessionId);

    for (const name of names) {
      await page.getByTestId('scope-kiln').click();
      const popout = page.getByTestId('scope-kiln-popout');
      await expect(popout).toBeVisible();
      const option = popout.getByRole('option', { name, exact: false }).first();
      await expect(option, `the chip never offered ${name}`).toBeVisible();
      await option.click();
      await page.keyboard.press('Escape');

      // The daemon is the authority on what is attached, so ask it.
      await expect
        .poll(
          async () => {
            const read = await api.get(`/api/session/${sessionId}`);
            if (read.status() !== 200) return `status ${read.status()}`;
            return ((await read.json()) as { kilns: string[] }).kilns;
          },
          { timeout: 15_000, message: `attaching ${name} never took` },
        )
        .toContain(name);
    }

    // Detaching one is the same control, used again.
    const first = names[0];
    await page.getByTestId('scope-kiln').click();
    await page.getByTestId('scope-kiln-popout').getByRole('option', { name: first, exact: false }).first().click();
    await page.keyboard.press('Escape');
    await expect
      .poll(
        async () => ((await (await api.get(`/api/session/${sessionId}`)).json()) as { kilns: string[] }).kilns,
        { timeout: 15_000, message: `detaching ${first} never took` },
      )
      .not.toContain(first);

    expect(describeFailures(failures)).toBe('');
    await api.dispose();
  });

  test('a session read after a daemon restart carries its modes and its model', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const sessionId = await createSession(api, []);

    // Stop only this fixture's daemon. The still-running web process starts a
    // fresh one on its next call, which is exactly the state the defect needed:
    // the session lives in storage, and no history read has revived it.
    execFileSync(state.cruBin!, ['daemon', 'stop'], {
      env: state.childEnv,
      stdio: 'ignore',
      timeout: 20_000,
    });
    await expect
      .poll(async () => (await api.get('/api/session/list')).status(), {
        timeout: 30_000,
        message: 'the web process never reconnected to a fresh daemon',
      })
      .toBe(200);

    // Only now start watching: the reconnect itself may race one call, and the
    // claim under test is about what the RESTORED pane sees.
    const failures = watchApiFailures(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionFromRail(page, sessionId);

    // The mode control renders only when the modes list is non-empty, and the
    // model chip only names a model when the models list answered. Both read
    // the session straight from storage now; both used to need a history read
    // first, and answered 422 without one.
    await expect(page.getByTestId('chat-mode')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId('model-picker-button')).toContainText(state.modelName!, {
      timeout: 15_000,
    });

    expect(describeFailures(failures)).toBe('');
    await api.dispose();
  });

  test('the kiln switches on a live session', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const failures = watchApiFailures(page);

    const names = await openKilnNames(api);
    expect(names.length).toBeGreaterThanOrEqual(2);
    const [from, to] = names;

    const sessionId = await createSession(api, [from]);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionFromRail(page, sessionId);
    await expect(page.getByTestId('scope-kiln')).toContainText(from);

    // Attach the other, drop the first: the switch is two toggles of one
    // control, because a session's kilns are a flat set with no member
    // privileged.
    await page.getByTestId('scope-kiln').click();
    await page.getByTestId('scope-kiln-popout').getByRole('option', { name: to, exact: false }).first().click();
    await page.getByTestId('scope-kiln-popout').getByRole('option', { name: from, exact: false }).first().click();
    await page.keyboard.press('Escape');

    await expect
      .poll(
        async () => ((await (await api.get(`/api/session/${sessionId}`)).json()) as { kilns: string[] }).kilns,
        { timeout: 15_000, message: 'the switch never reached the daemon' },
      )
      .toEqual([to]);
    // And the chip says so without a reload.
    await expect(page.getByTestId('scope-kiln')).toContainText(to);

    expect(describeFailures(failures)).toBe('');
    await api.dispose();
  });
});
