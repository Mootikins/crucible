import { test, expect, request as playwrightRequest, type APIRequestContext } from '@playwright/test';
import { appReady, openSessionsList } from '../helpers/nav';
import { readState } from './_state';

/**
 * Session lifecycle, against the real daemon.
 *
 * Converted from three tests in the mocked `e2e/session-lifecycle.spec.ts`,
 * which had drifted into asserting that a REQUEST fired. A request firing is
 * not a session being archived: the mocked archive test passed whether the
 * daemon accepted the call, refused it, or was not running at all. Here the
 * daemon's own list is the assertion, and the browser boundary the original
 * tests cared about — a hover-revealed control, a native `confirm()` — is still
 * what drives it.
 *
 * The rest of that file stays in the ui tier on purpose: they are DOM-absence
 * checks (no End button, no "Continue as new session") and a reload, none of
 * which the daemon decides.
 */
const state = readState();

// A real turn plus a resume in a second page load needs more than the
// file-wide 30s budget.
test.describe.configure({ timeout: 120_000 });

async function createTitledSession(api: APIRequestContext, title: string): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  const id = ((await created.json()) as { session_id: string }).session_id;
  const titled = await api.put(`/api/session/${id}/title`, { data: { title } });
  expect(titled.status(), await titled.text()).toBe(200);
  return id;
}

async function sessionIds(api: APIRequestContext, includeArchived = false): Promise<string[]> {
  const url = includeArchived ? '/api/session/list?include_archived=true' : '/api/session/list';
  const res = await api.get(url);
  expect(res.status()).toBe(200);
  // `session_id` on the wire: `session.list` sends the daemon's own record, and
  // `id` is the name the browser's mapper gives it afterwards. Reading `id`
  // here produced an array of `undefined` that passed a `not.toContain` check
  // while proving nothing.
  return ((await res.json()) as { sessions: { session_id: string }[] }).sessions.map(
    (s) => s.session_id,
  );
}

test.describe('live session lifecycle', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('a resumed session shows the turn it actually had', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createTitledSession(api, 'Resume me');

    // A real turn, answered by the fake model server. The mocked version of
    // this test hand-wrote a transcript the daemon had never stored, so it
    // could not tell a working history read from a broken one.
    const sent = await api.post('/api/chat/send', {
      data: { session_id: id, content: 'hermetic turn before resume' },
    });
    expect(sent.status(), await sent.text()).toBe(200);
    await expect
      .poll(
        async () => (await api.get(`/api/session/${id}/history`)).status(),
        { timeout: 30_000 },
      )
      .toBe(200);
    await expect
      .poll(
        async () => JSON.stringify(await (await api.get(`/api/session/${id}/history`)).json()),
        { timeout: 60_000, message: 'the turn never reached the transcript' },
      )
      .toContain('The live chain answered.');

    // Now resume it in a browser that has never seen it.
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    await page.getByTestId(`session-item-${id}`).click();

    await expect(page.getByTestId('message-user').first()).toContainText(
      'hermetic turn before resume',
      { timeout: 30_000 },
    );
    await expect(page.getByTestId('message-assistant').first()).toContainText(
      'The live chain answered.',
      { timeout: 30_000 },
    );

    await api.dispose();
  });

  test('the hover archive control archives the session in the daemon', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createTitledSession(api, 'Archive me');

    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);

    // The control is `opacity-0` at rest, so the hover is the test: a click
    // without it would pass while the affordance was unreachable by a human.
    const row = page.getByTestId(`session-item-${id}`);
    await expect(row).toBeVisible({ timeout: 15_000 });
    await row.hover();
    const archive = row.getByTitle('Archive session');
    await expect(archive).toBeVisible({ timeout: 5_000 });
    await archive.click();

    // The daemon archived it, and the rail stopped listing it.
    await expect
      .poll(() => sessionIds(api), { timeout: 15_000, message: 'the daemon never archived it' })
      .not.toContain(id);
    expect(await sessionIds(api, true)).toContain(id);
    await expect(row).toHaveCount(0, { timeout: 15_000 });

    await api.dispose();
  });

  test('the context-menu delete, confirmed, removes the session from the daemon', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createTitledSession(api, 'Delete me');

    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    await expect(page.getByTestId(`session-item-${id}`)).toBeVisible({ timeout: 15_000 });

    // The native confirm() round-trip only exists at the browser boundary.
    page.on('dialog', (dialog) => void dialog.accept());

    await page.getByTestId(`session-item-${id}`).click({ button: 'right' });
    const deleteItem = page.getByTestId('session-group-menu-delete-session');
    await expect(deleteItem).toBeVisible({ timeout: 5_000 });
    // zag highlights on pointerdown and selects the highlighted item on click.
    await deleteItem.hover();
    await deleteItem.click();

    // Gone from the daemon, archived sessions included — not merely gone from
    // one rail's rendering.
    await expect
      .poll(() => sessionIds(api, true), { timeout: 15_000, message: 'the daemon still holds it' })
      .not.toContain(id);

    await api.dispose();
  });
});
