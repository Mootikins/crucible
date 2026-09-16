import { expect, request, type Page } from '@playwright/test';

/**
 * Mounting a panel from a live spec, without a tour of the shell.
 *
 * Part E's claims are about FETCHES: how many a surface makes when it mounts,
 * and how many a SECOND surface reading the same entity adds. Reaching each
 * panel through its own ribbon button, palette entry or edge tab would make
 * every one of those claims depend on an affordance that has moved twice
 * already, and a spec that cannot open the Surfaces panel proves nothing about
 * the surfaces cache.
 *
 * So these helpers drive the window store directly, through the
 * `window.__windowActions` seam the shell already exposes and
 * `e2e/helpers/nav.ts` already uses. The panel that mounts is the real one, it
 * fetches through the real hooks, and the daemon answers for real — only the
 * gesture that opened it is synthetic.
 */

const MOUNT_TIMEOUT = 15_000;

/** Where a tab goes: an edge rail, or the centre. */
export type PaneRegion = 'left' | 'right' | 'center';

/** A tab to mount, as the window store names one. */
export interface TabSpec {
  id: string;
  title: string;
  contentType: string;
  metadata?: Record<string, unknown>;
}

/** The first tab group of a region's layout, or null when it has no pane. */
async function firstGroupId(page: Page, region: PaneRegion): Promise<string | null> {
  return page.evaluate((where) => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const walk = (node: any): string | null => {
      if (!node || typeof node !== 'object') return null;
      if (node.type === 'pane') return node.tabGroupId ?? null;
      return walk(node.first) ?? walk(node.second);
    };
    return walk(where === 'center' ? store.layout : store.edgePanels[where].layout);
  }, region);
}

/**
 * Adds a tab to a region's first pane and makes it the active one.
 *
 * Answers the group it landed in, so a caller can close the tab again — the
 * refcount specs need the close as much as the open.
 */
export async function mountTab(
  page: Page,
  region: PaneRegion,
  tab: TabSpec,
): Promise<string> {
  const groupId = await firstGroupId(page, region);
  expect(groupId, `no pane in the ${region} region to mount ${tab.contentType} in`).toBeTruthy();
  await page.evaluate(
    ([group, spec]) => {
      const actions = (window as unknown as Record<string, any>).__windowActions;
      actions.addTab(group as string, spec);
      actions.setActiveTab(group as string, (spec as { id: string }).id);
    },
    [groupId!, tab] as const,
  );
  return groupId!;
}

/** Removes a tab again, so its panel unmounts and releases what it held. */
export async function closeTab(page: Page, groupId: string, tabId: string): Promise<void> {
  await page.evaluate(
    ([group, id]) => {
      (window as unknown as Record<string, any>).__windowActions.removeTab(group, id);
    },
    [groupId, tabId] as const,
  );
}

/**
 * Opens a tab in a NEW pane beside the centre's first one, and answers its
 * tab group.
 *
 * `openTabInNewPane`, not `splitPane`: a split copies the original group's
 * tabs into the first half and leaves the second EMPTY, and it deletes the
 * group id the caller was holding. A spec that then added its second chat tab
 * to "the new group" could land it in the half that already had one, where a
 * tab group shows only its active tab — one visible pane, and a claim about
 * two panes that quietly tested one.
 *
 * Two PANES, not two tabs in one, because an inactive tab is not mounted, and
 * an unmounted pane subscribes to nothing.
 */
export async function openInNewPane(page: Page, tab: TabSpec): Promise<string> {
  const groupId = await page.evaluate((spec) => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    const firstPane = (node: any): string | null => {
      if (!node || typeof node !== 'object') return null;
      if (node.type === 'pane') return node.id ?? null;
      return firstPane(node.first) ?? firstPane(node.second);
    };
    const paneId = firstPane(store.layout);
    if (!paneId) throw new Error('openInNewPane: the centre region has no pane');
    return actions.openTabInNewPane(paneId, 'right', spec) as string | null;
  }, tab);
  expect(groupId, 'openTabInNewPane answered no group').toBeTruthy();
  return groupId!;
}

