import type { WindowPolicy } from '@/windowing/store/policy';
import { emptyState } from '@/windowing/model/tree';

/**
 * A policy with no product knowledge, for the core's own tests.
 *
 * The seed is the empty shape with one tab `t1` in the centre group. Each
 * member does the least thing: every tab may close, no repair, no icon. The
 * legacy upgrade throws, because a core test never reads a payload older
 * than v9.
 */
export function stubPolicy(over: Partial<WindowPolicy> = {}): WindowPolicy {
  return {
    seed: () => {
      const s = emptyState();
      const centre = s.layout.type === 'pane' ? s.layout.tabGroupId : null;
      const g = centre ? s.tabGroups[centre] : undefined;
      if (g) {
        g.tabs = [{ id: 't1', title: 'One', contentType: 'alpha' }];
        g.activeTabId = 't1';
      }
      return s;
    },
    mayCloseTab: () => true,
    repairLayout: () => {},
    onActiveTabChange: () => {},
    iconFor: () => undefined,
    tabAvailable: () => true,
    layoutHooks: {
      upgradeLegacy: () => {
        throw new Error('stubPolicy: no legacy layouts in core tests');
      },
      prune: () => {},
      iconFor: () => undefined,
    },
    shortcuts: [],
    onShortcut: () => false,
    ...over,
  };
}
