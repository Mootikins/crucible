import type {
  EdgePanelPosition,
  LayoutNode,
  PaneNode,
  TabContentType,
  TabGroup,
} from '@/types/windowTypes';
import type {
  LayoutCodecHooks,
  RestoredLayout,
  SerializedEdgePanelV9,
  SerializedLayoutV9,
  SerializedTab as CoreSerializedTab,
  SerializedTabGroup as CoreSerializedTabGroup,
} from '@/windowing/model/serializer';
import { iconForContentType } from '@/lib/tab-icons';
import { getGlobalRegistry } from '@/lib/panel-registry';

/**
 * The saved layout history of this app, from v1 to v9, and the app hooks of
 * the core layout reader.
 *
 * Each step here names content this app had at the time, so the steps stay
 * with the app. The core reader in `@/windowing/model/serializer` owns the
 * current format and the step from v9.
 */

/**
 * A stored layout at any version from 1 to 9. The steps below share one
 * loose shape: each one reads the shape its version really had, and a v9
 * layout is the result.
 */
type SerializedLayout = Omit<SerializedLayoutV9<TabContentType>, 'version'> & {
  version: number;
};
type SerializedTabGroup = CoreSerializedTabGroup<TabContentType>;
type SerializedTab = CoreSerializedTab<TabContentType>;
type SerializedEdgePanel = SerializedEdgePanelV9;

/** Pre-v5 edge panel shape: one tab group, no layout tree. */
interface LegacySerializedEdgePanel {
  id: string;
  tabGroupId: string;
  isCollapsed?: boolean;
  width?: number;
  height?: number;
}

/** Leaf group ids of a serialized edge panel, tolerating both shapes (the
 * shared migration helpers run on pre-v5 objects too). */
function edgePanelGroupIds(
  panel: SerializedEdgePanel | LegacySerializedEdgePanel
): string[] {
  if ('layout' in panel && panel.layout) {
    const ids: string[] = [];
    const walk = (n: LayoutNode): void => {
      if (n.type === 'pane') {
        if (n.tabGroupId) ids.push(n.tabGroupId);
      } else {
        walk(n.first);
        walk(n.second);
      }
    };
    walk(panel.layout);
    return ids;
  }
  const legacy = panel as LegacySerializedEdgePanel;
  return legacy.tabGroupId ? [legacy.tabGroupId] : [];
}

/** Group ids reachable from the layout tree, edge panels, and floating windows. */
function referencedGroupIds(layout: SerializedLayout): Set<string> {
  const ids = new Set<string>();
  const walk = (node: any) => {
    if (!node) return;
    if (node.type === 'pane') {
      if (node.tabGroupId) ids.add(node.tabGroupId);
    } else if (node.type === 'split') {
      walk(node.first);
      walk(node.second);
    }
  };
  walk(layout.layout);
  for (const panel of Object.values(layout.edgePanels)) {
    for (const id of edgePanelGroupIds(panel)) ids.add(id);
  }
  for (const w of layout.floatingWindows) ids.add((w as { tabGroupId: string }).tabGroupId);
  return ids;
}

// Drop tabs whose content type is no longer registered — the panel roster
// shrank (removed placeholder panels), and a persisted layout would otherwise
// resurrect ghost tabs that render "Unknown content type". Empty groups that
// nothing references are dropped; a referenced group may go empty (renders an
// empty state) so its pane/edge/window reference stays valid.
function migrateV2toV3(v2: SerializedLayout): SerializedLayout {
  const registry = getGlobalRegistry();
  // If the registry isn't populated yet (defensive — registerPanels() runs
  // before layout load in practice), skip pruning rather than nuke every tab.
  if (registry.list().length === 0) {
    return { ...v2, version: 3 };
  }
  const isKnown = (contentType: string) => registry.get(contentType) !== undefined;

  const tabGroups: Record<string, SerializedTabGroup> = {};
  for (const [id, group] of Object.entries(v2.tabGroups)) {
    const tabs = group.tabs.filter((t) => isKnown(t.contentType));
    const activeTabId =
      group.activeTabId && tabs.some((t) => t.id === group.activeTabId)
        ? group.activeTabId
        : (tabs[0]?.id ?? null);
    tabGroups[id] = { id: group.id, tabs, activeTabId };
  }

  const referenced = referencedGroupIds(v2);
  for (const [id, group] of Object.entries(tabGroups)) {
    if (group.tabs.length === 0 && !referenced.has(id)) {
      delete tabGroups[id];
    }
  }

  return { ...v2, version: 3, tabGroups };
}

