import { setStore, windowActions, windowStore } from '@/windowing/store';

/**
 * Stack a second pane under the right rail of the neutral seed.
 *
 * The neutral seed gives each rail one pane. Some core tests need a rail
 * that holds a column: the pane `right-pane` above a collapsed pane
 * `right-term-pane`, in the vertical split `right-split`. The upper pane
 * keeps the seed group. The lower pane gets a new group with one tab.
 */
export function stackRightRail(): void {
  const top = windowStore.edgePanels.right.layout;
  if (top.type !== 'pane') throw new Error('stackRightRail: the right rail is not one pane');
  const lowerGroup = windowActions.createTabGroup();
  windowActions.addTab(lowerGroup, { id: 'tab-right-lower', title: 'Lower', contentType: 'gamma' });
  setStore('edgePanels', 'right', 'layout', {
    id: 'right-split',
    type: 'split',
    direction: 'vertical',
    splitRatio: 0.65,
    first: { ...top, id: 'right-pane' },
    second: { id: 'right-term-pane', type: 'pane', tabGroupId: lowerGroup, collapsed: true },
  });
}
