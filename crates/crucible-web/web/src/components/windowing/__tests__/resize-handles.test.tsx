import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { SplitPane } from '../SplitPane';
import { EdgePanel } from '../EdgePanel';
import { FloatingWindow } from '../FloatingWindow';
import { windowStore, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane, generateId } from '@/stores/windowStoreInternals';

/**
 * Separator + panel-chrome contract (Obsidian-style):
 * - Visible separators are 1px lines (w-px / h-px), never filled bars.
 * - The pointer grab zone is widened invisibly via an after: pseudo-element.
 * - Panel toggle controls live in an always-visible ribbon.
 *
 * The old suite asserted all of this by grepping the component SOURCE for class
 * literals and structural regexes. This version RENDERS each component and
 * asserts on the emitted DOM — the separators, the ribbon controls, and the
 * floating-window grab zones as they actually appear.
 */

let mainPaneId: string;
let mainGroupId: string;

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
  mainPaneId = pane.id;
  mainGroupId = pane.tabGroupId!;
});

// A horizontal split so both panes + the divider mount.
function splitLayout() {
  const secondPaneId = generateId();
  const secondGroupId = generateId();
  setStore(
    produce((s) => {
      s.tabGroups[secondGroupId] = { id: secondGroupId, tabs: [], activeTabId: null };
      s.layout = {
        id: generateId(),
        type: 'split',
        direction: 'horizontal',
        splitRatio: 0.5,
        first: { id: mainPaneId, type: 'pane', tabGroupId: mainGroupId },
        second: { id: secondPaneId, type: 'pane', tabGroupId: secondGroupId },
      };
    }),
  );
  return windowStore.layout;
}

describe('SplitPane splitter — rendered DOM', () => {
  it('is a 1px line (w-px, cursor-col-resize) with a widened after: grab zone', () => {
    const layout = splitLayout();
    const { container } = render(() => (
      <DragDropProvider>
        <SplitPane node={layout} />
      </DragDropProvider>
    ));

    const splitter = container.querySelector<HTMLElement>('[data-testid="resize-splitter"]')!;
    expect(splitter).toBeTruthy();
    expect(splitter.getAttribute('data-split-id')).toBeTruthy();

    const cls = splitter.className;
    expect(cls).toContain('w-px');
    expect(cls).toContain('cursor-col-resize');
    // Invisible widened pointer target.
    expect(cls).toContain('after:absolute');
    expect(cls).toContain('after:-inset-x-1');
    // Not a filled bar.
    expect(cls).not.toMatch(/\bw-1\.5\b|\bw-2\b/);
    // 1px separators render clean — no grip glyph inside.
    expect(splitter.querySelector('svg')).toBeNull();
  });
});

describe('EdgePanel resize handle — rendered DOM', () => {
  it('is a 1px separator line with a widened after: grab zone, no grip glyph', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));

    const handle = container.querySelector<HTMLElement>('[role="separator"]')!;
    expect(handle).toBeTruthy();
    expect(handle.getAttribute('aria-orientation')).toBe('vertical');

    const cls = handle.className;
    expect(cls).toContain('w-px');
    expect(cls).toContain('cursor-col-resize');
    expect(cls).toContain('after:absolute');
    expect(cls).toContain('after:-inset-x-1');
    expect(cls).not.toMatch(/\bw-1\.5\b/);
    // No grip glyph — the 1px line is the whole separator.
    expect(handle.querySelector('svg')).toBeNull();
  });
});

