import type { Component } from 'solid-js';
import type { TabContentType } from '@/types/windowTypes';
import {
  Activity,
  ChartNetwork,
  ClipboardList,
  FileDiff,
  FileText,
  FolderTree,
  Inbox,
  Layers,
  Link2,
  MessageCircle,
  Package,
  Plus,
  Puzzle,
  Search,
  Settings,
  Target,
  Terminal,
  Wrench,
} from '@/lib/icons';

/**
 * Canonical tab icon per content type. Tab.icon is a component and cannot be
 * serialized — persisted layouts strip it, so restore paths (and any code
 * creating tabs) resolve icons here instead of carrying them in state.
 */
const TAB_ICONS: Partial<Record<TabContentType, Component<{ class?: string }>>> = {
  sessions: ClipboardList,
  backlinks: Link2,
  graph: ChartNetwork,
  canvas: ChartNetwork,
  files: FolderTree,
  search: Search,
  activity: Activity,
  terminal: Terminal,
  chat: MessageCircle,
  'chat-draft': Plus,
  inbox: Inbox,
  file: FileText,
  settings: Settings,
  plugins: Package,
  skills: Target,
  surfaces: Layers,
  tool: Wrench,
};

/**
 * Panels the registry holds that are not tab content types of their own.
 *
 * The registry keys on a plain string, so these never reached `TAB_ICONS` and
 * both the desktop tab bar and the phone's overflow menu drew them bare.
 */
const PANEL_ICONS: Record<string, Component<{ class?: string }>> = {
  changes: FileDiff,
  'plugin-blocks': Puzzle,
};

export function iconForContentType(
  contentType: TabContentType
): Component<{ class?: string }> | undefined {
  return TAB_ICONS[contentType];
}

/** The mark for anything the registry holds, by its registered id. */
export function iconForPanelId(id: string): Component<{ class?: string }> | undefined {
  return PANEL_ICONS[id] ?? TAB_ICONS[id as TabContentType];
}
