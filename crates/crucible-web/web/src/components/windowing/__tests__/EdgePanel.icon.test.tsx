import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { EdgePanel } from '../EdgePanel';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState } from '@/stores/windowStoreInternals';
import type { EdgePanelPosition } from '@/types/windowTypes';

// The old test scraped EdgePanel.tsx and windowStoreInternals.ts for source
// substrings ("{props.tab.icon ? (", "icon: ClipboardList", …). That never
// renders and breaks on renames. Here we render the edge ribbon and assert the
// real output: tabs with an icon render a Lucide <svg>, tabs without one fall
// back to their title initial, and the default roster's tabs actually carry
// component icons (checked against the live store, not the source text).

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
});

const edgeTabs = (position: EdgePanelPosition) =>
  windowStore.tabGroups[windowStore.edgePanels[position].tabGroupId].tabs;

describe('EdgePanel — tab icons', () => {
  it('renders each ribbon tab with its icon as an <svg>', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));

    // Left roster ships Sessions + Files, both with icons.
    const buttons = container.querySelectorAll('[data-testid="collapsed-tab-button-left"]');
    expect(buttons.length).toBe(edgeTabs('left').length);
    buttons.forEach((btn) => {
      expect(btn.querySelector('svg')).toBeTruthy();
    });
  });

  it('falls back to the title initial for a tab without an icon', () => {
    const groupId = windowStore.edgePanels.left.tabGroupId;
    windowActions.addTab(groupId, { id: 'no-icon-tab', title: 'Zeta', contentType: 'files' });

    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));

    const buttons = Array.from(
      container.querySelectorAll<HTMLButtonElement>('[data-testid="collapsed-tab-button-left"]'),
    );
    const fallback = buttons.find((b) => !b.querySelector('svg'));
    expect(fallback, 'a button rendering the title initial instead of an icon').toBeTruthy();
    expect(fallback!.textContent?.trim()).toBe('Z');
  });

  it('the default edge roster gives every tab a component icon', () => {
    const positions: EdgePanelPosition[] = ['left', 'right', 'bottom'];
    const tabs = positions.flatMap((p) => edgeTabs(p));
    // Sessions, Files, Backlinks, Activity, Terminal, Chat.
    expect(tabs.length).toBe(6);
    for (const tab of tabs) {
      expect(typeof tab.icon, `${tab.title} should carry a component icon`).toBe('function');
    }
  });
});
