import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

// This shell only ever draws on a phone, so the device store says so here.
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => true }));

// Only the queue's answer is staged; the rest of the sync layer is the real
// one, because the badge in the app bar reads it.
const offline = vi.hoisted(() => ({ conflicts: [] as Conflicted[] }));
vi.mock('@/lib/offline/sync', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/offline/sync')>()),
  pendingConflicts: async () => offline.conflicts,
}));

// The drawers' panels need every context; the shell's own job is the frame.
vi.mock('@/components/mobile/SessionsTab', () => ({
  SessionsTab: () => <div data-testid="sessions-panel" />,
}));
vi.mock('@/components/FilesPanel', () => ({
  FilesPanel: () => <div data-testid="files-panel" />,
}));
vi.mock('@/components/BacklinksPanel', () => ({
  BacklinksPanel: () => <div data-testid="backlinks-panel" />,
}));

import { MobileShell } from '@/components/mobile/MobileShell';
import { tabStackActions } from '@/stores/tabStackStore';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { __resetConflictStore } from '@/lib/conflicts';
import type { Tab } from '@/types/windowTypes';
import type { Conflicted } from '@/lib/offline/outbox';

/** One note whose write waits on a person. */
const conflict = (path: string): Conflicted => ({
  path,
  base: 'h0',
  kiln: '/kilns/notes',
  currentHash: 'h9',
  currentContent: 'theirs\n',
  mergedContent: 'mine\n',
  regions: [{ start_line: 1, end_line: 2, base: '', ours: 'mine\n', theirs: 'theirs\n' }],
});

const noteTab = (id: string, title: string): Tab => ({
  id,
  title,
  contentType: 'file',
  metadata: { filePath: `/kiln/${id}.md` },
});

const Stub = () => <div />;

beforeEach(() => {
  localStorage.clear();
  tabStackActions.reset();
  // The app registers panels at boot; the shell reads that registry.
  resetGlobalRegistry();
  const registry = getGlobalRegistry();
  registry.register('search', 'Search', Stub, 'center');
  registry.register('settings', 'Settings', Stub, 'center');
  registry.register('terminal', 'Terminal', Stub, 'right');
  registry.register('canvas', 'Canvas', Stub, 'center');
  registry.register('files', 'Files', Stub, 'right');
  registry.register('conflicts', 'Conflicts', Stub, 'center');
  offline.conflicts = [];
  __resetConflictStore();
});

const isOpen = (side: 'left' | 'right') =>
  !screen.getByTestId(`drawer-${side}`).hasAttribute('inert');
const leftButton = () => screen.getByRole('button', { name: 'Sessions and files' });
const rightButton = () => screen.getByRole('button', { name: 'Backlinks' });

describe('MobileShell', () => {
  // Decision log 2026-09-11: both pickers in the left drawer, as tabs;
  // the note's own context on the right.
  it('holds sessions and files in the left drawer, and backlinks in the right', () => {
    render(() => <MobileShell />);
    const left = screen.getByTestId('drawer-left');
    expect(left.contains(screen.getByTestId('sessions-panel'))).toBe(true);
    expect(left.contains(screen.getByTestId('files-panel'))).toBe(true);
    expect(screen.getByTestId('drawer-right').contains(screen.getByTestId('backlinks-panel'))).toBe(true);
  });

  it('shows one left-drawer tab at a time, sessions first', () => {
    render(() => <MobileShell />);
    fireEvent.click(leftButton());
    const sessionsTab = screen.getByRole('tab', { name: 'Sessions' });
    const filesTab = screen.getByRole('tab', { name: 'Files' });
    expect(sessionsTab.getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('files-panel').closest('[role=tabpanel]')!.hasAttribute('hidden')).toBe(true);

    fireEvent.click(filesTab);
    expect(filesTab.getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('files-panel').closest('[role=tabpanel]')!.hasAttribute('hidden')).toBe(false);
    // Hidden, not unmounted: the tree keeps its expansion and its scroll.
    expect(screen.getByTestId('sessions-panel').closest('[role=tabpanel]')!.hasAttribute('hidden')).toBe(true);
  });

  it('opens each drawer from its app-bar button', () => {
    render(() => <MobileShell />);
    expect(isOpen('left')).toBe(false);
    fireEvent.click(leftButton());
    expect(isOpen('left')).toBe(true);
  });

  it('never shows both drawers at once', () => {
    render(() => <MobileShell />);
    fireEvent.click(leftButton());
    fireEvent.click(rightButton());
    expect(isOpen('right')).toBe(true);
    expect(isOpen('left')).toBe(false);
  });

  it('gives each app-bar button and each drawer tab a touch-sized target', () => {
    render(() => <MobileShell />);
    for (const el of [leftButton(), rightButton()]) {
      expect(el.className).toMatch(/\bw-11\b/);
      expect(el.className).toMatch(/\bh-11\b/);
    }
    fireEvent.click(leftButton());
    for (const name of ['Sessions', 'Files']) {
      expect(screen.getByRole('tab', { name }).className).toMatch(/\bh-11\b/);
    }
  });
});

