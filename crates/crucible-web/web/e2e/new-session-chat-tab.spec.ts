import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';
import { openSessionsList, startNewSession } from './helpers/nav';

type PaneState = {
  groupId: string | null;
  tabs: Array<{
    id: string;
    title: string;
    contentType: string;
    metadata?: { sessionId?: string };
  }>;
  activeTabId: string | null;
};

const isChat = (t: { contentType: string }) =>
  t.contentType === 'chat' || t.contentType === 'chat-draft';

/**
 * Every centre leaf, in layout order.
 *
 * The sessions rail is fixed on the LEFT (WS-324), so leaf 0 is the centre
 * pane next to it — the pane a session opens in (WS-220).
 */
async function getCentrePanes(page: Page): Promise<PaneState[]> {
  return page.evaluate(() => {
    const store = (window as unknown as { __windowStore?: any }).__windowStore;
    const leaves = (node: any): string[] =>
      !node ? [] : node.type === 'pane'
        ? (node.tabGroupId ? [node.tabGroupId] : [])
        : [...leaves(node.first), ...leaves(node.second)];
    return leaves(store?.layout).map((id: string) => {
      const group = store.tabGroups[id];
      return { groupId: id, tabs: group?.tabs ?? [], activeTabId: group?.activeTabId ?? null };
    });
  });
}

/** The CENTRE pane that holds conversations. A session is a PEER of the
 * editor — its own pane on the sessions rail's side — not a tab in a rail. */
async function getSessionPaneState(page: Page): Promise<PaneState> {
  return (await getCentrePanes(page)).find((p) => p.tabs.some(isChat))
    ?? { groupId: null, tabs: [], activeTabId: null };
}


test.describe('New Session -> Chat Tab', () => {
  test.beforeEach(async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });
    await page.goto('/');
    await openSessionsList(page);
  });

  test('clicking New Session opens a draft; first message opens the chat tab in the centre pane beside the sessions rail', async ({ page }) => {
    const createdSession = {
      ...MOCK_SESSION,
      session_id: 'test-session-new',
      title: 'Brand New Session',
    };

    await page.route('**/api/session', async (route) => {
      if (route.request().method() === 'POST') {
        await route.fulfill({ json: createdSession });
        return;
      }
      await route.fallback();
    });

    // A fresh load gives the centre ONE empty pane, and no conversation.
    const centreBefore = await getCentrePanes(page);
    expect(centreBefore).toHaveLength(1);
    expect(centreBefore[0].tabs.filter(isChat)).toHaveLength(0);

    // Lazy creation: clicking New Session opens a DRAFT surface docked right —
    // nothing hits the daemon until the first message.
    let createdEarly = false;
    page.on('request', (req) => {
      if (req.url().endsWith('/api/session') && req.method() === 'POST') createdEarly = true;
    });

    await startNewSession(page);
    await expect(page.getByTestId('composer-input')).toBeVisible();
    expect(createdEarly).toBe(false);

    await expect
      .poll(async () => {
        const session = await getSessionPaneState(page);
        return session.tabs.filter((t) => t.contentType === 'chat-draft').length;
      })
      .toBe(1);

    // The first message creates the real session and swaps draft → chat tab.
    const createRequest = page.waitForRequest(
      (req) => req.url().includes('/api/session') && req.method() === 'POST',
    );
    await page.getByTestId('composer-input').fill('Hello from the draft');
    await page.getByTestId('composer-send').click();
    await createRequest;

    await expect(page.locator('[data-tab-id="tab-chat-test-session-new"]')).toBeVisible();

    await expect
      .poll(async () => {
        const session = await getSessionPaneState(page);
        return session.tabs.filter((t) => t.contentType === 'chat').length;
      })
      .toBe(1);

    const sessionAfter = await getSessionPaneState(page);
    expect(sessionAfter.groupId).not.toBeNull();
    // The draft closed itself once the real session opened.
    expect(sessionAfter.tabs.filter((t) => t.contentType === 'chat-draft')).toHaveLength(0);
    const chatTab = sessionAfter.tabs.find((t) => t.contentType === 'chat');
    expect(chatTab?.id).toBe('tab-chat-test-session-new');
    expect(chatTab?.metadata?.sessionId).toBe('test-session-new');
    expect(sessionAfter.activeTabId).toBe('tab-chat-test-session-new');

    // The session sits in the centre pane next to the sessions rail, and it
    // OCCUPIED the empty pane: the centre is still one pane, not a chat beside
    // an empty editor.
    const centreAfter = await getCentrePanes(page);
    expect(centreAfter).toHaveLength(1);
    expect(centreAfter[0].groupId).toBe(sessionAfter.groupId);
  });

  test('clicking an existing session opens its chat tab in the centre pane beside the sessions rail', async ({ page }) => {
    await page.route('**/api/session/test-session-002', (route) => route.fulfill({ json: MOCK_SESSION_2 }));

    const centreBefore = await getCentrePanes(page);
    expect(centreBefore).toHaveLength(1);
    expect(centreBefore[0].tabs.filter(isChat)).toHaveLength(0);

    const getSessionRequest = page.waitForRequest(
      (req) => req.url().includes('/api/session/test-session-002') && req.method() === 'GET',
    );

    await page.getByTestId('session-item-test-session-002').click();
    await getSessionRequest;

    await expect(page.locator('[data-tab-id="tab-chat-test-session-002"]')).toBeVisible();

    const sessionAfter = await getSessionPaneState(page);
    const chatTab = sessionAfter.tabs.find((t) => t.contentType === 'chat');
    expect(chatTab?.id).toBe('tab-chat-test-session-002');
    expect(chatTab?.metadata?.sessionId).toBe('test-session-002');
    expect(sessionAfter.activeTabId).toBe('tab-chat-test-session-002');

    // The session OCCUPIED the empty centre pane next to the sessions rail.
    const centreAfter = await getCentrePanes(page);
    expect(centreAfter).toHaveLength(1);
    expect(centreAfter[0].groupId).toBe(sessionAfter.groupId);
  });
});
