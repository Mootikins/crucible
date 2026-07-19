import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream } from '../helpers/mock-sse';
import { createStory } from './_helpers/story';

/**
 * Story: WS-104 — approve/deny a permission request from the browser.
 *
 * The daemon emits an `interaction_requested` SSE frame mid-turn; the frontend
 * (chatEventReducer → ChatContext.pendingInteraction) renders the real
 * PermissionInteraction inline. The user's choice POSTs /api/interaction/respond
 * and clears the modal. For write permissions a diff (old vs. new) renders,
 * with old content fetched from GET /api/kiln/file.
 */

const KILN = '/home/user/.crucible/kiln';
const FILE = `${KILN}/Draft.md`;

/** A write-permission interaction frame (drives the diff preview). */
function permFrame(id: string, newContent: string) {
  return {
    type: 'interaction_requested',
    data: {
      type: 'interaction_requested',
      kind: 'permission',
      id,
      action_type: 'write',
      tokens: [FILE],
      tool_args: { content: newContent },
    },
  };
}

async function openSessionWith(page: Page, sseEvents: Array<{ type: string; data: object }>) {
  await setupBasicMocks(page, { sseEvents });
  // Old content for the diff.
  await page.route('**/api/kiln/file**', (route) =>
    route.fulfill({ json: { content: '# Draft\n\nold body\n' } }),
  );
  let respondBody: unknown = null;
  await page.route('**/api/interaction/respond', (route) => {
    respondBody = route.request().postDataJSON();
    return route.fulfill({ status: 200, body: '' });
  });
  await page.goto('/');
  await page.getByTestId('session-item-test-session-001').click();
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });
  return { getRespond: () => respondBody };
}

test.describe('WS-104 permission from the browser', () => {
  test('allow-once posts the correct respond payload and clears the modal', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const ctx = await openSessionWith(page, [permFrame('perm-1', '# Draft\n\nnew body\n')]);

    // Modal + diff render.
    await expect(page.getByText('Permission Required')).toBeVisible({ timeout: 5000 });
    await expect(page.getByText(FILE).first()).toBeVisible();
    await expect(page.getByText('new body')).toBeVisible();
    await story.step(page, 'permission modal with diff');

    const respondPromise = page.waitForRequest('**/api/interaction/respond');
    await page.getByRole('button', { name: 'Allow' }).click();
    await respondPromise;

    const body = ctx.getRespond() as {
      session_id: string;
      request_id: string;
      response: { allowed: boolean; scope: string; pattern?: string };
    };
    expect(body.request_id).toBe('perm-1');
    expect(body.response.allowed).toBe(true);
    expect(body.response.scope).toBe('once');

    // Modal clears; turn continues.
    await expect(page.getByText('Permission Required')).toHaveCount(0);
    await story.step(page, 'allowed - modal cleared');
  });

  test('choosing a scope (session) is sent in the payload', async ({ page }) => {
    const ctx = await openSessionWith(page, [permFrame('perm-scope', '# Draft\n\nX\n')]);
    await expect(page.getByText('Permission Required')).toBeVisible({ timeout: 5000 });

    // Open scope options and pick "Session", then Allow.
    await page.getByRole('button', { name: /More options/ }).click();
    await page.getByRole('button', { name: 'Session', exact: true }).click();
    const respondPromise = page.waitForRequest('**/api/interaction/respond');
    await page.getByRole('button', { name: 'Allow' }).click();
    await respondPromise;

    const body = ctx.getRespond() as { response: { scope: string } };
    expect(body.response.scope).toBe('session');
  });

  test('deny posts allowed:false and clears the modal', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    const ctx = await openSessionWith(page, [permFrame('perm-2', '# Draft\n\nnope\n')]);

    await expect(page.getByText('Permission Required')).toBeVisible({ timeout: 5000 });
    const respondPromise = page.waitForRequest('**/api/interaction/respond');
    await page.getByRole('button', { name: 'Deny' }).click();
    await respondPromise;

    const body = ctx.getRespond() as { request_id: string; response: { allowed: boolean } };
    expect(body.request_id).toBe('perm-2');
    expect(body.response.allowed).toBe(false);
    await expect(page.getByText('Permission Required')).toHaveCount(0);
    await story.step(page, 'denied - modal cleared');
  });

  test('queued permissions open sequentially', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    await page.route('**/api/kiln/file**', (route) =>
      route.fulfill({ json: { content: '# Draft\n\nold\n' } }),
    );

    // Second frame is gated behind the first respond POST, then delivered on the
    // EventSource's reconnect. No streaming message exists, so the reconnect
    // 'error' event only sets a (harmless) error signal.
    let releaseSecond: (() => void) | null = null;
    const secondReady = new Promise<void>((r) => (releaseSecond = r));
    const responds: Array<{ request_id: string }> = [];
    await page.route('**/api/interaction/respond', (route) => {
      responds.push(route.request().postDataJSON() as { request_id: string });
      releaseSecond?.();
      return route.fulfill({ status: 200, body: '' });
    });

    let hit = 0;
    await page.route(/\/api\/chat\/events\/.*/, async (route) => {
      hit += 1;
      const sse = (frames: Array<{ type: string; data: object }>) =>
        route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream(frames),
        });
      if (hit === 1) {
        return sse([permFrame('q-1', '# Draft\n\nfirst\n')]);
      }
      await secondReady;
      return sse([permFrame('q-2', '# Draft\n\nsecond\n')]);
    });

    await page.goto('/');
    await page.getByTestId('session-item-test-session-001').click();
    await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });

    // First request shows.
    await expect(page.getByText('first')).toBeVisible({ timeout: 5000 });
    await story.step(page, 'first queued permission');
    await page.getByRole('button', { name: 'Allow' }).click();

    // After responding, the second arrives on reconnect.
    await expect(page.getByText('second')).toBeVisible({ timeout: 10000 });
    await story.step(page, 'second queued permission');
    await page.getByRole('button', { name: 'Deny' }).click();

    await expect.poll(() => responds.map((r) => r.request_id)).toEqual(['q-1', 'q-2']);
  });
});
