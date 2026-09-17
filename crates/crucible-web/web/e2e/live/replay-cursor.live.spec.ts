import { test, expect, request as playwrightRequest, type APIRequestContext, type Page } from '@playwright/test';
import { appReady, openSessionsList } from '../helpers/nav';
import { readState } from './_state';
import { apiQuiet, captureApiRequests, describeRequests, installEventSourceSpy, sourcesFor } from './_requests';
import { centerGroupIds, mountChat, resetStoredLayout } from './_panes';

/**
 * Task G5, live: the seq cursor turns a broken connection and a page reload
 * into a replay instead of a loss.
 *
 * Both specs drive a REAL turn — the fake model streams a long reply one word
 * every 120ms (`spin the slow wheel`, `global-setup.ts`), so the turn is still
 * running when the connection goes — and assert the transcript that survives
 * is the transcript an uninterrupted run would have drawn.
 */
const state = readState();

test.describe.configure({ timeout: 240_000 });

async function createSession(api: APIRequestContext, title: string): Promise<string> {
  const created = await api.post('/api/session', {
    data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
  });
  expect(created.status(), await created.text()).toBe(200);
  const id = ((await created.json()) as { session_id: string }).session_id;
  await api.put(`/api/session/${id}/title`, { data: { title } });
  return id;
}

const SLOW_REPLY_END = 'the day is done';

/** Sends a turn from the composer of the first pane. */
async function send(page: Page, text: string): Promise<void> {
  const input = page.getByTestId('chat-input').first();
  await input.fill(text);
  await page.getByTestId('send-button').first().click();
}

test.describe('live seq-cursor replay', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test.beforeEach(async () => {
    await resetStoredLayout(state.baseURL!);
  });

  test.afterAll(async () => {
    if (!state.skip) await resetStoredLayout(state.baseURL!);
  });

  test('a mid-turn disconnect reconnects from the cursor and loses nothing', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Reconnect me');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);
    log.reset();

    // A first turn that completes normally: whatever the reconnect replays
    // later, this turn's events were already applied with their seqs.
    await send(page, 'a hermetic first turn');
    await expect(page.getByTestId('message-assistant').first()).toContainText(
      'The live chain answered.',
      { timeout: 60_000 },
    );
    await apiQuiet(log);
    log.reset();

    // The turn that gets disconnected: it is still streaming when the network
    // goes, and still streaming when it comes back.
    await send(page, 'please spin the slow wheel');
    await expect(page.getByTestId('message-assistant').nth(1)).toContainText('The slow wheel', {
      timeout: 30_000,
    });

    await page.context().setOffline(true);
    // The pane SAYS the stream is down before anything else happens — that
    // banner is the condition, not a guessed sleep.
    await expect(page.getByTestId('chat-connection-banner').first()).toBeVisible({
      timeout: 15_000,
    });
    await page.context().setOffline(false);

    // The reconnect delivers the turn's own end: the replayed tail (what was
    // persisted during the gap) plus the live remainder.
    await expect(page.getByTestId('message-assistant').nth(1)).toContainText(SLOW_REPLY_END, {
      timeout: 90_000,
    });

    // The stream that came back named the last seq the store had applied.
    const streams = await sourcesFor(page, `/api/chat/events/${id}`);
    expect(streams.length, 'a reconnect opened a new source').toBeGreaterThanOrEqual(2);
    expect(streams[streams.length - 1]!.url).toMatch(/after=\d+/);

    // And nothing drew twice: one user bubble and one assistant bubble per
    // turn, which is what the uninterrupted run shows.
    await expect(page.getByTestId('message-user')).toHaveCount(2);
    await expect(page.getByTestId('message-assistant')).toHaveCount(2);
    const first = page.getByTestId('message-assistant').first();
    const second = page.getByTestId('message-assistant').nth(1);
    await expect(first).toContainText('The live chain answered.');
    await expect(second).toContainText('the stones remember');

    await api.dispose();
  });

  test('a reload mid-turn replays: one history read, one stream, one transcript', async ({ page }) => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const id = await createSession(api, 'Reload me');

    await installEventSourceSpy(page);
    const log = captureApiRequests(page);
    await page.goto(state.baseURL!);
    await appReady(page);
    await openSessionsList(page);
    const groups = await centerGroupIds(page);
    await mountChat(page, groups[0], id, `tab-chat-${id}`);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });
    await apiQuiet(log);
    log.reset();

    // Mid-turn reload: the turn is streaming when the page goes away.
    await send(page, 'please spin the slow wheel');
    await expect(page.getByTestId('message-assistant').first()).toContainText('The slow wheel', {
      timeout: 30_000,
    });

    await page.reload();
    await appReady(page);
    await expect(page.getByTestId('chat-input').first()).toBeVisible({ timeout: 20_000 });

    // Replay ≡ reload: the reloaded pane hydrates ONE history document, opens
    // ONE stream, and the turn finishes on it. A refetch storm would show as
    // more of either.
    await expect(page.getByTestId('message-assistant').first()).toContainText(SLOW_REPLY_END, {
      timeout: 90_000,
    });
    await apiQuiet(log);
    const HISTORY = new RegExp(`/api/session/${id}/history`);
    expect(log.count('GET', HISTORY), describeRequests(log, HISTORY)).toBe(1);
    expect(
      log.count('GET', `/api/chat/events/${id}`),
      describeRequests(log, `/api/chat/events/${id}`),
    ).toBe(1);

    // The reloaded stream resumed past the hydration's max seq — the cursor
    // the fold recorded — rather than replaying the whole log.
    const streams = await sourcesFor(page, `/api/chat/events/${id}`);
    expect(streams).toHaveLength(1);
    expect(streams[0]!.url).toMatch(/after=\d+/);

    // And the reloaded transcript drew the turn exactly once.
    await expect(page.getByTestId('message-user')).toHaveCount(1);
    await expect(page.getByTestId('message-assistant')).toHaveCount(1);
    await expect(page.getByTestId('message-assistant').first()).toContainText('the stones remember');

    await api.dispose();
  });
});
