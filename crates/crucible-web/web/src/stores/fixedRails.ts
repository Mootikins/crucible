import type { EdgePanelPosition, Tab, TabContentType, WindowState } from '@/types/windowTypes';
import { iconForContentType } from '@/lib/tab-icons';
import { collectLeafGroupIds, findFirstPane, generateId } from '@/windowing';

/**
 * The two panels the shell always keeps, and the rail each one belongs to.
 *
 * Sessions on the left and Files on the right are not tabs like the others:
 * they are the two ways INTO the app. Everything else opens from one of them,
 * or from the palette, so a shell with neither is a shell with no doorway —
 * and that is the state a user reached, because a rail could be emptied and a
 * restore brought it back empty. `swapSidePanels` still moves them as a pair,
 * so the rule names a DEFAULT side, not a fixed one.
 *
 * The title travels with the rule rather than being read from the panel
 * registry: this store must be able to repair a layout during boot, before
 * anything registers a panel, and `Sessions` / `Files` are the registry's
 * titles anyway (see lib/register-panels.tsx).
 */
const FIXED_RAIL_PANELS: Record<
  EdgePanelPosition,
  { contentType: TabContentType; title: string }
> = {
  left: { contentType: 'sessions', title: 'Sessions' },
  right: { contentType: 'files', title: 'Files' },
};

const FIXED_RAIL_TYPES: TabContentType[] = Object.values(FIXED_RAIL_PANELS).map(
  (p) => p.contentType,
);

/** Every tab of one content type, in every group, wherever it is docked. */
function tabsOfType(s: WindowState, contentType: TabContentType): Tab[] {
  return Object.values(s.tabGroups).flatMap((g) =>
    g.tabs.filter((t) => t.contentType === contentType),
  );
}

/**
 * True when closing this tab would leave the shell with no Sessions panel, or
 * no Files panel.
 *
 * The rule is "never zero", not "never leaves the rail". A user may drag
 * Sessions into the centre and keep working, and may close a SECOND copy of
 * it; what nothing may do is take the last one away.
 */
export function isLastFixedRailTab(
  s: WindowState,
  groupId: string,
  tabId: string,
): boolean {
  const tab = s.tabGroups[groupId]?.tabs.find((t) => t.id === tabId);
  if (!tab || !FIXED_RAIL_TYPES.includes(tab.contentType)) return false;
  return tabsOfType(s, tab.contentType).length <= 1;
}

/** The rail that currently holds one content type, or undefined. */
function railHolding(
  s: WindowState,
  contentType: TabContentType,
): EdgePanelPosition | undefined {
  return (['left', 'right'] as EdgePanelPosition[]).find((pos) =>
    collectLeafGroupIds(s.edgePanels[pos].layout).some((id) =>
      s.tabGroups[id]?.tabs.some((t) => t.contentType === contentType),
    ),
  );
}

/** Put one fixed panel back on a rail, and open that rail so the repair shows. */
function addFixedRailTab(
  s: WindowState,
  pos: EdgePanelPosition,
  panel: { contentType: TabContentType; title: string },
): void {
  const tab: Tab = {
    id: `${panel.contentType}-tab`,
    title: panel.title,
    contentType: panel.contentType,
    icon: iconForContentType(panel.contentType),
  };
  let groupId = collectLeafGroupIds(s.edgePanels[pos].layout).find((id) => s.tabGroups[id]);
  if (!groupId) {
    // The rail has no resolvable group — a restore of a layout with no panel
    // at this position synthesizes a pane with a null tab group.
    groupId = generateId();
    s.tabGroups[groupId] = { id: groupId, tabs: [], activeTabId: null };
    const pane = findFirstPane(s.edgePanels[pos].layout);
    if (pane) pane.tabGroupId = groupId;
    else s.edgePanels[pos].layout = { id: `${pos}-pane`, type: 'pane', tabGroupId: groupId };
  }
  const group = s.tabGroups[groupId];
  // Leading, and active: the rail's own panel reads first on its tab strip,
  // and a repair the user cannot see is not a repair.
  group.tabs = [tab, ...group.tabs];
  group.activeTabId = tab.id;
  s.edgePanels[pos].mode = 'docked';
}

/**
 * Put back any fixed panel the incoming layout lost. Mutates the draft.
 *
 * Runs on the two paths that write a whole layout — restore and reset — which
 * are the only ways a state the store did not build enters it. Nothing is
 * added while a copy is open anywhere else, so a user who docked Sessions in
 * the centre keeps one Sessions panel, not two.
 */
export function ensureFixedRails(s: WindowState): void {
  for (const pos of ['left', 'right'] as EdgePanelPosition[]) {
    const panel = FIXED_RAIL_PANELS[pos];
    if (tabsOfType(s, panel.contentType).length > 0) continue;
    const other: EdgePanelPosition = pos === 'left' ? 'right' : 'left';
    // A swapped layout holds the other fixed panel on this side. Re-add on
    // the free rail rather than stacking both on one.
    const target = railHolding(s, FIXED_RAIL_PANELS[other].contentType) === pos ? other : pos;
    addFixedRailTab(s, target, panel);
  }
}