/** Every tab group in the centre layout, in tree order. */
export async function centerGroupIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const found: string[] = [];
    const walk = (node: any): void => {
      if (!node || typeof node !== 'object') return;
      if (node.type === 'pane') {
        if (node.tabGroupId) found.push(node.tabGroupId);
        return;
      }
      walk(node.first);
      walk(node.second);
    };
    walk(store.layout);
    return found;
  });
}

/** The tab a bound chat pane is, as the window store names one. */
export function chatTab(sessionId: string, tabId: string): TabSpec {
  return {
    id: tabId,
    title: 'Chat',
    contentType: 'chat',
    metadata: { sessionId },
  };
}

/** Adds a chat tab for `sessionId` to `groupId` and activates it. */
export async function mountChat(
  page: Page,
  groupId: string,
  sessionId: string,
  tabId: string,
): Promise<void> {
  await page.evaluate(
    ([group, session, id]) => {
      const actions = (window as unknown as Record<string, any>).__windowActions;
      actions.addTab(group, {
        id,
        title: 'Chat',
        contentType: 'chat',
        metadata: { sessionId: session },
      });
      actions.setActiveTab(group, id);
    },
    [groupId, sessionId, tabId] as const,
  );
}

/**
 * Throws away the layout the daemon holds for this browser profile.
 *
 * The shell SAVES its layout (`POST /api/layout`), and the daemon keeps it
 * across page loads and across specs. So a spec that mounts a Skills panel
 * leaves one mounted for the next spec, whose "one page load reads X once"
 * count then includes a panel it never opened. Run this BEFORE `page.goto`
 * and every spec starts on `createInitialState()`, the layout a fresh profile
 * gets.
 *
 * `DELETE /api/layout` is the product's own reset (Settings → Reset layout),
 * so this uses the route the user has, not a back door around it.
 */
export async function resetStoredLayout(baseURL: string): Promise<void> {
  let ctx: Awaited<ReturnType<typeof request.newContext>> | null = null;
  try {
    ctx = await request.newContext({ baseURL });
    // 404 means there was nothing stored, which is the state this asks for.
    await ctx.delete('/api/layout');
  } catch (error) {
    // Deliberately swallowed. This runs in `beforeEach` and in `afterAll`, and
    // a hook that THROWS does not fail one test: Playwright treats it as a
    // broken worker and skips every test left in the run, so one hiccup here
    // would turn a whole tier green-by-absence. The specs assert about the
    // layout themselves; a reset that did not happen shows up there.
    console.warn(`live: could not reset the stored layout: ${String(error).slice(0, 200)}`);
  } finally {
    await ctx?.dispose().catch(() => undefined);
  }
}

/**
 * Opens the right rail on its file tree, from whatever state it is in.
 *
 * The rail starts collapsed on a fresh layout, so the toggle is needed; but a
 * spec that has already opened it would CLOSE it by toggling again. The
 * dropdown that names the browse root is the probe for "already open", because
 * it is the one control the tree always renders.
 */
export async function openFileTree(page: Page): Promise<void> {
  const dropdown = page.getByTestId('root-dropdown').first();
  if (!(await dropdown.isVisible().catch(() => false))) {
    await page.getByTestId('ribbon-toggle-right').click();
  }
  const tab = page.getByTestId('edge-tab-right-files-tab');
  await expect(tab).toBeVisible({ timeout: MOUNT_TIMEOUT });
  if (!(await dropdown.isVisible().catch(() => false))) {
    await tab.click();
  }
  await expect(dropdown).toBeVisible({ timeout: MOUNT_TIMEOUT });
}

/**
 * Picks a browse root by the name the daemon's roster gives it.
 *
 * The wait is on the POPOUT closing, not on the trigger's label. The trigger
 * is the current-root display, but it truncates and it carries an `aria-label`
 * of its own, so matching its text is matching a presentation detail. A closed
 * popout is what the click did; what the click MEANT is the fetch each caller
 * counts for itself.
 */
export async function selectRoot(page: Page, name: string): Promise<void> {
  await page.getByTestId('root-dropdown').first().click();
  const popout = page.getByTestId('root-dropdown-popout').first();
  await expect(popout).toBeVisible({ timeout: MOUNT_TIMEOUT });
  await popout.getByText(name, { exact: true }).first().click();
  await expect(popout).toBeHidden({ timeout: MOUNT_TIMEOUT });
}
