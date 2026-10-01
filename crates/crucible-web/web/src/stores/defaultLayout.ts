import {
  ClipboardList,
  FolderTree,
  Terminal,
} from '@/lib/icons';
import type { Tab, WindowState } from '@/types/windowTypes';
import { generateId } from '@/windowing';

const createSampleTabs = (): Tab[] => [];

// Navigation stays beside the document; conversations have their own rail.
// Search spans files, notes and sessions and opens on demand.
const createSessionTabs = (): Tab[] => [
  {
    id: 'sessions-tab',
    title: 'Sessions',
    contentType: 'sessions',
    icon: ClipboardList,
  },
];

const createFileTabs = (): Tab[] => [
  {
    id: 'files-tab',
    title: 'Files',
    contentType: 'files',
    icon: FolderTree,
  },
];

/** The shell under the conversation. */
const createTerminalTabs = (): Tab[] => [
  {
    id: 'terminal-tab-1',
    title: 'Terminal',
    contentType: 'terminal',
    icon: Terminal,
  },
];

export function defaultLayout(): WindowState {
  const mainPaneId = generateId();
  const tabGroupId1 = generateId();
  const leftGroupId = generateId();
  const rightGroupId = generateId();
  const rightTermGroupId = generateId();
  const chatGroupId = generateId();
  // Open each edge panel on its FIRST tab, derived rather than hard-coded: a
  // literal id that a tab-roster change orphans leaves the panel showing "No
  // tab selected" (it has happened for both the left and right panels).
  const leftTabs = createSessionTabs();
  const rightTabs = createFileTabs();
  const rightTermTabs = createTerminalTabs();
  return {
    layout: {
      id: mainPaneId,
      type: 'pane' as const,
      tabGroupId: tabGroupId1,
    },
    tabGroups: {
      [chatGroupId]: { id: chatGroupId, tabs: [], activeTabId: null },
      [tabGroupId1]: {
        id: tabGroupId1,
        tabs: createSampleTabs(),
        activeTabId: null,
      },
      [leftGroupId]: {
        id: leftGroupId,
        tabs: leftTabs,
        activeTabId: leftTabs[0]?.id ?? null,
      },
      [rightGroupId]: {
        id: rightGroupId,
        tabs: rightTabs,
        activeTabId: rightTabs[0]?.id ?? null,
      },
      [rightTermGroupId]: {
        id: rightTermGroupId,
        tabs: rightTermTabs,
        activeTabId: rightTermTabs[0]?.id ?? null,
      },
    },
    edgePanels: {
      left: {
        id: 'left-panel',
        layout: {
          id: 'left-split', type: 'split' as const, direction: 'vertical' as const, splitRatio: 0.4,
          first: { id: 'left-pane', type: 'pane' as const, tabGroupId: leftGroupId },
          second: { id: 'left-files-pane', type: 'pane' as const, tabGroupId: rightGroupId },
        },
        mode: 'docked',
        // A nav rail: the session list and its project groups. The
        // conversation itself opens as a pane beside the editor, so this
        // stays a list's width.
        width: 272,
      },
      right: {
        id: 'right-panel',
        // Conversations above a folded terminal; files live below Sessions on the left.
        layout: {
          id: 'right-split',
          type: 'split' as const,
          direction: 'vertical' as const,
          splitRatio: 0.65,
          first: { id: 'right-pane', type: 'pane' as const, tabGroupId: chatGroupId },
          // Keep an idle terminal folded until the user opens it.
          second: {
            id: 'right-term-pane',
            type: 'pane' as const,
            tabGroupId: rightTermGroupId,
            collapsed: true,
          },
        },
        mode: 'docked',
        width: 400,
      },
    },
    floatingWindows: [],
    activePaneId: mainPaneId,
    focusedRegion: 'center',
    nextZIndex: 100,
    expandedEdge: null,
    expandExit: 'toggle',
    ribbonPlacement: 'edge',
    floatingChrome: 'merged',
  };
}