describe('EdgePanel ribbon chrome — rendered DOM', () => {
  it('renders an always-visible ribbon with its panel toggle (w-4 svg glyph)', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));

    const toggle = container.querySelector<HTMLElement>('[data-testid="ribbon-toggle-left"]')!;
    expect(toggle).toBeTruthy();
    const svg = toggle.querySelector('svg');
    expect(svg).toBeTruthy();
    // Lucide default is a jarring 24px; the toggle glyph must be sized w-4.
    expect(svg!.getAttribute('class') ?? '').toContain('w-4');
  });

  it('the left ribbon carries the shell-wide toggles, and only those', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));

    // What the rail keeps: the three toggles that act on the WHOLE shell and
    // have nowhere else to live.
    for (const id of ['ribbon-cmd-swap-sides', 'ribbon-cmd-theme', 'ribbon-cmd-settings']) {
      expect(container.querySelector(`[data-testid="${id}"]`), id).toBeTruthy();
    }

    // What it dropped, and must not grow back: a command-palette bolt and a
    // new-session plus. Neither was the fastest route to its own action
    // (Ctrl+P; the session tree's hover button), and each spent the rail's
    // most reachable pixels on a third doorway.
    for (const id of ['ribbon-cmd-palette', 'ribbon-cmd-new-session']) {
      expect(container.querySelector(`[data-testid="${id}"]`), id).toBeNull();
    }
  });

  it('claims the ribbon floor exactly once', () => {
    // The bottom cluster is held down by a single `mt-auto`. A second claimant
    // splits the free space and the whole cluster floats mid-rail, so this is
    // an invariant, not a style detail. `data-ribbon-floor` marks the
    // claimant, and RibbonPaneStrip reads the first one to bound its overlay —
    // two of them and the overlay is bounded by the wrong element.
    for (const position of ['left', 'right'] as const) {
      const { container, unmount } = render(() => (
        <DragDropProvider>
          <EdgePanel position={position} />
        </DragDropProvider>
      ));
      const floors = container.querySelectorAll('[data-ribbon-floor]');
      expect(floors.length, `${position} ribbon floor claimants`).toBe(1);
      expect(
        (floors[0].getAttribute('class') ?? '').includes('mt-auto'),
        `${position} floor claimant carries mt-auto`,
      ).toBe(true);
      unmount();
    }
  });

  it('every edge position renders its own ribbon toggle', () => {
    for (const position of ['left', 'right'] as const) {
      const { container, unmount } = render(() => (
        <DragDropProvider>
          <EdgePanel position={position} />
        </DragDropProvider>
      ));
      expect(
        container.querySelector(`[data-testid="ribbon-toggle-${position}"]`),
        `${position} ribbon toggle`,
      ).toBeTruthy();
      unmount();
    }
  });

  it('pins the notification bell to the bottom of the RIGHT ribbon only', () => {
    // It used to float in the centre pane's bottom-right corner, over the
    // document, and disappeared with the rest of the transient chip cluster.
    // `mt-auto` is the same bottom-anchor the left ribbon's settings gear uses.
    const seen: Record<string, HTMLElement | null> = {};
    for (const position of ['left', 'right'] as const) {
      const { container, unmount } = render(() => (
        <DragDropProvider>
          <EdgePanel position={position} />
        </DragDropProvider>
      ));
      seen[position] = container.querySelector<HTMLElement>('[data-testid="corner-bell"]');
      // Read before unmount — the node is detached afterwards.
      //
      // The bell is LAST in the bottom cluster, but it is not always what
      // holds that cluster down: when the panel has a trailing pane, the
      // cluster of ribbon buttons for that pane claims the space instead, and
      // a second `mt-auto` here would split it. So the assertion is position,
      // not class — the previous one pinned `mt-auto` on the bell itself and
      // broke the moment anything else could legitimately claim the floor.
      if (seen[position]) {
        const ribbon = seen[position]!.closest('[data-testid^="edge-ribbon"]') ?? container;
        const buttons = Array.from(ribbon.querySelectorAll('button'));
        expect(buttons[buttons.length - 1], `${position} bell is last`).toBe(seen[position]);
      }
      unmount();
    }

    expect(seen.right, 'right ribbon hosts the bell').toBeTruthy();
    expect(seen.left, 'left ribbon must not').toBeNull();

  });

  it('the expanded tab bar has no duplicate in-bar collapse control', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));
    // The old duplicate collapse button carried these test ids / would render a
    // PanelClose glyph in the tab bar; the ribbon toggle is now canonical.
    expect(container.querySelector('[data-testid^="edge-collapse-"]')).toBeNull();
  });
});

describe('FloatingWindow grab zones — rendered DOM', () => {
  it('edge grab zones are 6px, corners are 12px', () => {
    const groupId = generateId();
    const winId = generateId();
    setStore(
      produce((s) => {
        s.tabGroups[groupId] = { id: groupId, tabs: [], activeTabId: null };
        s.floatingWindows = [
          {
            id: winId,
            tabGroupId: groupId,
            x: 100,
            y: 100,
            width: 400,
            height: 300,
            isMinimized: false,
            isMaximized: false,
            zIndex: 100,
            title: 'Floating',
          },
        ];
      }),
    );

    const { container } = render(() => (
      <DragDropProvider>
        <FloatingWindow window={windowStore.floatingWindows[0]} />
      </DragDropProvider>
    ));

    const handles = Array.from(container.querySelectorAll<HTMLElement>('div')).filter((el) =>
      (el.style.cursor ?? '').endsWith('resize'),
    );
    const byCursor = (cursor: string) => handles.find((h) => h.style.cursor === cursor)!;

    // All 8 grab zones present.
    expect(handles.length).toBe(8);

    // North edge: 6px tall.
    expect(byCursor('n-resize').style.height).toBe('6px');
    // West edge: 6px wide.
    expect(byCursor('w-resize').style.width).toBe('6px');
    // NW corner: 12px square.
    expect(byCursor('nw-resize').style.width).toBe('12px');
    expect(byCursor('nw-resize').style.height).toBe('12px');
  });
});
