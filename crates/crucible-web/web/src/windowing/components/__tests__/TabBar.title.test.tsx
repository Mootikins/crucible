import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { FileText } from '@/lib/icons';
import { TabBar, elideTabTitle } from '../TabBar';
import { windowStore, windowActions, setStore } from '@/windowing/store';
import { findFirstPane } from '@/windowing/model/tree';
import { railSeed, configureRails } from './fixtures';

beforeEach(() => configureRails());

const LONG = '2026-09-15 Web UI Review.md';
const LONGER = '2026-09-15 Architecture Review Notes.md';

let paneId: string;
let groupId: string;

beforeEach(() => {
  const fresh = railSeed();
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
});

describe('elideTabTitle', () => {
  it('leaves a short label whole', () => {
    expect(elideTabTitle('A.md')).toBe('A.md');
    expect(elideTabTitle(LONG)).toBe(LONG);
  });

  it('cuts the MIDDLE, keeping the head and the extension', () => {
    const out = elideTabTitle(LONGER);
    expect(out).toContain('…');
    expect(out.startsWith('2026-09-15 A')).toBe(true);
    expect(out.endsWith('Notes.md')).toBe(true);
    expect(out.length).toBeLessThan(LONGER.length);
  });

  it('keeps two dated notes apart, which an end-truncation cannot', () => {
    const a = elideTabTitle('2026-09-15 Architecture Review Notes.md');
    const b = elideTabTitle('2026-09-15 Architecture Review Report.md');
    expect(a).not.toBe(b);
  });

  it('splits by code point, so an emoji is never cut in half', () => {
    const title = '🔥'.repeat(40);
    expect([...elideTabTitle(title)].every((c) => c === '🔥' || c === '…')).toBe(true);
  });
});

describe('TabBar — titles', () => {
  it('caps the title at the tab measure and carries the full label in `title`', () => {
    windowActions.addTab(groupId, {
      id: 'tab-long',
      title: LONGER,
      contentType: 'file',
      icon: FileText,
    });
    windowActions.setActiveTab(groupId, 'tab-long');

    const { container } = render(() => (
      <DragDropProvider>
        <TabBar groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));

    const row = container.querySelector('[data-tab-id="tab-long"]')!;
    const label = [...row.querySelectorAll('span')].find((el) => el.textContent?.includes('…'))!;
    // The class names the TOKEN, and the token carries the width. The app's
    // stylesheet sets the value; style-consistency.test.ts asserts it.
    expect(label.className).toContain('max-w-(--cru-measure-tab)');
    expect(label.getAttribute('title')).toBe(LONGER);
  });
});

describe('TabBar — the title fade is measured', () => {
  it('does not fade a short title on an active tab', () => {
    windowActions.addTab(groupId, {
      id: 'tab-short',
      title: 'Short',
      contentType: 'file',
      icon: FileText,
    });
    windowActions.setActiveTab(groupId, 'tab-short');
    const { container } = render(() => (
      <DragDropProvider>
        <TabBar groupId={groupId} paneId={paneId} />
      </DragDropProvider>
    ));
    // jsdom lays nothing out, so scrollWidth and clientWidth are both 0:
    // nothing overflows, and the fade must stay off. The old code keyed the
    // fade on `isActive`, which faded every active title.
    // A short label carries no tooltip either: `title` only exists when
    // the label is cut.
    const row = container.querySelector('[data-tab-id="tab-short"]')!;
    const title = [...row.querySelectorAll('span')].find((el) => el.textContent === 'Short')!;
    expect(title.getAttribute('title')).toBeNull();
    // The theme fades `.wm-tab-title[data-overflows]`.
    expect(title.classList.contains('wm-tab-title')).toBe(true);
    expect(title.hasAttribute('data-overflows')).toBe(false);
  });

  it('marks a title that its box cuts, so the theme can fade it', () => {
    // jsdom lays nothing out. These two getters give the title a box that is
    // narrower than its text. They shadow the getters on Element.prototype,
    // and the `finally` block removes them.
    Object.defineProperty(HTMLElement.prototype, 'scrollWidth', { configurable: true, get: () => 300 });
    Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 100 });
    try {
      windowActions.addTab(groupId, { id: 'tab-cut', title: 'Cut', contentType: 'file', icon: FileText });
      const { container } = render(() => (
        <DragDropProvider>
          <TabBar groupId={groupId} paneId={paneId} />
        </DragDropProvider>
      ));
      const row = container.querySelector('[data-tab-id="tab-cut"]')!;
      const title = [...row.querySelectorAll('span')].find((el) => el.textContent === 'Cut')!;
      expect(title.hasAttribute('data-overflows')).toBe(true);
    } finally {
      const proto = HTMLElement.prototype as unknown as Record<string, unknown>;
      delete proto.scrollWidth;
      delete proto.clientWidth;
    }
  });
});