// Sessions dock in the right edge panel, which pre-v4 defaulted to a
// 250px sliver — unusable for chat. One-time bump of persisted narrow
// widths to the new default; widths the user already dragged past it are
// left alone.
function migrateV3toV4(v3: SerializedLayout): SerializedLayout {
  const right = v3.edgePanels?.right;
  const edgePanels = right && (right.width ?? 0) < 520
    ? { ...v3.edgePanels, right: { ...right, width: 520 } }
    : v3.edgePanels;
  return { ...v3, version: 4, edgePanels };
}

// Edge panels grew full layout trees (splittable like the center tiling):
// the single `tabGroupId` becomes a one-pane tree.
function migrateV4toV5(v4: SerializedLayout): SerializedLayout {
  const edgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(v4.edgePanels)) {
    const legacy = panel as unknown as LegacySerializedEdgePanel;
    edgePanels[pos as EdgePanelPosition] =
      'layout' in panel && panel.layout
        ? (panel as SerializedEdgePanel)
        : {
            id: legacy.id,
            layout: {
              id: `${legacy.id}-pane`,
              type: 'pane',
              tabGroupId: legacy.tabGroupId,
            },
            isCollapsed: legacy.isCollapsed,
            width: legacy.width,
            height: legacy.height,
          };
  }
  return { ...v4, version: 5, edgePanels };
}

// The Navigator was ONE left panel whose 'files' and 'sessions' scopes were
// mutually exclusive, so reading a file hid the session list. It splits into
// a Sessions rail on the left and a Files rail on the right.
//
// A persisted Navigator tab becomes the Sessions tab IN PLACE — wherever the
// user docked it is where the session list lands. Files is then added only
// when the layout holds none. The version bump is what makes that safe: this
// runs exactly once per stored layout, so a tab the user closes afterwards is
// not resurrected on the next load.
//
// Search is NOT seeded. It searches files, notes and sessions alike, so no
// rail can host it without claiming a scope it does not have; Ctrl+Shift+F
// and the palette open it on demand.
const V6_FILES_TAB_ID = 'files-tab';

function migrateV5toV6(v5: SerializedLayout): SerializedLayout {
  const tabGroups: Record<string, SerializedTabGroup> = {};
  const present = new Set<string>();

  for (const [id, group] of Object.entries(v5.tabGroups)) {
    const tabs: SerializedTab[] = [];
    let seenSessions = false;
    for (const tab of group.tabs) {
      const next =
        (tab.contentType as string) === 'navigator'
          ? { ...tab, contentType: 'sessions' as TabContentType, title: 'Sessions' }
          : tab;
      // A layout predating the Navigator can hold a `sessions` tab AND the
      // `navigator` tab that replaced it. Rewriting both puts two session
      // lists in one strip, so the second is dropped.
      if (next.contentType === 'sessions') {
        if (seenSessions) continue;
        seenSessions = true;
      }
      present.add(next.contentType);
      tabs.push(next);
    }
    const activeTabId =
      group.activeTabId && tabs.some((t) => t.id === group.activeTabId)
        ? group.activeTabId
        : (tabs[0]?.id ?? null);
    tabGroups[id] = { id: group.id, tabs, activeTabId };
  }

  if (!present.has('files')) {
    const right = v5.edgePanels?.right;
    // First RESOLVABLE leaf, not leaf[0]. With a split right panel, leaf[0]
    // may name a group that is gone -- the same case the WS-220 chat docking
    // below already handles. Taking it blindly yields `undefined`, and the
    // optional-chained push then drops Files with no error at all.
    const rightGroupId = right
      ? edgePanelGroupIds(right).find((id) => tabGroups[id])
      : undefined;
    const group = rightGroupId ? tabGroups[rightGroupId] : undefined;
    // APPENDED, never prepended, and it never claims `activeTabId`. As
    // `tabs[0]` it becomes what the downstream prune falls back to when the
    // stored active tab is dropped; claiming an empty group's active slot
    // starves the session-docking pass below, which shows a docked session
    // only while the right panel has no active tab of its own.
    group?.tabs.push({ id: V6_FILES_TAB_ID, title: 'Files', contentType: 'files' });
  }

  return { ...v5, version: 6, tabGroups };
}

