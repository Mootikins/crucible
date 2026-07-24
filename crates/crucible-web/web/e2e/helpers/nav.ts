import { expect, type Page } from '@playwright/test';

/**
 * Shell navigation helpers.
 *
 * Two shell changes broke the old inline selectors across the suite, so the
 * knowledge lives here instead of in 20 specs:
 *
 *  - The unified Navigator replaced the separate Files / Sessions left tabs.
 *    It opens in FILES scope, so the sessions list (and every
 *    `session-item-*`) only exists after switching scope.
 *  - An empty pane is void. The session composer is no longer an empty-center
 *    splash; it is the content of a New Session tab, opened from the ribbon.
 */

const READY_TIMEOUT = 15000;

/** Resolves once the app shell has painted (ribbon is up). */
export async function appReady(page: Page): Promise<void> {
  await expect(page.getByTestId('ribbon-cmd-new-session')).toBeVisible({
    timeout: READY_TIMEOUT,
  });
}

/** Put the Navigator in Sessions scope so `session-item-*` rows exist. */
export async function openSessionsList(page: Page): Promise<void> {
  const swapper = page.getByTestId('navigator-swapper');
  await expect(swapper).toBeVisible({ timeout: READY_TIMEOUT });
  // The swapper's label IS the current scope — skip if we're already there.
  if ((await swapper.textContent())?.includes('Sessions')) return;
  await swapper.click();
  await page.getByTestId('navigator-scope-sessions').click();
  await expect(page.getByTestId('new-session-button')).toBeVisible({
    timeout: READY_TIMEOUT,
  });
}

/** Click a session row in the Navigator, switching scope first if needed. */
export async function openSession(page: Page, sessionId: string): Promise<void> {
  await openSessionsList(page);
  const row = page.getByTestId(`session-item-${sessionId}`);
  await expect(row).toBeVisible({ timeout: READY_TIMEOUT });
  await row.click();
}

/**
 * Add scratch tabs to the left edge panel for tab-strip DnD tests.
 *
 * The default left roster is a single Navigator tab, so reorder/cross-zone
 * specs seed their own instead of leaning on whatever the shell happens to
 * ship — the roster changing is exactly what broke them before.
 */
export async function seedLeftTabs(
  page: Page,
  tabs: { id: string; title: string }[],
): Promise<void> {
  await page.evaluate((seed) => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    const firstGroup = (node: any): string | null => {
      if (!node || typeof node !== 'object') return null;
      if (node.type === 'pane') return node.tabGroupId ?? null;
      return firstGroup(node.first) ?? firstGroup(node.second);
    };
    const groupId = firstGroup(store.edgePanels.left.layout);
    if (!groupId) throw new Error('seedLeftTabs: left edge panel has no tab group');
    const wasActive = store.tabGroups[groupId]?.activeTabId ?? null;
    for (const t of seed) {
      actions.addTab(groupId, { id: t.id, title: t.title, contentType: 'backlinks' });
    }
    // addTab activates what it adds; keep the panel showing what it showed
    // (callers seed a strip to drag, they don't want the body swapped).
    if (wasActive) actions.setActiveTab(groupId, wasActive);
  }, tabs);
  for (const t of tabs) {
    await expect(page.locator(`[data-testid="edge-tab-left-${t.id}"]`)).toBeVisible({
      timeout: READY_TIMEOUT,
    });
  }
}

/** Open a New Session tab from the ribbon and wait for its composer. */
export async function openNewSessionTab(page: Page): Promise<void> {
  await appReady(page);
  await page.getByTestId('ribbon-cmd-new-session').click();
  await expect(page.getByTestId('composer-input')).toBeVisible({
    timeout: READY_TIMEOUT,
  });
}
