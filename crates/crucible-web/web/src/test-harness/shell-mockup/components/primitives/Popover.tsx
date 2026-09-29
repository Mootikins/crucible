/**
 * A popover beside the button that opened it. It opens to the right of the
 * anchor and grows up from the anchor's bottom edge. Escape or a click
 * outside it closes it.
 */
import { Show, type Component, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';

export interface PopoverProps {
  open: boolean;
  /** The rectangle of the button that opened the popover. */
  anchor: DOMRect | null;
  onClose: () => void;
  children: JSX.Element;
}

export const Popover: Component<PopoverProps> = (props) => (
  <Show when={props.open && props.anchor}>
    {(r) => (
      <Portal>
        <div class="mk-scrim" onMouseDown={() => props.onClose()} />
        <div
          class="mk-pop"
          role="dialog"
          style={{ left: `${r().right + 6}px`, bottom: `${Math.max(8, window.innerHeight - r().bottom)}px` }}
          onKeyDown={(e) => e.key === 'Escape' && props.onClose()}
        >
          {props.children}
        </div>
      </Portal>
    )}
  </Show>
);