describe('MobileShell overflow menu', () => {
  // Everything that is not a picker and not the note's context: a user visits
  // these, so they open as a content tab rather than living in a drawer.
  it('opens a panel as a content tab', () => {
    render(() => <MobileShell />);
    fireEvent.click(screen.getByRole('button', { name: 'More' }));
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));
    expect(tabStackActions.activeTab()?.contentType).toBe('search');
    // The menu closes behind the choice.
    expect(screen.queryByRole('button', { name: 'Search' })).toBeNull();
  });

  it('offers the panels a phone can draw, and not the ones it cannot', () => {
    render(() => <MobileShell />);
    fireEvent.click(screen.getByRole('button', { name: 'More' }));
    const labels = screen.getAllByRole('button').map((b) => b.textContent);
    expect(labels).toContain('Search');
    expect(labels).toContain('Settings');
    expect(labels).not.toContain('Terminal');
    expect(labels).not.toContain('Canvas');
  });

  // A phone has no rail to park a count in, so the one menu a thumb reaches
  // is where a conflict announces itself and where it is opened.
  it('lists Conflicts when one waits', async () => {
    offline.conflicts = [conflict('/kilns/notes/A.md')];
    render(() => <MobileShell />);
    fireEvent.click(screen.getByRole('button', { name: 'More' }));

    const row = await screen.findByRole('button', { name: 'Conflicts (1)' });
    fireEvent.click(row);
    expect(tabStackActions.activeTab()?.contentType).toBe('conflicts');
  });

  // The row is a count, not a panel: with nothing waiting it says nothing,
  // and the generic panel list must not offer a second door to the same tab.
  it('offers no Conflicts row when none waits', async () => {
    render(() => <MobileShell />);
    fireEvent.click(screen.getByRole('button', { name: 'More' }));
    await screen.findByRole('button', { name: 'Search' });
    const labels = screen.getAllByRole('button').map((b) => b.textContent);
    expect(labels.filter((l) => l?.startsWith('Conflicts'))).toHaveLength(0);
  });
});

describe('MobileShell tabs', () => {
  it('draws the open tab and names it in the app bar', () => {
    tabStackActions.open(noteTab('a', 'Note A'));
    render(() => <MobileShell />);
    expect(screen.getByRole('heading').textContent).toBe('Note A');
  });

  it('shows the tab count, and the overview behind it', () => {
    tabStackActions.open(noteTab('a', 'Note A'));
    tabStackActions.open(noteTab('b', 'Note B'));
    render(() => <MobileShell />);
    const button = screen.getByRole('button', { name: 'Tabs (2)' });
    fireEvent.click(button);
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
  });

  it('picks a tab from the overview and closes the overview', () => {
    tabStackActions.open(noteTab('a', 'Note A'));
    tabStackActions.open(noteTab('b', 'Note B'));
    render(() => <MobileShell />);
    fireEvent.click(screen.getByRole('button', { name: 'Tabs (2)' }));
    fireEvent.click(screen.getByRole('button', { name: 'Note A' }));
    expect(screen.getByRole('heading').textContent).toBe('Note A');
    expect(screen.queryAllByRole('listitem')).toHaveLength(0);
  });

  it('offers no tab button when nothing is open', () => {
    render(() => <MobileShell />);
    expect(screen.queryByRole('button', { name: /^Tabs/ })).toBeNull();
  });

  // Back walks the tabs the user has seen before it leaves the app.
  it('moves back through visited tabs', () => {
    tabStackActions.open(noteTab('a', 'Note A'));
    tabStackActions.open(noteTab('b', 'Note B'));
    render(() => <MobileShell />);
    expect(screen.getByRole('heading').textContent).toBe('Note B');
    window.dispatchEvent(new PopStateEvent('popstate', { state: null }));
    expect(screen.getByRole('heading').textContent).toBe('Note A');
  });
});
