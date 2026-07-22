/**
 * App-wide wikilink hover popovers — Obsidian Hover Editor semantics.
 *
 * Mounted once; listens at the document level for hover on any element
 * carrying a `data-note` attribute (chat message anchors, editor
 * decorations, panel rows). A resolved note spawns a TRANSIENT floating
 * window — the same FloatingWindow as pop-outs and tear-offs, with a real
 * editor inside — positioned next to the anchor. It auto-closes when the
 * pointer leaves the anchor and the window, unless pinned (titlebar pin,
 * or auto-pin on drag/resize). Loading and note-not-found states render a
 * small card, since there is nothing to put in a window yet.
 */
import { Component, Show, createSignal, onMount, onCleanup } from 'solid-js';
import { fetchNotePreview, type NotePreview } from '@/lib/note-actions';
import { statusBarStore } from '@/stores/statusBarStore';
import { windowStore, windowActions } from '@/stores/windowStore';
import { iconForContentType } from '@/lib/tab-icons';
import { useSettingsSafe } from '@/contexts/SettingsContext';

const SHOW_DELAY_MS = 300;
const HIDE_DELAY_MS = 300;
const WINDOW_WIDTH = 460;
const WINDOW_HEIGHT = 320;
const EDGE_GAP_PX = 8;

type CardState = { kind: 'loading' } | { kind: 'missing'; name: string };

