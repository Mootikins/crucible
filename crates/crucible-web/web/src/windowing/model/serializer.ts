import type { Component } from 'solid-js';
import { EDGE_CUES, EDGE_MODES } from './types';
import type {
  EdgeCue,
  EdgeMode,
  EdgePanel,
  EdgePanelPosition,
  FloatingWindow,
  LayoutNode,
  Tab,
  TabGroup,
} from './types';

/**
 * The saved layout format.
 *
 * The core owns the CURRENT format and the one step into it from v9. The
 * history before v9 belongs to the app, because each of those steps names
 * content types that the app had at the time. The app gives that history to
 * the reader as `upgradeLegacy`.
 */
const LAYOUT_VERSION = 10;

export interface SerializedTab<C extends string = string> {
  id: string;
  title: string;
  contentType: C;
  isModified?: boolean;
  isPinned?: boolean;
  metadata?: Record<string, unknown>;
}

export interface SerializedTabGroup<C extends string = string> {
  id: string;
  tabs: SerializedTab<C>[];
  activeTabId: string | null;
}

interface SerializedEdgePanel {
  id: string;
  layout: LayoutNode;
  mode: EdgeMode;
  /** Absent means `grip`. */
  cue?: EdgeCue;
  width?: number;
  height?: number;
}

export interface SerializedLayout<C extends string = string> {
  version: typeof LAYOUT_VERSION;
  layout: LayoutNode;
  tabGroups: Record<string, SerializedTabGroup<C>>;
  edgePanels: Record<EdgePanelPosition, SerializedEdgePanel>;
  floatingWindows: FloatingWindow[];
}

/** A v9 rail: open or collapsed, and no mode. */
export interface SerializedEdgePanelV9 {
  id: string;
  layout: LayoutNode;
  isCollapsed?: boolean;
  width?: number;
  height?: number;
}

/** The format before v10. The app history ends here. */
export interface SerializedLayoutV9<C extends string = string> {
  version: 9;
  layout: LayoutNode;
  tabGroups: Record<string, SerializedTabGroup<C>>;
  edgePanels: Record<EdgePanelPosition, SerializedEdgePanelV9>;
  floatingWindows: FloatingWindow[];
}

/** Any payload the reader accepts. Only the version is certain. */
export type StoredLayout<C extends string = string> =
  | SerializedLayout<C>
  | SerializedLayoutV9<C>
  | { version: number };

/** The part of a window state that a saved layout holds. */
export interface RestoredLayout<C extends string = string> {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup<C>>;
  edgePanels: Record<EdgePanelPosition, EdgePanel>;
  floatingWindows: FloatingWindow[];
}

/** What the app supplies to read a stored layout. Every member is required. */
export interface LayoutCodecHooks<C extends string = string> {
  /** Bring a payload older than v9 up to v9. The core calls it only when version < 9. */
  upgradeLegacy(json: unknown): SerializedLayoutV9<C>;
  /** Drop or move tabs the app no longer supports. Mutates the restored state. */
  prune(state: RestoredLayout<C>): void;
  /** The icon of a restored tab. An icon is a component, so no layout stores it. */
  iconFor(contentType: C): Component<{ class?: string }> | undefined;
}

const EDGE_POSITIONS: readonly EdgePanelPosition[] = ['left', 'right'];

function stripIcon<C extends string>(tab: Tab<C>): SerializedTab<C> {
  const { icon: _icon, ...rest } = tab;
  void _icon;
  return rest;
}

const cloneTree = (node: LayoutNode): LayoutNode =>
  JSON.parse(JSON.stringify(node)) as LayoutNode;

export function serializeLayout<C extends string>(state: RestoredLayout<C>): SerializedLayout<C> {
  const tabGroups: Record<string, SerializedTabGroup<C>> = {};
  for (const [id, group] of Object.entries(state.tabGroups)) {
    tabGroups[id] = {
      id: group.id,
      tabs: group.tabs.map(stripIcon),
      activeTabId: group.activeTabId,
    };
  }

  const edgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(state.edgePanels)) {
    edgePanels[pos as EdgePanelPosition] = {
      id: panel.id,
      layout: cloneTree(panel.layout),
      mode: panel.mode,
      cue: panel.cue,
      width: panel.width,
      height: panel.height,
    };
  }

  return {
    version: LAYOUT_VERSION,
    layout: cloneTree(state.layout),
    tabGroups,
    edgePanels,
    floatingWindows: state.floatingWindows.map((w) => ({ ...w })),
  };
}

