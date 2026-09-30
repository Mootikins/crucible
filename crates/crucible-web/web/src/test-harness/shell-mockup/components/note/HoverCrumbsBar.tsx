/**
 * A hover popup's bar, variant "crumbs": the path, pin, and one menu with
 * the view modes, ask, open in a tab and close.
 */
import { For, Show, createSignal, type Component } from 'solid-js';
import { Check, MoreHorizontal, Pin } from 'lucide-solid';
import { IconButton } from '../primitives/IconButton';
import { Breadcrumb } from './Breadcrumb';
import type { HoverActions } from './HoverTitleBar';
import type { NoteMode } from './NoteToolbar';

export interface HoverCrumbsBarProps extends HoverActions {
  path: string;
  mode: NoteMode;
  onMode: (mode: NoteMode) => void;
  onAsk: () => void;
  onOpenInTab: () => void;
}

const MODES: readonly (readonly [NoteMode, string])[] = [
  ['live', 'Live preview'],
  ['source', 'Source'],
  ['read', 'Reading view'],
];

export const HoverCrumbsBar: Component<HoverCrumbsBarProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const run = (action: () => void) => {
    setOpen(false);
    action();
  };
  return (
    <div class="mk-hoverbar" data-wm-drag-handle="">
      <Breadcrumb root="docs" path={props.path} />
      <span class="mk-grow" />
      <IconButton label="Pin: keep it open, with all its tools" onClick={() => props.onPin()}>
        <Pin class="mk-i" />
      </IconButton>
      <div class="mk-hovermenu-anchor">
        <IconButton label="More" pressed={open()} onClick={() => setOpen((o) => !o)}>
          <MoreHorizontal class="mk-i" />
        </IconButton>
        <Show when={open()}>
          <div class="mk-hovermenu" role="menu">
            <For each={MODES}>
              {([mode, label]) => (
                <button type="button" role="menuitemradio" aria-checked={props.mode === mode} onClick={() => run(() => props.onMode(mode))}>
                  <span class="mk-hm-check">{props.mode === mode ? <Check class="mk-i" /> : null}</span>
                  {label}
                </button>
              )}
            </For>
            <hr />
            <button type="button" role="menuitem" onClick={() => run(props.onAsk)}><span class="mk-hm-check" />Ask the session</button>
            <button type="button" role="menuitem" onClick={() => run(props.onOpenInTab)}><span class="mk-hm-check" />Open in a tab</button>
            <button type="button" role="menuitem" onClick={() => run(props.onClose)}><span class="mk-hm-check" />Close</button>
          </div>
        </Show>
      </div>
    </div>
  );
};
