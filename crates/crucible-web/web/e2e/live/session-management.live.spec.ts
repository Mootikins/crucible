import {
  test,
  expect,
  request as playwrightRequest,
  type APIRequestContext,
} from '@playwright/test';
import { appReady, openSessionsList } from '../helpers/nav';
import { busEmit } from '../helpers/bus';
import { readState } from './_state';
import { resetStoredLayout } from './_panes';

/**
 * Session management, against the real daemon.
 *
 * Converted from the mocked `e2e/session-management.spec.ts`, which asserted
 * the same three things about sessions a fixture invented. Every assertion here
 * held under the mocks and holds under the daemon — that is what made it a
 * straight conversion — but only this version can fail when the daemon and the
 * browser disagree about what a session is.
 */
const state = readState();

async function createTitledSession(api: APIRequestContext, title: string): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  const id = ((await created.json()) as { session_id: string }).session_id;
  const titled = await api.post('/api/rpc/session.set_title', { data: { session_id: id, title } });
  expect(titled.status(), await titled.text()).toBe(200);
  return id;
}

test.describe('live session management', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  // The shell saves its layout. Without this reset, the chat tabs of an
  // earlier spec come back when the page loads, and a locator that expects one
  // tab finds two. The saved layout lands only when its save wins a race with
  // the page close, so the failure comes and goes.
  test.beforeEach(async () => {
    await resetStoredLayout(state.baseURL!);
  });

  // And leave the profile as it was found, for the spec that runs next.
  test.afterAll(async () => {
    if (!state.skip) await resetStoredLayout(state.baseURL!);
  });

  test('the rail lists the sessions the daemon holds, by their titles', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const first = await createTitledSession(api, 'Live session one');
    const second = await createTitledSession(api, 'Live session two');

    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);

    // The ids come from the daemon, so the rows can only be there if the
    // browser read the daemon's own list.
    await expect(page.getByTestId(`session-item-${first}`)).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId(`session-item-${second}`)).toBeVisible();
    // Each row by its id, which is new on each try. A retry runs against the
    // same daemon, so the titles of an earlier try are still in the list.
    await expect(page.getByTestId(`session-item-${first}`)).toContainText('Live session one');
    await expect(page.getByTestId(`session-item-${second}`)).toContainText('Live session two');

    await api.dispose();
  });

  test('a draft creates nothing until its first message is sent', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const before = ((await (await api.post('/api/rpc/session.list', { data: {} })).json()) as { sessions: unknown[] })
      .sessions.length;

    await page.goto(state.baseURL!);
    await appReady(page);

    let createdEarly = false;
    page.on('request', (req) => {
      if (req.url().endsWith('/api/session') && req.method() === 'POST') createdEarly = true;
    });

    await busEmit(page, 'newSession');
    await expect(page.getByTestId('composer-input')).toBeVisible({ timeout: 15_000 });
    expect(createdEarly).toBe(false);
    // And the daemon agrees: lazy creation is a claim about the daemon's
    // records, not only about the browser's outbound calls.
    expect(
      ((await (await api.post('/api/rpc/session.list', { data: {} })).json()) as { sessions: unknown[] }).sessions
        .length,
    ).toBe(before);

    await page.getByTestId('composer-input').fill('First message');
    await page.getByTestId('composer-send').click();

    await expect
      .poll(
        async () =>
          ((await (await api.post('/api/rpc/session.list', { data: {} })).json()) as { sessions: unknown[] }).sessions
            .length,
        { timeout: 30_000, message: 'the first message created no session' },
      )
      .toBe(before + 1);

    await api.dispose();
  });

  test('clicking a session opens the one that was clicked', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const target = await createTitledSession(api, 'The clicked one');

    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);

    // `session.get` is one RPC method now (Simplification Plan step 19 item
    // 9), told apart per session by the request body, not the URL.
    const read = page.waitForResponse((res) => {
      if (!res.url().includes('/api/rpc/session.get') || res.request().method() !== 'POST') {
        return false;
      }
      const body = res.request().postDataJSON() as { session_id?: string };
      return body.session_id === target;
    });
    await page.getByTestId(`session-item-${target}`).click();
    // The daemon answered the read, rather than the browser drawing a row it
    // already had. A 200 is part of the claim: the stored-session read used to
    // answer 422 here.
    expect((await read).status()).toBe(200);
    await expect(page.locator(`[data-testid="rail-tab-tab-chat-${target}"]`)).toBeVisible({
      timeout: 15_000,
    });

    await api.dispose();
  });
});