export const WikilinkHoverPreview: Component = () => {
  const { settings } = useSettingsSafe();
  const [card, setCard] = createSignal<CardState | null>(null);
  const [cardPos, setCardPos] = createSignal<{ left: number; top: number }>({ left: 0, top: 0 });

  let currentAnchor: Element | null = null;
  // Identity of the open hover: CodeMirror rebuilds decoration spans on any
  // editor update, so the anchor ELEMENT goes stale while the pointer never
  // moved — track the note name and the anchor's rect instead.
  let currentName: string | null = null;
  let anchorRect: DOMRect | null = null;
  let hoverWindowId: string | null = null;
  let showTimer: number | undefined;
  let hideTimer: number | undefined;
  let fetchSeq = 0;

  const clearTimers = () => {
    window.clearTimeout(showTimer);
    window.clearTimeout(hideTimer);
  };

  const hoverWindow = () =>
    hoverWindowId ? windowStore.floatingWindows.find((w) => w.id === hoverWindowId) : undefined;

  /** Close the transient window (if still transient) and drop all state. */
  const dismiss = () => {
    currentAnchor = null;
    currentName = null;
    anchorRect = null;
    setCard(null);
    const win = hoverWindow();
    hoverWindowId = null;
    if (win?.transient) {
      windowActions.closeFloatingWindow(win.id);
    }
  };

  const spawnHoverWindow = (anchor: Element, preview: NotePreview) => {
    // Backlinks rows ask the preview to scroll to the wikilink that points
    // back at their focused note (data-scroll-note = that note's key,
    // data-scroll-line = the exact referencing line when known).
    const scrollToNote = anchor.getAttribute('data-scroll-note') || undefined;
    const scrollLineRaw = Number(anchor.getAttribute('data-scroll-line') ?? NaN);
    const scrollToLine = Number.isFinite(scrollLineRaw) && scrollLineRaw > 0 ? scrollLineRaw : undefined;
    // Already showing this note (or the user pinned one for it)? Done.
    const open = windowStore.floatingWindows.find((fw) =>
      windowStore.tabGroups[fw.tabGroupId]?.tabs.some(
        (t) => t.metadata?.filePath === preview.absPath,
      ),
    );
    if (open) {
      if (open.transient) hoverWindowId = open.id;
      return;
    }

    const rect = anchor.getBoundingClientRect();
    const left = Math.max(
      EDGE_GAP_PX,
      Math.min(rect.left, window.innerWidth - WINDOW_WIDTH - EDGE_GAP_PX),
    );
    // Hug the anchor: a taller gap exposed the next link in a list, so the
    // pointer crossed it (and spawned ITS preview) en route to the window.
    const fitsBelow = rect.bottom + WINDOW_HEIGHT + EDGE_GAP_PX <= window.innerHeight;
    const top = fitsBelow
      ? rect.bottom + 2
      : Math.max(EDGE_GAP_PX, rect.top - WINDOW_HEIGHT - 2);

    const groupId = windowActions.createTabGroup();
    windowActions.addTab(groupId, {
      // Distinct id from center-pane file tabs: hovering a note that's open
      // elsewhere must not be treated as "the same tab in two groups".
      id: `tab-hoverfile-${preview.absPath}`,
      title: preview.title,
      contentType: 'file',
      icon: iconForContentType('file'),
      // Popovers open in the configured hover mode (default: the fully
      // rendered reading view, like Obsidian's page preview). background:
      // a hover must never claim activeFile, or focus-following panels
      // (backlinks) retarget on every hover.
      metadata: {
        filePath: preview.absPath,
        initialMode: settings.editor.hoverMode,
        background: true,
        ...(scrollToNote ? { scrollToNote } : {}),
        ...(scrollToLine ? { scrollToLine } : {}),
      },
    });
    hoverWindowId = windowActions.createFloatingWindow(
      groupId,
      left,
      top,
      WINDOW_WIDTH,
      WINDOW_HEIGHT,
      { transient: true, showTabBar: false, title: preview.title },
    );
  };

  const show = async (anchor: Element, name: string) => {
    currentAnchor = anchor;
    currentName = name;
    const rect = anchor.getBoundingClientRect();
    anchorRect = rect;
    setCardPos({ left: rect.left, top: rect.bottom + 6 });
    setCard({ kind: 'loading' });

    const seq = ++fetchSeq;
    const kiln = anchor.getAttribute('data-kiln') ?? statusBarStore.kilnPath() ?? undefined;
    const preview = await fetchNotePreview(name, kiln);
    // A newer hover superseded this fetch, or the hover was dismissed.
    if (seq !== fetchSeq || currentAnchor !== anchor) return;
    if (!preview) {
      setCard({ kind: 'missing', name });
      return;
    }
    setCard(null);
    spawnHoverWindow(anchor, preview);
  };

  const onMouseOver = (event: MouseEvent) => {
    const target = event.target as Element | null;
    if (!target) return;

    // Pointer over the hover window keeps it open — and cancels any pending
    // show from a link the pointer crossed on its way in (that stray timer
    // used to swap the preview to whatever sat under the transit path).
    const winId = hoverWindowId;
    if (winId && target.closest?.(`[data-window-id="${winId}"]`)) {
      clearTimers();
      return;
    }

    // Transit corridor: with a window open, the strip between the anchor and
    // the window's near edge belongs to the hover — links inside it must not
    // trigger while the pointer is merely traveling to the window.
    const win = hoverWindow();
    if (win && anchorRect) {
      const withinX =
        event.clientX >= Math.min(win.x, anchorRect.left) - 8 &&
        event.clientX <= Math.max(win.x + win.width, anchorRect.right) + 8;
      const belowPath =
        win.y >= anchorRect.bottom &&
        event.clientY >= anchorRect.top &&
        event.clientY <= win.y + 4;
      const abovePath =
        win.y + win.height <= anchorRect.top &&
        event.clientY <= anchorRect.bottom &&
        event.clientY >= win.y + win.height - 4;
      if (withinX && (belowPath || abovePath)) {
        clearTimers();
        return;
      }
    }

    const anchor = target.closest?.('[data-note]');
    if (anchor) {
      const name = anchor.getAttribute('data-note');
      if (!name) return;
      window.clearTimeout(hideTimer);
      // Same note (possibly a rebuilt decoration span): just refresh the
      // tracked anchor, the popover stays.
      if (anchor === currentAnchor || name === currentName) {
        currentAnchor = anchor;
        currentName = name;
        anchorRect = anchor.getBoundingClientRect();
        return;
      }
      window.clearTimeout(showTimer);
      // Hovering a different link: retire the previous popover first.
      if (hoverWindowId || card()) {
        hideTimer = window.setTimeout(dismiss, HIDE_DELAY_MS);
      }
      showTimer = window.setTimeout(() => void show(anchor, name), SHOW_DELAY_MS);
      return;
    }

    // A DOM change under a STATIONARY pointer (loading overlays, CodeMirror
    // redecoration) re-fires mouseover with a non-anchor target at the same
    // coordinates — that is not "hovering away". Only pointer positions
    // outside the anchor's rect count as leaving.
    if (
      anchorRect &&
      event.clientX >= anchorRect.left - 8 &&
      event.clientX <= anchorRect.right + 8 &&
      event.clientY >= anchorRect.top - 8 &&
      event.clientY <= anchorRect.bottom + 8
    ) {
      return;
    }

    // Hovering anything else: cancel a pending show, schedule dismissal.
    window.clearTimeout(showTimer);
    if (currentAnchor || hoverWindowId) {
      window.clearTimeout(hideTimer);
      hideTimer = window.setTimeout(() => {
        // Pinned (or already closed) windows are no longer ours to manage.
        const win = hoverWindow();
        if (win && !win.transient) {
          hoverWindowId = null;
          currentAnchor = null;
          currentName = null;
          anchorRect = null;
          setCard(null);
          return;
        }
        dismiss();
      }, HIDE_DELAY_MS);
    }
  };

  onMount(() => {
    document.addEventListener('mouseover', onMouseOver);
  });

  onCleanup(() => {
    document.removeEventListener('mouseover', onMouseOver);
    clearTimers();
  });

  return (
    <Show when={card()} keyed>
      {(s) => (
        <div
          data-testid="wikilink-preview"
          class="fixed z-[80] rounded-md border border-hairline bg-surface-overlay px-3 py-2 text-xs text-muted shadow-lg cru-anim-rise"
          style={{ left: `${cardPos().left}px`, top: `${cardPos().top}px` }}
        >
          <Show when={s.kind === 'missing' && s} fallback={<>Loading preview…</>}>
            {(missing) => (
              <span data-testid="wikilink-preview-missing">
                Note not found: {(missing() as { kind: 'missing'; name: string }).name}
              </span>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
};
