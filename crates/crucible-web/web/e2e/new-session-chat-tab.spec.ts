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

async function getFirstPaneState(page: Page): Promise<PaneState> {
  return page.evaluate(() => {
    const store = (window as unknown as { __windowStore?: any }).__windowStore;

    // The EDITOR group: the first centre leaf that is not a conversation. It
    // used to be simply "the first leaf", which stopped meaning the editor the
    // moment a session began opening as a pane to its LEFT.
    const leaves = (node: any): string[] =>
      !node ? [] : node.type === 'pane'
        ? (node.tabGroupId ? [node.tabGroupId] : [])
        : [...leaves(node.first), ...leaves(node.second)];
    const ids = store ? leaves(store.layout) : [];
    const isChat = (t: any) => t.contentType === 'chat' || t.contentType === 'chat-draft';
    const groupId =
      ids.find((id: string) => !(store.tabGroups[id]?.tabs ?? []).some(isChat)) ?? ids[0] ?? null;
    const group = groupId ? store.tabGroups[groupId] : null;

    return {
      groupId,
      tabs: group?.tabs ?? [],
      activeTabId: group?.activeTabId ?? null,
    };
  });
}

/** The CENTRE group holding conversations. A session is a PEER of the editor
 * now — its own pane, left of it — not a tab docked in a rail. */
async function getRightPaneState(page: Page): Promise<PaneState> {
  return page.evaluate(() => {
    const store = (window as unknown as { __windowStore?: any }).__windowStore;
    const leaves = (node: any): string[] =>
      !node ? [] : node.type === 'pane'
        ? (node.tabGroupId ? [node.tabGroupId] : [])
        : [...leaves(node.first), ...leaves(node.second)];
    const groupId =
      leaves(store?.layout).find((id: string) =>
        (store.tabGroups[id]?.tabs ?? []).some((t: any) =>
          t.contentType === 'chat' || t.contentType === 'chat-draft')) ?? null;
    const group = groupId ? store.tabGroups[groupId] : null;

    return {
      groupId,
      tabs: group?.tabs ?? [],
      activeTabId: group?.activeTabId ?? null,
    };
  });
}

test.describe('New Session -> Chat Tab', () => {
  test.beforeEach(async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });
    await page.goto('/');
    await openSessionsList(page);
  });

  test('clicking New Session opens a draft; first message creates the chat tab beside the editor', async ({ page }) => {
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

    // Fresh load lands on Home (the shell's landing tab) in the left/first pane.
    const leftBefore = await getFirstPaneState(page);
    expect(leftBefore.tabs.filter((t) => t.contentType === 'chat')).toHaveLength(0);

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
        const right = await getRightPaneState(page);
        return right.tabs.filter((t) => t.contentType === 'chat-draft').length;
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
        const right = await getRightPaneState(page);
        return right.tabs.filter((t) => t.contentType === 'chat').length;
      })
      .toBe(1);

    const rightAfter = await getRightPaneState(page);
    expect(rightAfter.groupId).not.toBeNull();
    // The draft closed itself once the real session opened.
    expect(rightAfter.tabs.filter((t) => t.contentType === 'chat-draft')).toHaveLength(0);
    const chatTab = rightAfter.tabs.find((t) => t.contentType === 'chat');
    expect(chatTab?.id).toBe('tab-chat-test-session-new');
    expect(chatTab?.metadata?.sessionId).toBe('test-session-new');
    expect(rightAfter.activeTabId).toBe('tab-chat-test-session-new');

    // The left pane keeps its original (non-chat) tabs — sessions never land there.
    const leftAfter = await getFirstPaneState(page);
    expect(leftAfter.tabs.filter((t) => t.contentType === 'chat')).toHaveLength(0);
  });

  test('clicking an existing session opens its chat tab beside the editor', async ({ page }) => {
    await page.route('**/api/session/test-session-002', (route) => route.fulfill({ json: MOCK_SESSION_2 }));

    const leftBefore = await getFirstPaneState(page);
    expect(leftBefore.tabs.filter((t) => t.contentType === 'chat')).toHaveLength(0);

    const getSessionRequest = page.waitForRequest(
      (req) => req.url().includes('/api/session/test-session-002') && req.method() === 'GET',
    );

    await page.getByTestId('session-item-test-session-002').click();
    await getSessionRequest;

    await expect(page.locator('[data-tab-id="tab-chat-test-session-002"]')).toBeVisible();

    const rightAfter = await getRightPaneState(page);
    const chatTab = rightAfter.tabs.find((t) => t.contentType === 'chat');
    expect(chatTab?.id).toBe('tab-chat-test-session-002');
    expect(chatTab?.metadata?.sessionId).toBe('test-session-002');
    expect(rightAfter.activeTabId).toBe('tab-chat-test-session-002');

    // The left pane did not gain a chat tab.
    const leftAfter = await getFirstPaneState(page);
    expect(leftAfter.tabs.filter((t) => t.contentType === 'chat')).toHaveLength(0);
  });
});
