import type { WindowPolicy } from '@/windowing/store/policy';
import type { WindowState } from '@/windowing/model/types';
import { emptyState, findFirstPane } from '@/windowing/model/tree';
import { LAYOUT_SHORTCUTS } from '@/windowing/shortcuts';

/**
 * The policy with no product knowledge. The core's unit tests and the
 * harness page (`windowing-harness.html`) use this one module, so the two
 * cannot drift apart.
 *
 * No production module imports this folder, so it never reaches `dist/`.
 */

/** The three content types of the neutral seed. */
type NeutralType = 'alpha' | 'beta' | 'gamma';

/** The tab group of the first pane in a layout. The empty state has one in each place. */
function firstGroup(s: WindowState, layout: WindowState['layout']): string {
  const id = findFirstPane(layout)?.tabGroupId;
  if (!id || !s.tabGroups[id]) throw new Error('neutralSeed: the empty state has no group here');
  return id;
}

/**
 * The empty state with tabs in it. The centre holds `tab-alpha` (active)
 * and `tab-beta`. The docked left rail holds `tab-left`. The right rail is
 * a strip that holds `tab-right`. Nothing else.
 */
function neutralSeed(): WindowState<NeutralType> {
  const s = emptyState<NeutralType>();
  const fill = (groupId: string, tabs: WindowState<NeutralType>['tabGroups'][string]['tabs']) => {
    const g = s.tabGroups[groupId]!;
    g.tabs = tabs;
    g.activeTabId = tabs[0]?.id ?? null;
  };
  fill(firstGroup(s, s.layout), [
    { id: 'tab-alpha', title: 'Alpha', contentType: 'alpha' },
    { id: 'tab-beta', title: 'Beta', contentType: 'beta' },
  ]);
  fill(firstGroup(s, s.edgePanels.left.layout), [
    { id: 'tab-left', title: 'Left', contentType: 'gamma' },
  ]);
  fill(firstGroup(s, s.edgePanels.right.layout), [
    { id: 'tab-right', title: 'Right', contentType: 'gamma' },
  ]);
  return s;
}

/**
 * Each member does the least thing: every tab may close, no repair, no
 * icon, no reason to grey a tab out. The chord table is the layout's own.
 * The legacy upgrade throws, because nothing here reads a payload older
 * than v9. A test replaces a member with `over`.
 */
export function neutralPolicy(over: Partial<WindowPolicy> = {}): WindowPolicy {
  return {
    seed: neutralSeed,
    mayCloseTab: () => true,
    repairLayout: () => {},
    onActiveTabChange: () => {},
    iconFor: () => undefined,
    unavailableReason: () => null,
    layoutHooks: {
      upgradeLegacy: () => {
        throw new Error('neutralPolicy: no legacy layouts in the windowing core');
      },
      prune: () => {},
    },
    shortcuts: LAYOUT_SHORTCUTS,
    onShortcut: () => false,
    ...over,
  };
}
