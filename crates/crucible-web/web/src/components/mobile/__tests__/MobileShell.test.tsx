import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

// The drawers' panels need every context; the shell's own job is the frame.
vi.mock('@/components/SessionsPanel', () => ({
  SessionsPanel: () => <div data-testid="sessions-panel" />,
}));
vi.mock('@/components/FilesPanel', () => ({
  FilesPanel: () => <div data-testid="files-panel" />,
}));
vi.mock('@/components/BacklinksPanel', () => ({
  BacklinksPanel: () => <div data-testid="backlinks-panel" />,
}));

import { MobileShell } from '@/components/mobile/MobileShell';
import { tabStackActions } from '@/stores/tabStackStore';
import type { Tab } from '@/types/windowTypes';

const noteTab = (id: string, title: string): Tab => ({
  id,
  title,
  contentType: 'file',
  metadata: { filePath: `/kiln/${id}.md` },
});

beforeEach(() => {
  localStorage.clear();
  tabStackActions.reset();
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