// The terminal was a full-width dock across the bottom of the window, so
// showing a shell cost the EDITOR its height — for a tool that belongs beside
// the files it runs against. It moves into the file-tree rail as a pane under
// the tree, which is also what makes it survive a flip intact: `mirrorLayout`
// reverses columns and leaves what is stacked inside them alone.
//
// Moved IN PLACE and only when the user still has one: a terminal they closed
// is not resurrected, and one they had already dragged somewhere else is left
// where they put it. The version bump is what makes that safe — this runs
// exactly once per stored layout.
const V7_TERMINAL_PANE_ID = 'right-term-pane';
const V7_TERMINAL_GROUP_ID = 'right-term-group';

function migrateV6toV7(v6: SerializedLayout): SerializedLayout {
  // A v6 layout still carries THREE docks; `EdgePanelPosition` now names two,
  // so the stored shape is read loosely here. That is the point of a
  // migration: it is the one place the old shape is still real.
  const stored = v6.edgePanels as unknown as Record<string, SerializedEdgePanel | undefined>;
  const bottom = stored?.bottom;
  const right = stored?.right;
  if (!bottom || !right?.layout) return dropBottomDock({ ...v6, version: 7 });

  const bottomGroupIds = new Set(edgePanelGroupIds(bottom));
  // Only a terminal still docked at the BOTTOM moves. One the user dragged to
  // a pane or a rail is already where they wanted it.
  const source = [...bottomGroupIds]
    .map((id) => v6.tabGroups[id])
    .find((g) => g?.tabs.some((t) => t.contentType === 'terminal'));
  if (!source) return { ...v6, version: 7 };

  const terminals = source.tabs.filter((t) => t.contentType === 'terminal');
  const tabGroups: Record<string, SerializedTabGroup> = { ...v6.tabGroups };
  const kept = source.tabs.filter((t) => t.contentType !== 'terminal');
  tabGroups[source.id] = {
    id: source.id,
    tabs: kept,
    activeTabId: kept.some((t) => t.id === source.activeTabId)
      ? source.activeTabId
      : (kept[0]?.id ?? null),
  };
  tabGroups[V7_TERMINAL_GROUP_ID] = {
    id: V7_TERMINAL_GROUP_ID,
    tabs: terminals,
    activeTabId: terminals[0]?.id ?? null,
  };

  const edgePanels = {
    ...v6.edgePanels,
    right: {
      ...right,
      layout: {
        id: 'right-split',
        type: 'split' as const,
        direction: 'vertical' as const,
        splitRatio: 0.65,
        first: right.layout,
        second: {
          id: V7_TERMINAL_PANE_ID,
          type: 'pane' as const,
          tabGroupId: V7_TERMINAL_GROUP_ID,
        },
      },
      // A tree's width plus room for a command line.
      width: Math.max(right.width ?? 0, 340),
    },
  };

  return dropBottomDock({ ...v6, version: 7, tabGroups, edgePanels } as SerializedLayout);
}

// A rail pane collapses to its tab strip on its own, which gives the terminal
// an honest default: a BAR under the file tree instead of a third of the rail
// held open for a shell nobody started. v7 stored the terminal pane expanded,
// so the flag is set once here — the same shape `defaultLayout` seeds.
//
// Every terminal-only pane in a rail, not the id `migrateV6toV7` minted: a
// user who dragged the shell into the other rail still gets the new default.
// A rail's LAST expanded pane is left alone — that invariant is what keeps a
// rail from becoming a stack of bars with nothing open.
function migrateV7toV8(v7: SerializedLayout): SerializedLayout {
  const isTerminalOnly = (groupId: string | null): boolean => {
    const group = groupId ? v7.tabGroups[groupId] : undefined;
    return !!group && group.tabs.length > 0
      && group.tabs.every((t) => t.contentType === 'terminal');
  };

  const edgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(v7.edgePanels)) {
    const panes = panelPanes(panel.layout);
    const collapsing = new Set(
      panes.filter((p) => isTerminalOnly(p.tabGroupId)).map((p) => p.id)
    );
    if (panes.every((p) => collapsing.has(p.id))) collapsing.clear();
    edgePanels[pos as EdgePanelPosition] = collapsing.size
      ? { ...panel, layout: markCollapsed(panel.layout, collapsing) }
      : panel;
  }
  return { ...v7, version: 8, edgePanels };
}

