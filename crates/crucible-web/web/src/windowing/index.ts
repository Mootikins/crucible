/**
 * The public entry of the windowing core. App code imports the core from
 * `@/windowing` and from no deeper path. The boundary test in
 * `__tests__/boundary.test.ts` enforces that rule.
 *
 * This module holds re-exports only. Each name here has an app caller.
 * Tests, the harness pages and the core itself may still import a deep path.
 */

// The store, its policy seam and the actions.
export {
  configureWindowing,
  findEdgePanelForGroup,
  setStore,
  windowActions,
  windowStore,
} from './store';
export type { WindowActions, WindowPolicy } from './store';

// The mounted window manager and its app slots.
export { WindowManager } from './components/WindowManager';
export { DROP_OVER_ATTR } from './components/context';
export type { WindowingSlots } from './components/context';

// The rail button look and the command button. These are public on purpose:
// the app draws its own rail buttons, and they must match the core buttons.
export { RibbonCommand, ribbonBtn } from './components/RibbonButton';

// The layout model: its types and the tree queries that the app uses.
export type {
  EdgeCue,
  EdgeMode,
  EdgePanel,
  EdgePanelPosition,
  LayoutNode,
  PaneNode,
  Tab,
  TabGroup,
  WindowState,
} from './model/types';
export { isEdgeCollapsed } from './model/types';
export {
  collectLeafGroupIds,
  collectPanes,
  findFirstPane,
  generateId,
  primaryEdgeGroupId,
} from './model/tree';

// The stored layout format and the reader hooks.
export type {
  LayoutCodecHooks,
  RestoredLayout,
  SerializedEdgePanelV9,
  SerializedLayout,
  SerializedLayoutV9,
  SerializedTab,
  SerializedTabGroup,
  StoredLayout,
} from './model/serializer';
export { isLegacyV9 } from './model/serializer';

// The keyboard chords and the native context menu rule.
export { chordLabel, LAYOUT_SHORTCUTS, matchShortcut } from './shortcuts';
export type { ShortcutAction } from './shortcuts';
export { attachNativeMenuGuard, shouldUseNativeMenu } from './context-menu';