// A rail had two states, open or collapsed, stored as `isCollapsed`. It now
// has four modes, stored as `mode`. A collapsed rail becomes a strip, and any
// other rail stays docked. The legacy field goes, so that one field is the
// only source of the mode.
function migrateV9toV10<C extends string>(v9: SerializedLayoutV9<C>): SerializedLayout<C> {
  // A partial payload reaches this step too, so an absent rail stays absent.
  // The rebuild below gives it a panel.
  const edgePanels = {} as Record<EdgePanelPosition, SerializedEdgePanel>;
  for (const [pos, panel] of Object.entries(v9.edgePanels ?? {})) {
    const { isCollapsed, ...rest } = panel;
    edgePanels[pos as EdgePanelPosition] = {
      ...rest,
      mode: isCollapsed ? 'strip' : 'docked',
    };
  }
  return { ...v9, version: LAYOUT_VERSION, edgePanels };
}

/** A stored layout at v9, the last version before edge modes. */
export function isLegacyV9<C extends string>(s: { version: number }): s is SerializedLayoutV9<C> {
  return s.version === 9;
}

/** A stored layout in the current format. */
function isCurrentLayout<C extends string>(s: { version: number }): s is SerializedLayout<C> {
  return s.version === LAYOUT_VERSION;
}

const isEdgeMode = (value: unknown): value is EdgeMode => EDGE_MODES.some((m) => m === value);
const isEdgeCue = (value: unknown): value is EdgeCue => EDGE_CUES.some((c) => c === value);

function isSupported(version: number): boolean {
  return Number.isInteger(version) && version >= 1 && version <= LAYOUT_VERSION;
}

function unsupported(version: unknown): Error {
  return new Error(`Unsupported layout version: ${String(version)}`);
}

/**
 * Read a stored layout into window state.
 *
 * The order is fixed: the app history up to v9, the step to v10, the rebuild
 * of absent rails, the app prune, and last the icons. The prune sees every
 * rail, and the icons go only on the tabs that the prune keeps.
 */
export function deserializeLayout<C extends string>(
  json: StoredLayout<C>,
  hooks: LayoutCodecHooks<C>,
): RestoredLayout<C> {
  if (!isSupported(json.version)) throw unsupported(json.version);

  let stored: StoredLayout<C> = json;
  if (stored.version < 9) {
    // Typed wide on purpose: the hook promises v9, and this check does not trust it.
    const upgraded: { version: number } = hooks.upgradeLegacy(stored);
    if (!isLegacyV9<C>(upgraded)) {
      throw new Error(
        `Legacy layout upgrade from v${json.version} returned v${upgraded.version}, not v9`,
      );
    }
    stored = upgraded;
  }
  let current: SerializedLayout<C>;
  if (isLegacyV9<C>(stored)) current = migrateV9toV10(stored);
  else if (isCurrentLayout<C>(stored)) current = stored;
  else throw unsupported(stored.version);

  const tabGroups: Record<string, TabGroup<C>> = {};
  for (const [id, group] of Object.entries(current.tabGroups)) {
    tabGroups[id] = {
      id: group.id,
      tabs: group.tabs.map((t) => ({ ...t })),
      activeTabId: group.activeTabId,
    };
  }

  // Every position MUST come back with a valid panel: a partial payload
  // (missing edgePanels entries, or an empty object from a truncated write)
  // otherwise leaves `edgePanels[pos]` undefined, and every read of a rail
  // crashes the whole window manager. An absent position
  // gets an empty panel in the strip mode.
  const edgePanels = {} as Record<EdgePanelPosition, EdgePanel>;
  for (const pos of EDGE_POSITIONS) {
    const panel = current.edgePanels?.[pos];
    if (!panel) {
      edgePanels[pos] = {
        id: `${pos}-panel`,
        layout: { id: `${pos}-pane`, type: 'pane', tabGroupId: null },
        mode: 'strip',
        width: 280,
      };
      continue;
    }
    edgePanels[pos] = {
      id: panel.id,
      // A payload with a null or absent tree (hand-corrupted JSON) must not
      // block the renderer. It degrades to an empty pane.
      layout: panel.layout ?? { id: `${panel.id}-pane`, type: 'pane', tabGroupId: null },
      // A hand-edited payload can lack the mode or hold one this build does
      // not know. Docked is the safe reading, the same one the migration
      // gives an open rail. An unknown cue goes, so the default applies.
      mode: isEdgeMode(panel.mode) ? panel.mode : 'docked',
      ...(isEdgeCue(panel.cue) ? { cue: panel.cue } : {}),
      width: panel.width,
      height: panel.height,
    };
  }

  const restored: RestoredLayout<C> = {
    layout: current.layout,
    tabGroups,
    edgePanels,
    floatingWindows: current.floatingWindows.map((w) => ({ ...w })),
  };

  hooks.prune(restored);

  for (const group of Object.values(restored.tabGroups)) {
    group.tabs = group.tabs.map((t) => ({ ...t, icon: hooks.iconFor(t.contentType) }));
  }
  return restored;
}
