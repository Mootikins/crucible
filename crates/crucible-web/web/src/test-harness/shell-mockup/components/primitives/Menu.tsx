/**
 * A small menu under (or over) the button that opens it. The menu floats in
 * a portal, so a card with `overflow: hidden` does not clip it. Escape, a
 * click outside it, or a choice closes it. The real app uses the Ark UI
 * `Menu` of `components/ui/`.
 */
import { For, Show, createSignal, type Component, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Check } from 'lucide-solid';

/**
 * One entry: an action, a choice that can show a check, or a line. `hint`
 * is quiet text after the label; `bar` is a colour that marks the entry.
 */
export type MenuEntry =
  | { kind: 'item'; label: string; onSelect: () => void; checked?: boolean; hint?: string; bar?: string }
  | { kind: 'sep' };

export interface MenuProps {
  /** The accessible name and the tooltip of the button. */
  label: string;
  /** The classes of the button. The default is the icon button. */
  triggerClass?: string;
  trigger: JSX.Element;
  entries: readonly MenuEntry[];
  /** The menu edge that lines up with the button: its start or its end. */
  align?: 'start' | 'end';
  /** Open above the button, for a button near the bottom of a pane. */
  up?: boolean;
}

export const Menu: Component<MenuProps> = (props) => {
  const [rect, setRect] = createSignal<DOMRect | null>(null);
  const close = () => setRect(null);
  const pick = (run: () => void) => {
    close();
    run();
  };
  const place = (r: DOMRect): JSX.CSSProperties => ({
    ...(props.up ? { bottom: `${window.innerHeight - r.top + 4}px` } : { top: `${r.bottom + 4}px` }),
    ...(props.align === 'end' ? { right: `${window.innerWidth - r.right}px` } : { left: `${r.left}px` }),
  });
  // The menu has check marks only when one of its entries can show one.
  const hasChecks = () => props.entries.some((e) => e.kind === 'item' && e.checked !== undefined);
  return (
    <>
      <button
        type="button"
        class={props.triggerClass ?? 'mk-iconbtn'}
        title={props.label}
        aria-label={props.label}
        aria-haspopup="menu"
        aria-expanded={!!rect()}
        onClick={(e) => (rect() ? close() : setRect(e.currentTarget.getBoundingClientRect()))}
      >
        {props.trigger}
      </button>
      <Show when={rect()}>
        {(r) => (
          <Portal>
            <div class="mk-scrim" onMouseDown={close} />
            <div class="mk-menu" role="menu" style={place(r())} onKeyDown={(e) => e.key === 'Escape' && close()}>
              <For each={props.entries}>
                {(e) =>
                  e.kind === 'sep' ? (
                    <hr />
                  ) : (
                    <button
                      type="button"
                      role={e.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                      aria-checked={e.checked}
                      onClick={() => pick(e.onSelect)}
                    >
                      <Show when={hasChecks()}>
                        <span class="mk-hm-check">{e.checked ? <Check class="mk-i" /> : null}</span>
                      </Show>
                      <span class="mk-t">{e.label}</span>
                      <Show when={e.hint}>
                        <span class="mk-kind">{e.hint}</span>
                      </Show>
                      <Show when={e.bar}>
                        <span class="mk-grow" />
                        <span class="mk-sessionbar" style={{ background: e.bar }} />
                      </Show>
                    </button>
                  )
                }
              </For>
            </div>
          </Portal>
        )}
      </Show>
    </>
  );
};
