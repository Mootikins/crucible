/**
 * Model F on the real windowing core: the seed and the policy.
 *
 * Left rail: sessions over files. Centre: note tabs. Right rail: a tab for
 * each open session, with the terminal folded into the rail's corner. The session covers the
 * centre with the core's expand (Shift+Esc, or the button in its header).
 */
import type { Component } from 'solid-js';
import { ClipboardList, FileText, FolderTree, GitCompare, MessageSquare, Terminal } from 'lucide-solid';
import type { WindowPolicy } from '@/windowing/store/policy';
import type { WindowState } from '@/windowing/model/types';
import { emptyState, generateId } from '@/windowing/model/tree';
import { LAYOUT_SHORTCUTS } from '@/windowing/shortcuts';
import { state } from './state';

export type MockType = 'sessions' | 'files' | 'note' | 'changes' | 'session' | 'terminal';

export const ICONS: Record<MockType, Component<{ class?: string }>> = {
  sessions: ClipboardList,
  files: FolderTree,
  note: FileText,
  changes: GitCompare,
  session: MessageSquare,
  terminal: Terminal,
};

/** The tab of one session. Each open session has its own. */
export const sessionTab = (sid: string) => ({
  id: `session:${sid}`,
  title: state.sessions[sid]?.title ?? 'Session',
  contentType: 'session' as const,
  icon: ICONS.session,
  metadata: { sid },
});

const note = (path: string) => ({
  id: `note:${path}`,
  title: path.split('/').pop()!,
  contentType: 'note' as const,
  icon: ICONS.note,
  metadata: { path },
});

function seed(): WindowState<MockType> {
  const s = emptyState<MockType>();
  const group = (tabs: WindowState<MockType>['tabGroups'][string]['tabs'], active = 0) => {
    const id = generateId();
    s.tabGroups[id] = { id, tabs, activeTabId: tabs[active]?.id ?? null };
    return id;
  };
  // The empty state's own groups go; every group here is named below.
  s.tabGroups = {};
  const centre = group([note('Index'), note('Help/Concepts/Kilns'), note('Help/Concepts/Precognition')], 2);
  const sessions = group([{ id: 'sessions', title: 'Sessions', contentType: 'sessions', icon: ICONS.sessions }]);
  const files = group([{ id: 'files', title: 'Files', contentType: 'files', icon: ICONS.files }]);
  const session = group([sessionTab(state.active)]);
  const terminal = group([{ id: 'terminal', title: 'Terminal', contentType: 'terminal', icon: ICONS.terminal }]);
  const centrePane = generateId();
  s.layout = { id: centrePane, type: 'pane', tabGroupId: centre };
  s.activePaneId = centrePane;
  s.edgePanels = {
    left: {
      id: 'left-panel',
      layout: {
        id: 'left-split',
        type: 'split',
        direction: 'vertical',
        splitRatio: 0.4,
        first: { id: 'left-sessions', type: 'pane', tabGroupId: sessions },
        second: { id: 'left-files', type: 'pane', tabGroupId: files },
      },
      mode: 'docked',
      width: 272,
    },
    right: {
      id: 'right-panel',
      layout: {
        id: 'right-split',
        type: 'split',
        direction: 'vertical',
        splitRatio: 0.72,
        first: { id: 'right-session', type: 'pane', tabGroupId: session },
        // The terminal keeps its corner: folded under the session until opened.
        second: { id: 'right-terminal', type: 'pane', tabGroupId: terminal, collapsed: true },
      },
      mode: 'docked',
      width: 400,
    },
  };
  return s;
}

/** Common chords, rebindable through this table: Ctrl/Cmd+L focuses the composer (Cursor, opencode). */
const APP_SHORTCUTS = [
  { key: 'l', modifiers: ['ctrl'] as const, action: 'focusComposer', description: 'Focus the composer' },
];

export function mockPolicy(onShortcut: (action: string, e: KeyboardEvent) => boolean): WindowPolicy<MockType> {
  return {
    seed,
    // The left rail's identity panes stay, as in the app (WS-324). A session tab closes.
    mayCloseTab: (_s, _g, tabId) => !['sessions', 'files'].includes(tabId),
    repairLayout: () => {},
    onActiveTabChange: () => {},
    iconFor: (type) => ICONS[type],
    unavailableReason: () => null,
    layoutHooks: {
      upgradeLegacy: () => {
        throw new Error('shell mockup: no legacy layouts');
      },
      prune: () => {},
    },
    shortcuts: [...LAYOUT_SHORTCUTS, ...APP_SHORTCUTS.map((s) => ({ ...s, modifiers: [...s.modifiers] }))],
    onShortcut,
  };
}
