import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { FileText } from '@/lib/icons';
import { TabBar } from '../TabBar';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane } from '@/stores/windowStoreInternals';

// The old test scraped TabBar.tsx for the absence of "▼" and the presence of a
// "<ChevronDown …class=…" substring with a regex. That never renders the bar
// and breaks on a class rename. Here we render the tab bar and assert the real
// output: tab rows carry Lucide <svg> icons (never the "▼" glyph), and the
// overflow control — forced visible by faking scroll overflow — is a
// ChevronDown <svg>, not a text arrow.

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
  windowActions.addTab(groupId, { id: 'tab-a', title: 'A.md', contentType: 'file', icon: FileText });
  windowActions.setActiveTab(groupId, 'tab-a');
});

describe('TabBar — icons', () => {
  it('renders tab-row icons as <svg> and never the "▼" glyph', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar mode="center" groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    const row = container.querySelector('[data-tab-id="tab-a"]')!;
    expect(row.querySelector('svg')).toBeTruthy();
    expect(container.textContent ?? '').not.toContain('▼');
  });

  it('overflow control renders a ChevronDown <svg>, not a "▼" arrow', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar mode="center" groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    // jsdom reports zero layout, so the overflow check never trips on its own.
    // Fake a scrolling tab strip, then nudge the tabs signal so TabStrip's
    // overflow effect re-runs and reveals the "Show all tabs" button.
    const strip = container.querySelector<HTMLElement>('.overflow-x-auto')!;
    Object.defineProperty(strip, 'scrollWidth', { configurable: true, value: 1000 });
    Object.defineProperty(strip, 'clientWidth', { configurable: true, value: 10 });
    windowActions.addTab(groupId, { id: 'tab-b', title: 'B.md', contentType: 'file' });

    const overflowBtn = container.querySelector<HTMLButtonElement>(
      'button[aria-label="Show all tabs"]',
    );
    expect(overflowBtn, 'overflow button visible once the strip overflows').toBeTruthy();
    expect(overflowBtn!.querySelector('svg')).toBeTruthy();
    expect(overflowBtn!.textContent ?? '').not.toContain('▼');
  });
});
