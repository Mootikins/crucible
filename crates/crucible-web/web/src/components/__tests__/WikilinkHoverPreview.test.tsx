import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';

const fetchNotePreviewMock = vi.fn();

vi.mock('@/lib/note-actions', () => ({
  fetchNotePreview: (...args: unknown[]) => fetchNotePreviewMock(...args),
}));

import { WikilinkHoverPreview } from '../WikilinkHoverPreview';
import { windowStore, setStore } from '@/stores/windowStore';
import { createInitialState } from '@/stores/windowStoreInternals';

const PREVIEW = {
  title: 'Rust',
  path: 'notes/rust.md',
  absPath: '/kiln/notes/rust.md',
  excerpt: 'Rust is great.',
};

// jsdom getBoundingClientRect() is all zeros, so "away" hovers must carry
// coordinates outside the controller's anchor-rect grace margin (±8px).
function hover(el: Element, at: { x: number; y: number } = { x: 0, y: 0 }): void {
  el.dispatchEvent(
    new MouseEvent('mouseover', { bubbles: true, clientX: at.x, clientY: at.y }),
  );
}
const AWAY = { x: 500, y: 500 };

function mountWithAnchor(html: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = html;
  document.body.appendChild(host);
  render(() => <WikilinkHoverPreview />, {
    container: document.body.appendChild(document.createElement('div')),
  });
  return host;
}

const hoverWindows = () => windowStore.floatingWindows;
const hoverTab = (w: (typeof windowStore.floatingWindows)[number]) =>
  windowStore.tabGroups[w.tabGroupId]?.tabs[0];

beforeEach(() => {
  vi.clearAllMocks();
  document.body.innerHTML = '';
  const fresh = createInitialState();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.nextZIndex = 100;
    }),
  );
});

describe('WikilinkHoverPreview (Hover Editor popovers)', () => {
  it('hovering a resolved link spawns a transient floating window with the file tab', async () => {
    fetchNotePreviewMock.mockResolvedValue(PREVIEW);
    const host = mountWithAnchor('<a data-note="rust">rust</a>');
    hover(host.querySelector('a[data-note]')!);

    await waitFor(() => {
      expect(hoverWindows()).toHaveLength(1);
    });
    const win = hoverWindows()[0];
    expect(win.transient).toBe(true);
    expect(win.showTabBar).toBe(false);
    expect(win.title).toBe('Rust');
    const tab = hoverTab(win);
    expect(tab?.contentType).toBe('file');
    expect(tab?.metadata?.filePath).toBe('/kiln/notes/rust.md');
    // Popovers default to the fully rendered reading view (configurable).
    expect(tab?.metadata?.initialMode).toBe('reading');
    // Popover, not workspace state: never part of the persisted layout.
    expect(fetchNotePreviewMock).toHaveBeenCalledWith('rust', undefined);
  });

  it('hovering away closes the transient window', async () => {
    fetchNotePreviewMock.mockResolvedValue(PREVIEW);
    const host = mountWithAnchor('<a data-note="rust">rust</a><span>away</span>');
    hover(host.querySelector('a[data-note]')!);
    await waitFor(() => expect(hoverWindows()).toHaveLength(1));

    hover(host.querySelector('span')!, AWAY);
    await waitFor(() => {
      expect(hoverWindows()).toHaveLength(0);
    });
    // The tab group went with it — no orphaned tabs.
    expect(
      Object.values(windowStore.tabGroups).some((g) =>
        g.tabs.some((t) => t.metadata?.filePath === '/kiln/notes/rust.md'),
      ),
    ).toBe(false);
  });

  it('a pinned window survives hover-away (Hover Editor pin)', async () => {
    vi.useFakeTimers();
    try {
      fetchNotePreviewMock.mockResolvedValue(PREVIEW);
      const host = mountWithAnchor('<a data-note="rust">rust</a><span>away</span>');
      hover(host.querySelector('a[data-note]')!);
      // Drive the SHOW_DELAY gate deterministically instead of polling: the
      // popover opens once the show timer fires and the (mocked) preview fetch
      // settles.
      await vi.advanceTimersByTimeAsync(300);
      expect(hoverWindows()).toHaveLength(1);

      const { windowActions } = await import('@/stores/windowStore');
      windowActions.pinFloatingWindow(hoverWindows()[0].id);

      hover(host.querySelector('span')!, AWAY);
      // Advance well past HIDE_DELAY — a pinned (non-transient) window must
      // survive the hover-away close timer.
      await vi.advanceTimersByTimeAsync(600);
      expect(hoverWindows()).toHaveLength(1);
      expect(hoverWindows()[0].transient).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not spawn a second window while one for the note is open', async () => {
    vi.useFakeTimers();
    try {
      fetchNotePreviewMock.mockResolvedValue(PREVIEW);
      const host = mountWithAnchor(
        '<a id="a" data-note="rust">one</a><a id="b" data-note="rust">two</a>',
      );
      hover(host.querySelector('#a')!);
      await vi.advanceTimersByTimeAsync(300);
      expect(hoverWindows()).toHaveLength(1);
      hover(host.querySelector('#b')!);
      // Advance past both the hide and show delays so any duplicate spawn the
      // second hover could schedule would have fired — the negative assert is
      // now deterministic, not merely "nothing happened within this tick".
      await vi.advanceTimersByTimeAsync(600);
      expect(hoverWindows()).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it('shows a small not-found card for unresolvable notes (no window)', async () => {
    fetchNotePreviewMock.mockResolvedValue(null);
    const host = mountWithAnchor('<a data-note="ghost">ghost</a>');
    hover(host.querySelector('a[data-note]')!);

    await waitFor(() => {
      const missing = document.querySelector('[data-testid="wikilink-preview-missing"]');
      expect(missing?.textContent).toContain('ghost');
    });
    expect(hoverWindows()).toHaveLength(0);
  });

  it('does nothing for elements without data-note', async () => {
    vi.useFakeTimers();
    try {
      const host = mountWithAnchor('<span>plain text</span>');
      hover(host.querySelector('span')!, AWAY);
      // Past every hover delay: a non-anchor hover must never schedule a
      // fetch or a window.
      await vi.advanceTimersByTimeAsync(600);
      expect(hoverWindows()).toHaveLength(0);
      expect(fetchNotePreviewMock).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('passes an explicit data-kiln through to resolution', async () => {
    fetchNotePreviewMock.mockResolvedValue(null);
    const host = mountWithAnchor('<a data-note="rust" data-kiln="/other-kiln">rust</a>');
    hover(host.querySelector('a[data-note]')!);
    await waitFor(() => {
      expect(fetchNotePreviewMock).toHaveBeenCalledWith('rust', '/other-kiln');
    });
  });
});
