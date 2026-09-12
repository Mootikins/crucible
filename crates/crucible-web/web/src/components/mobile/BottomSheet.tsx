import { Component, JSX, Show, createEffect, on, onCleanup } from 'solid-js';
import { navStack } from '@/components/mobile/NavStack';
import { menuItem } from '@/components/ui/menu-style';

/**
 * A sheet from the bottom edge: the phone's menu, its picker and its dialog.
 *
 * It mounts only while open, unlike a drawer. A drawer keeps a panel's scroll
 * and loaded data between visits; a sheet asks one question and goes.
 *
 * It closes on a scrim tap, on Escape, and on the hardware back button — the
 * last is the one a phone actually has.
 */
export const BottomSheet: Component<{
  open: boolean;
  label: string;
  onClose: () => void;
  children: JSX.Element;
}> = (props) => {
  let panel: HTMLDivElement | undefined;
  let release: (() => void) | null = null;

  createEffect(
    on(
      () => props.open,
      (open, wasOpen) => {
        if (open && !wasOpen) {
          release = navStack().push(() => props.onClose());
          queueMicrotask(() => panel?.focus());
        } else if (!open && wasOpen) {
          release?.();
          release = null;
        }
      },
    ),
  );
  onCleanup(() => release?.());

  return (
    <Show when={props.open}>
      <div
        data-testid="sheet-scrim"
        class="fixed inset-0 z-[60] bg-black/40"
        onClick={() => props.onClose()}
        aria-hidden="true"
      />
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-label={props.label}
        tabIndex={-1}
        data-testid="bottom-sheet"
        class="focus-ring fixed inset-x-0 bottom-0 z-[61] max-h-[75vh] overflow-y-auto rounded-t border-t border-hairline-strong bg-surface-elevated px-1 py-1 text-xs text-shell-ink shadow-md outline-none"
        style={{ 'padding-bottom': 'var(--inset-bottom)' }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            props.onClose();
          }
        }}
      >
        <div class="flex justify-center py-2" aria-hidden="true">
          <span class="h-1 w-10 rounded-full bg-hairline-strong" />
        </div>
        {props.children}
      </div>
    </Show>
  );
};

/** One choice in a sheet. 44 px, because a thumb picks it. */
export const SheetOption: Component<{
  label: string;
  detail?: string;
  selected?: boolean;
  onSelect: () => void;
}> = (props) => (
  <button
    type="button"
    aria-label={props.label}
    aria-pressed={props.selected}
    // `menuItem`'s vocabulary at a thumb's height: the shell's menu rows are
    // px-3/py-1.5; only the height and the focus ring differ.
    class={`${menuItem} w-full h-11 px-3 rounded text-left focus-ring ${
      props.selected ? 'bg-control text-shell-ink font-medium' : 'text-shell-body hover:bg-hover-wash'
    }`}
    onClick={() => props.onSelect()}
  >
    <span class="flex-1 truncate">{props.label}</span>
    <Show when={props.detail}>
      <span class="text-floor text-muted-dark shrink-0">{props.detail}</span>
    </Show>
  </button>
);
