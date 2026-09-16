import type { Component } from 'solid-js';

export interface Tab<C extends string = string> {
  id: string;
  title: string;
  icon?: Component<{ class?: string }>;
  contentType: C;
  isModified?: boolean;
  isPinned?: boolean;
  metadata?: Record<string, unknown>;
}

export interface TabGroup<C extends string = string> {
  id: string;
  tabs: Tab<C>[];
  activeTabId: string | null;
}

// Pane types
export type SplitDirection = 'horizontal' | 'vertical';

export interface PaneNode {
  id: string;
  type: 'pane';
  tabGroupId: string | null;
  /**
   * Collapsed to its tab strip, independently of the rail's own collapse.
   *
   * The flag lives on the NODE, not in a set of pane ids on the EdgePanel,
   * because the node is the one thing every layout transform already carries.
   * `mirrorLayout` returns leaves untouched, so a flip keeps it for free;
   * `insertPaneRelative`, `collapseEmptyNodes` and `updateRootWhere` rebuild
   * trees around leaves; and `serializeLayout` deep-clones the tree, so it
   * persists with no new field. A side set would need its own mirror rule, its
   * own serialized slot, and a sweep for ids no pane owns any more.
   *
   * Optional, not required: a v7 layout deserializes unchanged, and an absent
   * flag means expanded.
   */
  collapsed?: boolean;
  /** How the collapsed pane comes back. Default `click`. */
  reveal?: PaneReveal;
}

interface SplitNode {
  id: string;
  type: 'split';
  direction: SplitDirection;
  first: LayoutNode;
  second: LayoutNode;
  splitRatio: number;
}

export type LayoutNode = PaneNode | SplitNode;

// Edge panel types
/**
 * The two docks. There was a third, `bottom`, a full-width dock. A panel in
 * it cost the CENTRE its height, for a tool that belonged beside the rail
 * content it works with. That panel moved into a rail as a pane under the
 * rail's first pane; nothing else ever lived down there.
 *
 * Two values, not three-with-one-unused: a dock nothing can reach is a drop
 * target, a ribbon, a toggle, a shortcut and a serialized slot that all still
 * have to be maintained.
 */
export type EdgePanelPosition = 'left' | 'right';

/**
 * How an edge host presents itself.
 *
 * `docked`: the body sits in the flow. `strip`: the ribbon shows, the body
 * does not. `flyout`: the body floats over the centre until its button is
 * clicked again. `hidden`: neither shows; a hot zone on the screen edge
 * reveals the host as a flyout.
 */
export type EdgeMode = 'docked' | 'strip' | 'flyout' | 'hidden';

/**
 * Every `EdgeMode`, for a check of stored data. `satisfies` refuses a value
 * outside the union; `types.test.ts` refuses a union member outside the table.
 */
export const EDGE_MODES = ['docked', 'strip', 'flyout', 'hidden'] as const satisfies readonly EdgeMode[];

/** The affordance a hidden host leaves on the screen edge. */
export type EdgeCue = 'grip' | 'none';

/** Every `EdgeCue`, with the same two checks as `EDGE_MODES`. */
export const EDGE_CUES = ['grip', 'none'] as const satisfies readonly EdgeCue[];

/** How a pane collapsed to its band comes back. */
export type PaneReveal = 'click' | 'hover';

export type FocusedRegion = EdgePanelPosition | 'center';

export interface EdgePanel {
  id: string;
  /** Panel content is a full binary layout tree (same shape as the center
   * tiling) — leaves are PaneNodes referencing tab groups, so edge panels
   * split exactly like the center area. */
  layout: LayoutNode;
  mode: EdgeMode;
  /** Only read when `mode` is `hidden`. Default `grip`. */
  cue?: EdgeCue;
  width?: number;
  height?: number;
}

// Floating window types
export interface FloatingWindow {
  id: string;
  tabGroupId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  isMinimized: boolean;
  isMaximized: boolean;
  zIndex: number;
  title?: string;
  /** Hover-spawned popover: auto-closes on hover-away and is excluded from
   * layout persistence. Pinning (or dragging/resizing — Hover Editor's
   * auto-pin) clears it, promoting the popover to a normal window. */
  transient?: boolean;
  /** false hides the tab bar (compact hover-editor look); the titlebar
   * toggle brings it back for native tab drag-and-drop. */
  showTabBar?: boolean;
  /** Bounds to restore when un-maximizing. */
  restoreBounds?: { x: number; y: number; width: number; height: number };
}

// Drag and drop types
// 'tab' moves an existing tab between groups; 'newTab' spawns a tab that has
// no source group yet (e.g. dragging an item out of a hover card) —
// every drop target treats both alike, so any surface can join the window
// system by carrying a Tab payload.
export type DragSource<C extends string = string> =
  | { type: 'tab'; tab: Tab<C>; sourceGroupId: string }
  | { type: 'newTab'; tab: Tab<C> };

export type DropTarget =
  | {
      type: 'pane';
      paneId: string;
      position?: 'center' | 'left' | 'right' | 'top' | 'bottom';
    }
  | { type: 'tabGroup'; groupId: string; insertIndex?: number }
  | { type: 'edgePanel'; panelId: EdgePanelPosition; insertIndex?: number }
  // `at` = viewport point to spawn the window at (hover-card tear-off).
  | { type: 'newFloating'; at?: { x: number; y: number } };

// ---------------------------------------------------------------------------
// Native drags from outside the window manager do NOT use this solid-dnd
// pipeline. The app attaches them through the `attachDropTarget` slot, with
// native HTML5 drag events. A second solid-dnd provider was rejected:
// solid-dnd only matches within its nearest provider, which blocks a drag
// that crosses from outside the window manager into a pane. The two systems
// coexist because solid-dnd is pointer-event based and a native drag uses
// dragstart/drop.
// ---------------------------------------------------------------------------

// One TabBar for every region: edge panels host the same Pane/TabBar stack
// as the center tiling (the bar derives its region from the group).
export interface TabBarProps {
  groupId: string;
  paneId: string;
  onPopOut?: () => void;
}

/** True when the body is out of the flow: strip, flyout or hidden. */
export function isEdgeCollapsed(panel: Pick<EdgePanel, 'mode'>): boolean {
  return panel.mode !== 'docked';
}

export interface WindowState<C extends string = string> {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup<C>>;
  edgePanels: Record<EdgePanelPosition, EdgePanel>;
  floatingWindows: FloatingWindow[];
  activePaneId: string | null;
  focusedRegion: FocusedRegion;
  nextZIndex: number;
}

export type PaneDropPosition = 'left' | 'right' | 'top' | 'bottom';
