import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';

type CenterPaneState = {
  groupId: string | null;
  tabs: Array<{
    id: string;
    title: string;
    contentType: string;
    metadata?: { sessionId?: string };
  }>;
  activeTabId: string | null;
};

async function getCenterPaneState(page: Page): Promise<CenterPaneState> {
  return page.evaluate(() => {
    const store = (window as unknown as { __windowStore?: any }).__windowStore;

    const findFirstPaneGroupId = (node: any): string | null => {
      if (!node) return null;
      if (node.type === 'pane') return node.tabGroupId ?? null;
      return findFirstPaneGroupId(node.first) || findFirstPaneGroupId(node.second);
    };

    const groupId = store ? findFirstPaneGroupId(store.layout) : null;
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
    await expect(page.getByTestId('new-session-button')).toBeVisible({ timeout: 10000 });
  });

  test('clicking New Session opens a chat tab in center', async ({ page }) => {
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

    const centerBefore = await getCenterPaneState(page);
    expect(centerBefore.tabs).toHaveLength(0);

    const createRequest = page.waitForRequest(
      (req) => req.url().includes('/api/session') && req.method() === 'POST',
    );

    await page.getByTestId('new-session-button').click();
    await createRequest;

    await expect(page.locator('[data-tab-id="tab-chat-test-session-new"]')).toBeVisible();

    await expect
      .poll(async () => {
        const center = await getCenterPaneState(page);
        return center.tabs.length;
      })
      .toBe(1);

    const centerAfter = await getCenterPaneState(page);
    expect(centerAfter.groupId).not.toBeNull();
    expect(centerAfter.tabs[0]?.id).toBe('tab-chat-test-session-new');
    expect(centerAfter.tabs[0]?.contentType).toBe('chat');
    expect(centerAfter.tabs[0]?.metadata?.sessionId).toBe('test-session-new');
    expect(centerAfter.activeTabId).toBe('tab-chat-test-session-new');
  });

  test('clicking an existing session opens its chat tab in center', async ({ page }) => {
    await page.route('**/api/session/test-session-002', (route) => route.fulfill({ json: MOCK_SESSION_2 }));

    const centerBefore = await getCenterPaneState(page);
    expect(centerBefore.tabs).toHaveLength(0);

    const getSessionRequest = page.waitForRequest(
      (req) => req.url().includes('/api/session/test-session-002') && req.method() === 'GET',
    );

    await page.getByTestId('session-item-test-session-002').click();
    await getSessionRequest;

    await expect(page.locator('[data-tab-id="tab-chat-test-session-002"]')).toBeVisible();

    const centerAfter = await getCenterPaneState(page);
    expect(centerAfter.tabs[0]?.id).toBe('tab-chat-test-session-002');
    expect(centerAfter.tabs[0]?.contentType).toBe('chat');
    expect(centerAfter.tabs[0]?.metadata?.sessionId).toBe('test-session-002');
    expect(centerAfter.activeTabId).toBe('tab-chat-test-session-002');
  });
});
