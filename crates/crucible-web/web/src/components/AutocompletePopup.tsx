import { Component, For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import type { AutocompleteItem } from '@/hooks/useAutocomplete';
import { placePopup, type PopupPlacement } from '@/lib/popup-placement';

interface AutocompletePopupProps {
  items: AutocompleteItem[];
  selectedIndex: number;
  onSelect: (index: number) => void;
  /** Element the list aligns to — normally the composer textarea. */
  anchor?: HTMLElement;
}

export const AutocompletePopup: Component<AutocompletePopupProps> = (props) => {
  let listRef: HTMLDivElement | undefined;
  const [placement, setPlacement] = createSignal<PopupPlacement | null>(null);

  const reposition = () => {
    const anchor = props.anchor;
    if (!anchor) {
      setPlacement(null);
      return;
    }
    setPlacement(
      placePopup(anchor.getBoundingClientRect(), {
        width: window.innerWidth,
        height: window.innerHeight,
      }),
    );
  };

  // Recompute when the anchor or the item count changes — a list that grows or
  // shrinks can cross the flip threshold.
  createEffect(() => {
    props.items.length;
    props.anchor;
    reposition();
  });

  // `true` (capture) so scrolling ANY ancestor — the transcript, a split pane —
  // moves the popup with its anchor; scroll events don't bubble.
  window.addEventListener('scroll', reposition, true);
  window.addEventListener('resize', reposition);
  onCleanup(() => {
    window.removeEventListener('scroll', reposition, true);
    window.removeEventListener('resize', reposition);
  });

  // Keep the keyboard-selected row visible when the list overflows. Query the
  // selected option from the DOM rather than a positional index→ref map: the
  // fuzzy re-sort reorders <For> rows by moving existing nodes without
  // re-running their ref callbacks, so a creation-order ref array points at
  // the wrong row after a re-sort. aria-selected always marks the live row.
  createEffect(() => {
    props.selectedIndex;
    props.items;
    listRef
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView({ block: 'nearest' });
  });

  const style = () => {
    const p = placement();
    if (!p) return { display: 'none' };
    return {
      position: 'fixed' as const,
      left: `${p.left}px`,
      width: `${p.width}px`,
      ...(p.direction === 'up' ? { bottom: `${p.bottom}px` } : { top: `${p.top}px` }),
      // No fallback: placement always sets a height, and a legitimate 0
      // (nothing fits either side) must render as 0 rather than be treated as
      // "unset" and replaced by a full-height panel back off the viewport.
      'max-height': `${p.maxHeight}px`,
      'z-index': '50',
    };
  };

  // Portalled to <body>: the chat composer sits at the bottom of an
  // `overflow-hidden` flex column, which clipped the list away entirely when it
  // rendered inline. Fixed positioning also escapes ancestor stacking contexts
  // (transformed panes, floating windows).
  return (
    <Portal>
      <div
        ref={listRef}
        role="listbox"
        data-testid="autocomplete-popup"
        data-direction={placement()?.direction}
        style={style()}
        class="overflow-y-auto rounded-lg border border-hairline bg-surface-elevated shadow-xl cru-anim-rise"
      >
        <For each={props.items}>
          {(item, index) => (
            <button
              type="button"
              role="option"
              aria-selected={index() === props.selectedIndex}
              // Keyboard selection reads as an ember tint (distinct from the
              // lighter hover wash) so it's clear what Enter will insert.
              class="w-full px-3 py-2 text-left text-sm text-shell-ink hover:bg-hover-wash transition-colors focus-visible:outline-none"
              classList={{ 'bg-primary/15': index() === props.selectedIndex }}
              onMouseDown={(e) => {
                e.preventDefault();
                props.onSelect(index());
              }}
            >
              <span class="block truncate">{item.label}</span>
              <Show when={item.detail}>
                <span class="block truncate text-xs text-muted-dark">{item.detail}</span>
              </Show>
            </button>
          )}
        </For>
      </div>
    </Portal>
  );
};
