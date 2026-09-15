import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { darkTokens, resolveToken } from '@/test-utils/css-tokens';
import { FileText } from '@/lib/icons';
import { TabBar, elideTabTitle } from '../TabBar';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane } from '@/stores/windowStoreInternals';

const LONG = '2026-09-15 Web UI Review.md';
const LONGER = '2026-09-15 Architecture Review Notes.md';

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
  it('caps the title at 200px and carries the full label in `title`', () => {
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
    // The class names the TOKEN, and the token carries the width. Asserting
    // the literal class would go green after someone re-valued the token, and
    // asserting only the token would go green at any width.
    //
    // The value is `rem`, so the cap follows a reader's font-size preference:
    // a wider title needs a wider cap. 12.5rem is 200px at the default root.
    expect(label.className).toContain('max-w-(--cru-measure-tab)');
    expect(resolveToken(darkTokens, '--cru-measure-tab')).toBe('12.5rem');
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
    expect(title.className).not.toContain('tab-title-fade');
  });
});
