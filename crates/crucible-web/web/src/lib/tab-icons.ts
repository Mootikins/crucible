import type { Component } from 'solid-js';
import type { TabContentType } from '@/types/windowTypes';
import {
  Activity,
  ChartNetwork,
  ClipboardList,
  FileText,
  FolderTree,
  Inbox,
  Link2,
  MessageCircle,
  Package,
  Plus,
  Settings,
  Target,
  Terminal,
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
  files: FolderTree,
  activity: Activity,
  terminal: Terminal,
  chat: MessageCircle,
  'chat-draft': Plus,
  inbox: Inbox,
  file: FileText,
  settings: Settings,
  plugins: Package,
  skills: Target,
};

export function iconForContentType(
  contentType: TabContentType
): Component<{ class?: string }> | undefined {
  return TAB_ICONS[contentType];
}
