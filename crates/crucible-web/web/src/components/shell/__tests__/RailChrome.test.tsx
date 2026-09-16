import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import type { ParentComponent } from 'solid-js';
import { EdgeHost } from '@/windowing/components/EdgeHost';
import { WindowingProvider } from '@/windowing/components/context';
import { renderPanel } from '@/lib/render-panel';
import { appWindowSlots } from '@/components/shell/windowSlots';
import { setStore } from '@/stores/windowStore';
import { defaultLayout } from '@/stores/defaultLayout';

/** The rails as AppShell draws them: the app chrome on the core's slots. */
const AppProviders: ParentComponent = (props) => (
  <WindowingProvider renderContent={renderPanel} slots={appWindowSlots}>
    <DragDropProvider>{props.children}</DragDropProvider>
  </WindowingProvider>
);

const renderRail = (position: 'left' | 'right') =>
  render(() => (
    <AppProviders>
      <EdgeHost position={position} />
    </AppProviders>
  ));

beforeEach(() => {
  const fresh = defaultLayout();
  setStore(produce((s) => Object.assign(s, fresh)));
});

describe('the app rail chrome', () => {
  it('the left ribbon carries the shell-wide toggles and the layout menu', () => {
    const { container } = renderRail('left');

    // What the rail keeps: the three toggles that act on the WHOLE shell and
    // have nowhere else to live.
    for (const id of ['ribbon-cmd-swap-sides', 'ribbon-cmd-theme', 'ribbon-cmd-settings']) {
      expect(container.querySelector(`[data-testid="${id}"]`), id).toBeTruthy();
    }
    expect(container.querySelector('[data-testid="layout-menu"]'), 'layout menu').toBeTruthy();
  });

  it('claims the ribbon floor exactly once on each rail', () => {
    // The bottom cluster is held down by a single `mt-auto`. A second claimant
    // splits the free space and the whole cluster floats mid-rail.
    for (const position of ['left', 'right'] as const) {
      const { container, unmount } = renderRail(position);
      const floors = container.querySelectorAll('[data-ribbon-floor]');
      expect(floors.length, `${position} ribbon floor claimants`).toBe(1);
      expect(
        (floors[0].getAttribute('class') ?? '').includes('mt-auto'),
        `${position} floor claimant carries mt-auto`,
      ).toBe(true);
      unmount();
    }
  });

  it('pins the notification bell to the bottom of the RIGHT ribbon only', () => {
    // It used to float in the centre pane's bottom-right corner, over the
    // document, and disappeared with the rest of the transient chip cluster.
    const seen: Record<string, HTMLElement | null> = {};
    for (const position of ['left', 'right'] as const) {
      const { container, unmount } = renderRail(position);
      seen[position] = container.querySelector<HTMLElement>('[data-testid="corner-bell"]');
      // Read before unmount — the node is detached afterwards. The assertion
      // is position, not class: the cluster of ribbon buttons for a trailing
      // pane may claim the floor instead of the bell.
      if (seen[position]) {
        const ribbon = container.querySelector(`[data-testid="edge-collapsed-drop-${position}"]`)!;
        const buttons = Array.from(ribbon.querySelectorAll('button'));
        expect(buttons[buttons.length - 1], `${position} bell is last`).toBe(seen[position]);
      }
      unmount();
    }

    expect(seen.right, 'right ribbon hosts the bell').toBeTruthy();
    expect(seen.left, 'left ribbon must not').toBeNull();
  });
});
