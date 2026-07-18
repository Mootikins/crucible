import { describe, it, expect, beforeEach } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { TabBar } from '../TabBar';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane } from '@/stores/windowStoreInternals';

// The old test grepped TabBar.tsx for the exact classList literal
// ("'opacity-0 group-hover:opacity-100': !props.isActive") plus a couple of
// Tailwind class strings. That breaks on any benign class rename and never
// proves the button works. Here we render the tab bar with an active and an
// inactive tab and assert the emitted DOM: the active tab's close button is
// visible (no opacity-0), the inactive one is hover-revealed (opacity-0), and
// clicking a close button actually removes the tab.

let paneId: string;
let groupId: string;

beforeEach(() => {
  const fresh = createInitialState();
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
  const pane = findFirstPane(windowStore.layout)!;
  paneId = pane.id;
  groupId = pane.tabGroupId!;
  windowActions.addTab(groupId, { id: 'tab-a', title: 'A.md', contentType: 'file' });
  windowActions.addTab(groupId, { id: 'tab-b', title: 'B.md', contentType: 'file' });
  windowActions.setActiveTab(groupId, 'tab-a');
});

const closeButton = (container: HTMLElement, tabId: string) =>
  container.querySelector<HTMLButtonElement>(
    `[data-tab-id="${tabId}"] button[aria-label="Close tab"]`,
  );

describe('TabBar — close button visibility & behavior', () => {
  it('renders a close button on every tab', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar mode="center" groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    expect(closeButton(container, 'tab-a')).toBeTruthy();
    expect(closeButton(container, 'tab-b')).toBeTruthy();
    expect(container.querySelectorAll('button[aria-label="Close tab"]').length).toBe(2);
  });

  it('active tab close button is always visible; inactive is hover-revealed', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar mode="center" groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    // tab-a is active → not hidden.
    const active = closeButton(container, 'tab-a')!;
    expect(active.className).not.toContain('opacity-0');

    // tab-b is inactive → hidden until hover/focus.
    const inactive = closeButton(container, 'tab-b')!;
    expect(inactive.className).toContain('opacity-0');
    expect(inactive.className).toContain('group-hover:opacity-100');
    // Focus reveals it too.
    expect(inactive.className).toContain('focus:opacity-100');
  });

  it('reflects the active tab flipping (visibility follows isActive)', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar mode="center" groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    windowActions.setActiveTab(groupId, 'tab-b');

    expect(closeButton(container, 'tab-b')!.className).not.toContain('opacity-0');
    expect(closeButton(container, 'tab-a')!.className).toContain('opacity-0');
  });

  it('clicking a close button removes that tab', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar mode="center" groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    expect(windowStore.tabGroups[groupId].tabs.length).toBe(2);
    // tab-b is not modified, so confirmTabClose returns true without a prompt.
    fireEvent.click(closeButton(container, 'tab-b')!);

    const remaining = windowStore.tabGroups[groupId].tabs.map((t) => t.id);
    expect(remaining).toEqual(['tab-a']);
    expect(container.querySelector('[data-tab-id="tab-b"]')).toBeNull();
  });
});
