import type * as Core from '@/windowing/model/types';

// Tab types — every entry is a REGISTERED panel (register-panels.tsx),
// except the legacy 'settings' below.
export type TabContentType =
  | 'file'
  | 'terminal'
  // LEGACY. Settings is a dialog, and `registerPanels` registers no panel for
  // it, so a tab of this type renders nothing. The name survives for the
  // layouts that still carry one; `migrateV8toV9` drops them on restore.
  | 'settings'
  | 'chat'
  | 'chat-draft'
  | 'inbox'
  | 'sessions'
  | 'files'
  | 'search'
  | 'skills'
  | 'plugins'
  | 'activity'
  | 'backlinks'
  | 'graph'
  | 'canvas'
  | 'surfaces';

export type {
  EdgeCue,
  EdgeMode,
  EdgePanel,
  EdgePanelPosition,
  LayoutNode,
  PaneNode,
} from '@/windowing/model/types';
export { isEdgeCollapsed } from '@/windowing/model/types';

export type Tab = Core.Tab<TabContentType>;
export type TabGroup = Core.TabGroup<TabContentType>;
export type WindowState = Core.WindowState<TabContentType>;
