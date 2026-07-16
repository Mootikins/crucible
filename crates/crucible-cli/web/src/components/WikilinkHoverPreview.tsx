/**
 * App-wide wikilink hover previews.
 *
 * Mounted once; listens at the document level for hover on any element
 * carrying a `data-note` attribute (chat message anchors, editor
 * decorations, panel rows) and floats a note-preview card next to it.
 * One component makes every surface knowledge-native — no per-surface
 * wiring beyond emitting `data-note`.
 */
import { Component, Show, Switch, Match, createSignal, onMount, onCleanup } from 'solid-js';
import { fetchNotePreview, openNoteInEditor, type NotePreview } from '@/lib/note-actions';
import { renderMarkdown } from '@/lib/markdown';
import { statusBarStore } from '@/stores/statusBarStore';

const SHOW_DELAY_MS = 300;
const HIDE_DELAY_MS = 200;
const CARD_WIDTH_PX = 384; // w-96
const CARD_MAX_HEIGHT_PX = 288; // max-h-72
const EDGE_GAP_PX = 12;

type PreviewState =
  | { kind: 'loading' }
  | { kind: 'missing'; name: string }
  | { kind: 'ready'; name: string; preview: NotePreview };

export const WikilinkHoverPreview: Component = () => {
  const [state, setState] = createSignal<PreviewState | null>(null);
  const [position, setPosition] = createSignal<{ left: number; top?: number; bottom?: number }>({
    left: 0,
    top: 0,
  });

  let cardEl: HTMLDivElement | undefined;
  let currentAnchor: Element | null = null;
  let showTimer: number | undefined;
  let hideTimer: number | undefined;
  let fetchSeq = 0;

  const clearTimers = () => {
    window.clearTimeout(showTimer);
    window.clearTimeout(hideTimer);
  };

  const hide = () => {
    currentAnchor = null;
    setState(null);
  };

  const positionFor = (anchor: Element) => {
    const rect = anchor.getBoundingClientRect();
    const left = Math.max(
      EDGE_GAP_PX,
      Math.min(rect.left, window.innerWidth - CARD_WIDTH_PX - EDGE_GAP_PX),
    );
    const fitsBelow = rect.bottom + CARD_MAX_HEIGHT_PX + EDGE_GAP_PX <= window.innerHeight;
    return fitsBelow
      ? { left, top: rect.bottom + 6 }
      : { left, bottom: window.innerHeight - rect.top + 6 };
  };

  const show = async (anchor: Element, name: string) => {
    currentAnchor = anchor;
    setPosition(positionFor(anchor));
    setState({ kind: 'loading' });

    const seq = ++fetchSeq;
    const kiln = anchor.getAttribute('data-kiln') ?? statusBarStore.kilnPath() ?? undefined;
    const preview = await fetchNotePreview(name, kiln);
    // A newer hover superseded this fetch, or the card was dismissed.
    if (seq !== fetchSeq || currentAnchor !== anchor) return;
    setState(preview ? { kind: 'ready', name, preview } : { kind: 'missing', name });
  };

  const onMouseOver = (event: MouseEvent) => {
    const target = event.target as Element | null;
    if (!target) return;

    // Moving onto the card keeps it open.
    if (cardEl && cardEl.contains(target)) {
      window.clearTimeout(hideTimer);
      return;
    }

    const anchor = target.closest?.('[data-note]');
    if (anchor) {
      const name = anchor.getAttribute('data-note');
      if (!name) return;
      window.clearTimeout(hideTimer);
      if (anchor === currentAnchor) return;
      window.clearTimeout(showTimer);
      showTimer = window.setTimeout(() => void show(anchor, name), SHOW_DELAY_MS);
      return;
    }

    // Hovering anything else: cancel a pending show, schedule a hide.
    window.clearTimeout(showTimer);
    if (currentAnchor) {
      window.clearTimeout(hideTimer);
      hideTimer = window.setTimeout(hide, HIDE_DELAY_MS);
    }
  };

  onMount(() => {
    document.addEventListener('mouseover', onMouseOver);
  });

  onCleanup(() => {
    document.removeEventListener('mouseover', onMouseOver);
    clearTimers();
  });

  const openCurrent = () => {
    const s = state();
    if (s && s.kind === 'ready') {
      void openNoteInEditor(s.name, statusBarStore.kilnPath() ?? undefined);
      hide();
    }
  };

  return (
    <Show when={state()} keyed>
      {(s) => (
        <div
          ref={cardEl}
          data-testid="wikilink-preview"
          class="fixed z-[80] w-96 max-h-72 overflow-hidden rounded-lg border border-white/10 bg-surface-overlay shadow-xl"
          style={{
            left: `${position().left}px`,
            ...(position().top !== undefined ? { top: `${position().top}px` } : {}),
            ...(position().bottom !== undefined ? { bottom: `${position().bottom}px` } : {}),
          }}
        >
          <Switch>
            <Match when={s.kind === 'loading'}>
              <div class="px-3 py-2 text-xs text-muted">Loading preview…</div>
            </Match>
            <Match when={s.kind === 'missing' && s}>
              {(missing) => (
                <div class="px-3 py-2 text-xs text-muted" data-testid="wikilink-preview-missing">
                  Note not found: {(missing() as { kind: 'missing'; name: string }).name}
                </div>
              )}
            </Match>
            <Match when={s.kind === 'ready' && s}>
              {(readyState) => {
                const ready = readyState() as { kind: 'ready'; name: string; preview: NotePreview };
                return (
                  <>
                    <button
                      type="button"
                      class="block w-full border-b border-white/10 px-3 py-2 text-left hover:bg-white/5"
                      onClick={openCurrent}
                      data-testid="wikilink-preview-title"
                    >
                      <span class="text-sm font-medium text-shell-ink">{ready.preview.title}</span>
                      <span class="ml-2 truncate text-[11px] text-muted">{ready.preview.path}</span>
                    </button>
                    <div
                      class="prose prose-invert prose-sm max-w-none overflow-hidden px-3 py-2 text-[13px] leading-snug
                        prose-headings:my-1 prose-headings:text-sm prose-p:my-1 prose-a:text-primary
                        prose-ul:my-1 prose-ol:my-1 prose-li:my-0.5"
                      data-testid="wikilink-preview-body"
                      // eslint-disable-next-line solid/no-innerhtml
                      innerHTML={renderMarkdown(ready.preview.excerpt || '*Empty note*')}
                    />
                  </>
                );
              }}
            </Match>
          </Switch>
        </div>
      )}
    </Show>
  );
};
