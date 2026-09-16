import {
  Activity,
  ClipboardList,
  FolderTree,
  Link2,
  Terminal,
} from '@/lib/icons';
import type { Tab, WindowState } from '@/types/windowTypes';
import { generateId } from '@/windowing/model/tree';

const createSampleTabs = (): Tab[] => [];

// Only IMPLEMENTED panels ship in the default layout — no placeholder tabs.
// Identity on the left, working context on the right. A persisted layout
// reaches the same shape through migrateV5toV6, and this default must match
// it or a FRESH profile opens tabs with no registered panel ("Unknown content
// type").
// Search is deliberately absent: it spans files, notes and sessions, so it
// belongs to neither rail and is opened on demand (Ctrl+Shift+F / palette).
const createLeftPanelTabs = (): Tab[] => [
  {
    id: 'sessions-tab',
    title: 'Sessions',
    contentType: 'sessions',
    icon: ClipboardList,
  },
];

const createRightPanelTabs = (): Tab[] => [
  {
    id: 'files-tab',
    title: 'Files',
    contentType: 'files',
    icon: FolderTree,
  },
  {
    id: 'backlinks-tab',
    title: 'Backlinks',
    contentType: 'backlinks',
    icon: Link2,
  },
  {
    id: 'activity-tab',
    title: 'Activity',
    contentType: 'activity',
    icon: Activity,
  },
];

/** The shell under the file tree. */
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
  // Open each edge panel on its FIRST tab, derived rather than hard-coded: a
  // literal id that a tab-roster change orphans leaves the panel showing "No
  // tab selected" (it has happened for both the left and right panels).
  const leftTabs = createLeftPanelTabs();
  const rightTabs = createRightPanelTabs();
  const rightTermTabs = createTerminalTabs();
  return {
    layout: {
      id: mainPaneId,
      type: 'pane' as const,
      tabGroupId: tabGroupId1,
    },
    tabGroups: {
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
        layout: { id: 'left-pane', type: 'pane' as const, tabGroupId: leftGroupId },
        mode: 'docked',
        // A nav rail: the session list and its project groups. The
        // conversation itself opens as a pane beside the editor, so this
        // stays a list's width.
        width: 280,
      },
      right: {
        id: 'right-panel',
        // A COLUMN, not a single pane: the file tree above, a terminal under
        // it. The terminal used to be a full-width dock across the bottom of
        // the window, which cost the editor its height to show a shell that
        // belongs beside the files it runs against. Stacked inside one rail it
        // also survives a flip intact — `mirrorLayout` reverses columns and
        // leaves what is stacked inside them alone.
        layout: {
          id: 'right-split',
          type: 'split' as const,
          direction: 'vertical' as const,
          splitRatio: 0.65,
          first: { id: 'right-pane', type: 'pane' as const, tabGroupId: rightGroupId },
          // The shell ships COLLAPSED: a fresh rail shows the tree at full
          // height with a terminal BAR under it, which is the honest default —
          // a shell nobody started yet does not deserve a third of the rail.
          // One click on its ribbon marker (or its bar) opens it, and the
          // 0.65 ratio above is what it opens back to.
          // A persisted layout reaches the same shape through migrateV7toV8.
          second: {
            id: 'right-term-pane',
            type: 'pane' as const,
            tabGroupId: rightTermGroupId,
            collapsed: true,
          },
        },
        mode: 'strip',
        // The tree side: files, backlinks, activity, and the shell. A
        // sidebar's width, plus room for a command line.
        width: 340,
      },
    },
    floatingWindows: [],
    activePaneId: mainPaneId,
    focusedRegion: 'center',
    nextZIndex: 100,
  };
}
