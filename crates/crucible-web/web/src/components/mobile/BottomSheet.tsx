import { Component, JSX, Show, createEffect, on, onCleanup } from 'solid-js';
import { Dynamic, Portal } from 'solid-js/web';
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
      {/* Portalled, because `fixed` is relative to the nearest TRANSFORMED
          ancestor, and the drawer animates with `translateX`. A sheet opened
          from inside the drawer — the project switcher, the kiln picker —
          was clipped to the drawer's 320 px and left the rest of the screen
          un-scrimmed. The same sheet opened from the shell was full width,
          which is the contrast that gave it away. */}
      <Portal>
      <div
        data-testid="sheet-scrim"
        class="fixed inset-0 z-[60] bg-black/60"
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
        // A modal surface wears modal chrome: `rounded-t` is 3 px, which is
        // the popover radius this inherited from `menu-style`, and every
        // other modal in the app pairs a large radius with `shadow-2xl`.
        class="focus-ring fixed inset-x-0 bottom-0 z-[61] max-h-[75vh] overflow-y-auto rounded-t-2xl border-t border-hairline-strong bg-surface-elevated px-2 py-2 text-reading text-shell-ink shadow-2xl"
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
      </Portal>
    </Show>
  );
};

/** One choice in a sheet. 44 px, because a thumb picks it. */
export const SheetOption: Component<{
  label: string;
  detail?: string;
  selected?: boolean;
  /** The mark the desktop gives this row, so a menu reads the same on both. */
  icon?: Component<{ class?: string }>;
  onSelect: () => void;
}> = (props) => (
  <button
    type="button"
    aria-label={props.label}
    aria-pressed={props.selected}
    // `menuItem`'s vocabulary at a thumb's height: the shell's menu rows are
    // px-3/py-1.5; only the height and the focus ring differ.
    class={`${menuItem} w-full h-11 rounded text-left focus-ring ${
      props.selected ? 'bg-control text-shell-ink font-medium' : 'text-shell-body hover:bg-hover-wash'
    }`}
    onClick={() => props.onSelect()}
  >
    <Show when={props.icon}>
      <Dynamic component={props.icon!} class="w-4 h-4 shrink-0 text-muted-dark" />
    </Show>
    <span class="flex-1 truncate">{props.label}</span>
    <Show when={props.detail}>
      <span class="text-floor text-muted-dark shrink-0">{props.detail}</span>
    </Show>
  </button>
);
