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

  it('the left ribbon hosts command buttons (palette, new session, settings)', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <EdgePanel position="left" />
      </DragDropProvider>
    ));

    expect(container.querySelector('[data-testid="ribbon-cmd-palette"]')).toBeTruthy();
    expect(container.querySelector('[data-testid="ribbon-cmd-new-session"]')).toBeTruthy();
    expect(container.querySelector('[data-testid="ribbon-cmd-settings"]')).toBeTruthy();
  });

  it('every edge position renders its own ribbon toggle', () => {
    for (const position of ['left', 'right', 'bottom'] as const) {
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
