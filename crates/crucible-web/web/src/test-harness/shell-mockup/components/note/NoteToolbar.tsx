/**
 * The bar over a note: history, path, the view switch and the button that
 * sends the reader to the session. A note outside a tab (a peek or a hover
 * editor) has no history, so it shows no history buttons.
 */
import { Show, type Component, type JSX } from 'solid-js';
import { BookOpen, Code, MessageSquare, Pencil } from 'lucide-solid';
import { IconButton } from '../primitives/IconButton';
import { ToggleGroup, type ToggleOption } from '../primitives/ToggleGroup';
import { Breadcrumb } from './Breadcrumb';
import { HistoryNav, type HistoryNavProps } from './HistoryNav';

export type NoteMode = 'live' | 'source' | 'read';

const MODES: readonly ToggleOption<NoteMode>[] = [
  { value: 'live', title: 'Live preview', icon: Pencil },
  { value: 'source', title: 'Source', icon: Code },
  { value: 'read', title: 'Reading view', icon: BookOpen },
];

export interface NoteToolbarProps {
  path: string;
  history?: HistoryNavProps;
  mode: NoteMode;
  onMode: (mode: NoteMode) => void;
  onAsk: () => void;
  /**
   * The controls of the window around a note that has no tab bar (a peek or
   * a hover editor). The bar then also drags the window.
   */
  windowControls?: JSX.Element;
}

export const NoteToolbar: Component<NoteToolbarProps> = (props) => (
  <div class="mk-crumbbar" data-wm-drag-handle={props.windowControls ? '' : undefined}>
    <Show when={props.history}>
      {(h) => <HistoryNav canBack={h().canBack} canForward={h().canForward} onGo={h().onGo} />}
    </Show>
    <Breadcrumb root="docs" path={props.path} />
    <ToggleGroup label="View" value={props.mode} options={MODES} onChange={props.onMode} />
    <IconButton label="Ask the session about this note" onClick={() => props.onAsk()}>
      <MessageSquare class="mk-i" />
    </IconButton>
    {props.windowControls}
  </div>
);
