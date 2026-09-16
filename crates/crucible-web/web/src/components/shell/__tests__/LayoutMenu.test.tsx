import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { windowStore, setStore } from '@/stores/windowStore';
import { primaryEdgeGroupId } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import { createMockFetch, type MockFetch } from '@/test-utils/mock-fetch';

/**
 * The reset reaches the daemon on the WIRE.
 *
 * The layout is deliberately NOT a cache entry — it is a signal written to
 * disk, loaded once at start — so there is no hook to read here. What there is
 * is one `DELETE /api/layout`, and a mocked module counts the calls that reach
 * the module rather than the ones that reach the daemon.
 */
let fetchMock: MockFetch;
let realFetch: typeof fetch;
/** How many times the daemon was asked to forget the layout. */
const resetCount = () => fetchMock.calls('DELETE /api/layout');

const openPanelTab = vi.fn();
vi.mock('@/lib/panel-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openPanelTab: (id: string) => openPanelTab(id),
}));

import { LayoutMenu } from '../LayoutMenu';

const Dummy = () => null;

const resetStore = () => {
  const fresh = defaultLayout();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = 'center';
      s.nextZIndex = 100;
    }),
  );
};

/** Open the kebab and return its popout. */
const openMenu = async (getByTestId: (id: string) => HTMLElement) => {
  fireEvent.click(getByTestId('layout-menu'));
  return waitFor(() => {
    const el = document.querySelector<HTMLElement>(
      '[data-scope="menu"][data-part="content"][data-state="open"]',
    );
    expect(el).toBeTruthy();
    return el!;
  });
};

describe('LayoutMenu — the rail kebab is the layout control', () => {
  beforeEach(() => {
    resetGlobalRegistry();
    const registry = getGlobalRegistry();
    registry.register('sessions', 'Sessions', Dummy, 'left');
    registry.register('files', 'Files', Dummy, 'right');
    registry.register('graph', 'Graph', Dummy, 'center');
    registry.register('skills', 'Skills', Dummy, 'left');
    // Registered but never re-addable on its own: a file tab names a file.
    registry.register('file', 'File', Dummy, 'center');
    resetStore();
    realFetch = global.fetch;
    fetchMock = createMockFetch({ 'DELETE /api/layout': { status: 204 } });
    global.fetch = fetchMock;
    openPanelTab.mockClear();
  });

  afterEach(() => {
    global.fetch = realFetch;
    vi.restoreAllMocks();
    resetGlobalRegistry();
  });

  it('offers Re-add pane and Reset layout', async () => {
    const { getByTestId } = render(() => <LayoutMenu />);
    await openMenu(getByTestId);
    expect(document.querySelector('[data-testid="layout-readd"]')).toBeTruthy();
    expect(document.querySelector('[data-testid="layout-reset"]')).toBeTruthy();
  });

  it('lists only the panels that are closed', async () => {
    const { getByTestId } = render(() => <LayoutMenu />);
    await openMenu(getByTestId);
    fireEvent.click(document.querySelector<HTMLElement>('[data-testid="layout-readd"]')!);
    await waitFor(() =>
      expect(document.querySelector('[data-testid="layout-readd-graph"]')).toBeTruthy(),
    );
    // Sessions and Files are open in the two rails; File is never offered.
    expect(document.querySelector('[data-testid="layout-readd-sessions"]')).toBeNull();
    expect(document.querySelector('[data-testid="layout-readd-files"]')).toBeNull();
    expect(document.querySelector('[data-testid="layout-readd-file"]')).toBeNull();
    expect(document.querySelector('[data-testid="layout-readd-skills"]')).toBeTruthy();
  });

  it('re-adds a closed panel in its default zone', async () => {
    const { getByTestId } = render(() => <LayoutMenu />);
    await openMenu(getByTestId);
    fireEvent.click(document.querySelector<HTMLElement>('[data-testid="layout-readd"]')!);
    const item = await waitFor(() => {
      const el = document.querySelector<HTMLElement>('[data-testid="layout-readd-graph"]');
      expect(el).toBeTruthy();
      return el!;
    });
    fireEvent.pointerDown(item);
    fireEvent.click(item);
    await waitFor(() => expect(openPanelTab).toHaveBeenCalledWith('graph'));
  });

  it('re-lists a panel the user closed again', async () => {
    setStore(
      produce((s) => {
        const groupId = primaryEdgeGroupId(s, 'left')!;
        s.tabGroups[groupId].tabs = [
          ...s.tabGroups[groupId].tabs,
          { id: 'graph-tab', title: 'Graph', contentType: 'graph' },
        ];
      }),
    );
    const { getByTestId } = render(() => <LayoutMenu />);
    await openMenu(getByTestId);
    fireEvent.click(document.querySelector<HTMLElement>('[data-testid="layout-readd"]')!);
    await waitFor(() =>
      expect(document.querySelector('[data-testid="layout-readd-skills"]')).toBeTruthy(),
    );
    expect(document.querySelector('[data-testid="layout-readd-graph"]')).toBeNull();
  });

  it('asks before it resets, and does nothing when the user declines', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const { getByTestId } = render(() => <LayoutMenu />);
    await openMenu(getByTestId);
    const item = document.querySelector<HTMLElement>('[data-testid="layout-reset"]')!;
    fireEvent.pointerDown(item);
    fireEvent.click(item);
    await waitFor(() => expect(confirm).toHaveBeenCalled());
    expect(resetCount()).toBe(0);
  });

  it('resets the server copy and the local layout, and keeps both rails', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    setStore('edgePanels', 'left', 'mode', 'strip');
    const { getByTestId } = render(() => <LayoutMenu />);
    await openMenu(getByTestId);
    const item = document.querySelector<HTMLElement>('[data-testid="layout-reset"]')!;
    fireEvent.pointerDown(item);
    fireEvent.click(item);
    await waitFor(() => expect(resetCount()).toBe(1));
    await waitFor(() => expect(windowStore.edgePanels.left.mode).toBe('docked'));
    const leftGroup = windowStore.tabGroups[primaryEdgeGroupId(windowStore, 'left')!];
    const rightGroup = windowStore.tabGroups[primaryEdgeGroupId(windowStore, 'right')!];
    expect(leftGroup.tabs.map((t) => t.contentType)).toContain('sessions');
    expect(rightGroup.tabs.map((t) => t.contentType)).toContain('files');
  });
});