// Settings is a DIALOG, not a pane. `registerPanels` no longer registers a
// `settings` content type, so a layout saved while the settings PAGE existed
// carries a tab that nothing can render: the strip draws "Unknown content
// type", and the always-on prune below cannot collapse what it empties.
//
// The tab leaves the way tab close takes it. The group drops it, a group the
// drop empties goes, and a pane that named that group collapses out of the
// tree. A tree that would collapse to nothing keeps ONE pane with no group,
// which is the legitimate "Nothing open" state. A floating window over an
// emptied group closes with it, because an empty floating window is a shell
// the user cannot reach.
function migrateV8toV9(v8: SerializedLayout): SerializedLayout {
  const tabGroups: Record<string, SerializedTabGroup> = {};
  const emptied = new Set<string>();
  for (const [id, group] of Object.entries(v8.tabGroups)) {
    const tabs = group.tabs.filter((t) => (t.contentType as string) !== 'settings');
    if (tabs.length === group.tabs.length) {
      tabGroups[id] = group;
    } else if (tabs.length === 0) {
      emptied.add(id);
    } else {
      tabGroups[id] = {
        id: group.id,
        tabs,
        activeTabId: tabs.some((t) => t.id === group.activeTabId)
          ? group.activeTabId
          : (tabs[0]?.id ?? null),
      };
    }
  }
  if (emptied.size === 0) return { ...v8, version: 9, tabGroups };

  const layout = dropPanesForGroups(v8.layout, emptied);
  // A partial payload reaches this migration whole: a layout STORED at v8 skips
  // every migration above, so nothing here may assume a complete shape. The
  // deserializer below rebuilds an absent panel.
  const edgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(v8.edgePanels ?? {})) {
    edgePanels[pos as EdgePanelPosition] = panel.layout
      ? { ...panel, layout: dropPanesForGroups(panel.layout, emptied) }
      : panel;
  }

  return {
    ...v8,
    version: 9,
    layout,
    tabGroups,
    edgePanels,
    floatingWindows: (v8.floatingWindows ?? []).filter(
      (w) => !emptied.has((w as { tabGroupId: string }).tabGroupId),
    ),
  };
}

/**
 * Drop every pane that names one of `groups`, and collapse the splits that
 * lose a child.
 *
 * The root always survives: a tree with no pane is a shell with nothing to
 * render, so a root that loses everything comes back as one pane with no tab
 * group. That is the same shape `collapseEmptyNodes` leaves behind.
 */
function dropPanesForGroups(node: LayoutNode, groups: Set<string>): LayoutNode {
  const drop = (n: LayoutNode): LayoutNode | null => {
    if (n.type === 'pane') {
      return n.tabGroupId && groups.has(n.tabGroupId) ? null : n;
    }
    const first = drop(n.first);
    const second = drop(n.second);
    if (first && second) return { ...n, first, second };
    return first ?? second;
  };
  const pruned = drop(node);
  if (pruned) return pruned;
  const root = firstPaneOf(node);
  return { id: root?.id ?? node.id, type: 'pane', tabGroupId: null };
}

/** The first leaf pane of a serialized tree, for its id. */
function firstPaneOf(node: LayoutNode): PaneNode | undefined {
  if (node.type === 'pane') return node;
  return firstPaneOf(node.first) ?? firstPaneOf(node.second);
}

/** Leaf panes of a serialized panel tree, in order. */
function panelPanes(node: LayoutNode | undefined): PaneNode[] {
  if (!node) return [];
  if (node.type === 'pane') return [node];
  return [...panelPanes(node.first), ...panelPanes(node.second)];
}

function markCollapsed(node: LayoutNode, ids: Set<string>): LayoutNode {
  if (node.type === 'pane') {
    return ids.has(node.id) ? { ...node, collapsed: true } : node;
  }
  return {
    ...node,
    first: markCollapsed(node.first, ids),
    second: markCollapsed(node.second, ids),
  };
}

/**
 * Delete the bottom dock, rehoming anything still in it.
 *
 * The dock is gone as a concept — `EdgePanelPosition` names two sides — so a
 * stored layout that still has one would otherwise keep a panel nothing can
 * reach, render or close. Surviving tabs go to the CENTRE rather than being
 * dropped: they are the user's, and the centre is the one region that always
 * exists.
 */
