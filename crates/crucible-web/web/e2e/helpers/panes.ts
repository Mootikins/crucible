import { type Page } from '@playwright/test';

/**
 * Give every centre pane a tab.
 *
 * A pane with no tabs yields its width to the side that holds content, and the
 * splitter between them goes inert — the same rule a collapsed pane follows,
 * because `splitRatio` is the size the pane opens back to (see `splitFlex` in
 * `src/windowing/model/pane-collapse.ts` and `locked` in
 * `src/components/windowing/SplitPane.tsx`).
 *
 * So a test that drags a splitter has to put content on BOTH sides. Without
 * this it drags a control the app has deliberately switched off, and measures
 * a width the layout is not being asked to change.
 */
export async function fillCenterPanes(page: Page): Promise<void> {
  await page.evaluate(() => {
    const store = (window as unknown as { __windowStore: any }).__windowStore;
    const actions = (window as unknown as { __windowActions: any }).__windowActions;

    const groupIds = (node: any): string[] => {
      if (node.type === 'pane') return node.tabGroupId ? [node.tabGroupId] : [];
      return [...groupIds(node.first), ...groupIds(node.second)];
    };

    for (const groupId of groupIds(store.layout)) {
      const group = store.tabGroups[groupId];
      // A pane id that resolves to no group cannot be filled — closing a
      // pane's last tab collapses the pane out of the tree and leaves that.
      if (!group || group.tabs.length > 0) continue;
      actions.addTab(groupId, {
        id: `filler-${groupId}`,
        title: 'Filler',
        // The neutral dummy the windowing tests use: it occupies the pane
        // without mounting a panel that fetches.
        contentType: 'tool',
      });
    }
  });
}
