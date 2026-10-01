import type { EdgePanelPosition, Tab, TabContentType, WindowState } from '@/types/windowTypes';
import { iconForContentType } from '@/lib/tab-icons';
import { collectLeafGroupIds, findFirstPane, generateId } from '@/windowing';

/** Navigation panels are always present, including after layout repair. */
const FIXED_RAIL_PANELS = [
  { contentType: 'sessions', title: 'Sessions' },
  { contentType: 'files', title: 'Files' },
] as const;
const FIXED_RAIL_TYPES: readonly TabContentType[] = FIXED_RAIL_PANELS.map((panel) => panel.contentType);

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
  if (s.tabGroups[groupId].tabs.length > 0) {
    const nextGroup = generateId();
    s.tabGroups[nextGroup] = { id: nextGroup, tabs: [], activeTabId: null };
    s.edgePanels[pos].layout = {
      id: generateId(), type: 'split', direction: 'vertical', splitRatio: 0.4,
      first: s.edgePanels[pos].layout,
      second: { id: generateId(), type: 'pane', tabGroupId: nextGroup },
    };
    groupId = nextGroup;
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
  migrateNavigationLayout(s);
  for (const panel of FIXED_RAIL_PANELS) {
    if (tabsOfType(s, panel.contentType).length > 0) continue;
    const target = railHolding(s, 'sessions') ?? railHolding(s, 'files') ?? 'left';
    addFixedRailTab(s, target, panel);
  }
}

/** Stored pre-migration default layouts put Files above Terminal. Move that
 * known skeleton as a whole; custom trees and all tab identities stay intact. */
function migrateNavigationLayout(s: WindowState): void {
  const nav = railHolding(s, 'sessions');
  if (!nav) return;
  const conversation = nav === 'left' ? 'right' : 'left';
  const navTree = s.edgePanels[nav].layout;
  const old = s.edgePanels[conversation].layout;
  if (navTree.type !== 'pane' || old.type !== 'split' || old.direction !== 'vertical') return;
  if (old.first.type !== 'pane' || old.second.type !== 'pane') return;
  const files = old.first.tabGroupId && s.tabGroups[old.first.tabGroupId];
  const terminal = old.second.tabGroupId && s.tabGroups[old.second.tabGroupId];
  if (!files || !terminal || !files.tabs.some((tab) => tab.contentType === 'files') ||
      !terminal.tabs.every((tab) => tab.contentType === 'terminal')) return;
  const groupId = generateId();
  s.tabGroups[groupId] = { id: groupId, tabs: [], activeTabId: null };
  s.edgePanels[nav].layout = {
    id: generateId(), type: 'split', direction: 'vertical', splitRatio: 0.4,
    first: navTree, second: old.first,
  };
  s.edgePanels[conversation].layout = {
    ...old, first: { id: generateId(), type: 'pane', tabGroupId: groupId },
  };
  s.edgePanels[conversation].width = Math.max(s.edgePanels[conversation].width ?? 0, 400);
}