function dropBottomDock(layout: SerializedLayout): SerializedLayout {
  const stored = layout.edgePanels as unknown as Record<string, SerializedEdgePanel | undefined>;
  const bottom = stored?.bottom;
  if (!bottom) return layout;

  const { bottom: _dropped, ...sides } = stored;
  const orphans = edgePanelGroupIds(bottom).flatMap((id) => layout.tabGroups[id]?.tabs ?? []);

  const tabGroups = { ...layout.tabGroups };
  for (const id of edgePanelGroupIds(bottom)) delete tabGroups[id];

  if (orphans.length) {
    const centreId = centreGroupIds(layout.layout).find((id) => tabGroups[id]);
    const centre = centreId ? tabGroups[centreId] : undefined;
    if (centre) {
      tabGroups[centre.id] = {
        ...centre,
        tabs: [...centre.tabs, ...orphans],
        activeTabId: centre.activeTabId ?? orphans[0]?.id ?? null,
      };
    }
  }

  return {
    ...layout,
    tabGroups,
    edgePanels: sides as unknown as SerializedLayout['edgePanels'],
  };
}

/** Leaf group ids of the centre layout tree. */
function centreGroupIds(node: LayoutNode): string[] {
  if (node.type === 'pane') return node.tabGroupId ? [node.tabGroupId] : [];
  return [...centreGroupIds(node.first), ...centreGroupIds(node.second)];
}

function migrateV1toV2(v1: any): SerializedLayout {
  const newTabGroups = { ...v1.tabGroups };

  // Migrate each edge panel
  for (const pos of ['left', 'right'] as const) {
    const panel = v1.edgePanels[pos];
    // Absent positions synthesize defaults at the end of deserializeLayout.
    if (!panel) continue;
    if (panel.tabs && Array.isArray(panel.tabs)) {
      // Create new tab group from v1 inline tabs
      const groupId = `edge-${pos}-${Date.now()}`;
      const tabs = panel.tabs.map((tab: any) => {
        const { panelPosition: _panelPosition, ...rest } = tab;
        void _panelPosition;
        return rest;
      });

      newTabGroups[groupId] = {
        id: groupId,
        tabs,
        activeTabId: panel.activeTabId ?? (tabs.length > 0 ? tabs[0].id : null),
      };

      // Replace inline tabs with tabGroupId reference
      v1.edgePanels[pos] = {
        id: panel.id,
        tabGroupId: groupId,
        isCollapsed: panel.isCollapsed,
        width: panel.width,
        height: panel.height,
      };
    }
  }

  return {
    ...v1,
    version: 2,
    tabGroups: newTabGroups,
  };
}

/**
 * Bring a stored layout from v1..v8 up to v9. The core reader calls this only
 * for a version below 9.
 *
 * v1 → v2 (edge-panel tab groups) → v3 (prune tabs whose content type is no
 * longer registered) → v4 (chat-worthy right panel width) → v5 (edge panels
 * carry layout trees) → v6 (the Navigator splits into Sessions / Search /
 * Files) → v7 (the bottom dock goes; the terminal moves into the file rail)
 * → v8 (rail panes collapse on their own; the terminal ships collapsed) → v9
 * (the settings PAGE is gone; its tabs go, and the panes they empty go with
 * them).
 */
function upgradeLegacy(json: unknown): SerializedLayoutV9<TabContentType> {
  let layout = json as SerializedLayout;
  if (layout.version === 1) {
    layout = migrateV1toV2(layout as any);
  }
  if (layout.version === 2) {
    layout = migrateV2toV3(layout);
  }
  if (layout.version === 3) {
    layout = migrateV3toV4(layout);
  }
  if (layout.version === 4) {
    layout = migrateV4toV5(layout);
  }
  if (layout.version === 5) {
    layout = migrateV5toV6(layout);
  }
  if (layout.version === 6) {
    layout = migrateV6toV7(layout);
  }
  if (layout.version === 7) {
    layout = migrateV7toV8(layout);
  }
  if (layout.version === 8) {
    layout = migrateV8toV9(layout);
  }
  // The core reader checks the version that comes back.
  return layout as SerializedLayoutV9<TabContentType>;
}

/**
 * The passes that run on every restore, at every version. They mutate the
 * restored state; the core gives the icons after them.
 */
function pruneRestored(restored: RestoredLayout<TabContentType>): void {
  // Always-on prune of unregistered content types: the v2→v3 migration only
  // covers layouts that were still v2 — a v4 layout persisted before a panel
  // was deleted (e.g. the removed Home page) would otherwise resurrect a
  // ghost tab that renders "Unknown content type" forever.
  const registry = getGlobalRegistry();
  const isKnown = (contentType: string) =>
    registry.list().length === 0 || registry.get(contentType) !== undefined;

  const tabGroups: Record<string, TabGroup> = {};
  for (const [id, group] of Object.entries(restored.tabGroups)) {
    // Always-on prune: a `chat` tab WITHOUT a sessionId is the legacy
    // generic Chat panel from before sessions opened as session-bound tabs
    // in the right pane (WS-220). Restoring it makes the active session
    // render wherever that tab happens to be docked — sessions then never
    // appear in the right pane. Session tabs (metadata.sessionId) survive.
    const tabs = group.tabs.filter(
      (t) => isKnown(t.contentType) && !(t.contentType === 'chat' && !t.metadata?.sessionId),
    );
    tabGroups[id] = {
      id: group.id,
      tabs,
      // Remap ONLY when the active tab was pruned — a stored null must stay
      // null (round-trip identity; the property tests enforce it).
      activeTabId:
        group.activeTabId && !tabs.some((t) => t.id === group.activeTabId)
          ? (tabs[0]?.id ?? null)
          : group.activeTabId,
    };
  }

  // Chat tabs dock in the right edge panel (WS-220). Layouts persisted in
  // the center-split era hold session chat tabs in center-tiling groups —
  // restoring them boots with a stale second chat surface next to the
  // editor. Migrate them into the right panel group and collapse any pane
  // this empties out of the tree.
  let layoutTree = restored.layout;
  // First RESOLVABLE leaf group of the right panel: with a split right
  // panel, leaf [0] may be empty/missing from tabGroups, and skipping the
  // chat migration would resurrect the stale center chat surface WS-220
  // exists to prevent.
  const rightGroupId = restored.edgePanels.right
    ? edgePanelGroupIds(restored.edgePanels.right).find((id) => tabGroups[id])
    : undefined;
  if (rightGroupId && tabGroups[rightGroupId]) {
    const centerGroupIds = new Set<string>();
    const collect = (n: LayoutNode): void => {
      if (n.type === 'pane') {
        if (n.tabGroupId) centerGroupIds.add(n.tabGroupId);
      } else {
        collect(n.first);
        collect(n.second);
      }
    };
    collect(layoutTree);

    const right = tabGroups[rightGroupId];
    const emptied = new Set<string>();
    for (const gid of centerGroupIds) {
      if (gid === rightGroupId) continue;
      const g = tabGroups[gid];
      if (!g) continue;
      const moving = g.tabs.filter((t) => t.contentType === 'chat');
      if (moving.length === 0) continue;
      g.tabs = g.tabs.filter((t) => t.contentType !== 'chat');
      if (g.activeTabId && !g.tabs.some((t) => t.id === g.activeTabId)) {
        g.activeTabId = g.tabs[0]?.id ?? null;
      }
      right.tabs = [...right.tabs, ...moving];
      if (!right.activeTabId) right.activeTabId = moving[0].id;
      if (g.tabs.length === 0) emptied.add(gid);
    }

    if (emptied.size > 0) {
      const collapse = (n: LayoutNode): LayoutNode | null => {
        if (n.type === 'pane') {
          return n.tabGroupId && emptied.has(n.tabGroupId) ? null : n;
        }
        const first = collapse(n.first);
        const second = collapse(n.second);
        if (first && second) return { ...n, first, second };
        return first ?? second;
      };
      // A root pane that emptied stays (renders the EmptyState) — only
      // split branches collapse away.
      layoutTree = collapse(layoutTree) ?? layoutTree;
      // Groups the collapse orphaned entirely can go.
      const stillReferenced = new Set<string>();
      const collectInto = (n: LayoutNode): void => {
        if (n.type === 'pane') {
          if (n.tabGroupId) stillReferenced.add(n.tabGroupId);
        } else {
          collectInto(n.first);
          collectInto(n.second);
        }
      };
      collectInto(layoutTree);
      for (const panel of Object.values(restored.edgePanels)) {
        for (const id of edgePanelGroupIds(panel)) stillReferenced.add(id);
      }
      for (const w of restored.floatingWindows) stillReferenced.add((w as { tabGroupId: string }).tabGroupId);
      for (const gid of emptied) {
        if (!stillReferenced.has(gid)) delete tabGroups[gid];
      }
    }
  }

  restored.layout = layoutTree;
  restored.tabGroups = tabGroups;
}

/** The app hooks of the core layout reader. */
export const appLayoutHooks: LayoutCodecHooks<TabContentType> = {
  upgradeLegacy,
  prune: pruneRestored,
  iconFor: iconForContentType,
};
