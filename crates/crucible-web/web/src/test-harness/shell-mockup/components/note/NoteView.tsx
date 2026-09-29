/**
 * One note in a document tab: the toolbar, then the note in live, source or
 * reading view. Alt+Left/Right and the mouse's back and forward buttons walk
 * the tab's history, as in a web browser.
 */
import { For, Show, createMemo, createSignal, type Component, type JSX } from 'solid-js';
import type { WikilinkEvents } from '../primitives/wikilinks';
import type { HistoryNavProps } from './HistoryNav';
import { NoteArticle } from './NoteArticle';
import { NoteMissing } from './NoteMissing';
import { NoteToolbar, type NoteMode } from './NoteToolbar';
import { parseFront } from './parse';

export interface NoteViewProps {
  path: string;
  /** The note's text (EditorContext and `useGetFileContent` in the real app). None: the text is not loaded. */
  source?: string;
  /** Only a note in a tab has a history. */
  history?: HistoryNavProps;
  links: WikilinkEvents;
  renderHunk: (id: string) => JSX.Element;
  /** Hunks to show under a note whose text is not loaded. */
  orphanHunks: string[];
  onAsk: () => void;
  /** See `NoteToolbarProps.windowControls`. */
  windowControls?: JSX.Element;
}

export const NoteView: Component<NoteViewProps> = (props) => {
  const [mode, setMode] = createSignal<NoteMode>('live');
  const parsed = createMemo(() => (props.source ? parseFront(props.source) : null));
  const go = (step: -1 | 1) => props.history?.onGo(step);
  return (
    <div
      class="mk-body"
      onKeyDown={(e) => {
        if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
          e.preventDefault();
          go(e.key === 'ArrowLeft' ? -1 : 1);
        }
      }}
      onMouseUp={(e) => {
        if (e.button === 3 || e.button === 4) go(e.button === 3 ? -1 : 1);
      }}
    >
      <NoteToolbar
        path={props.path}
        history={props.history}
        mode={mode()}
        onMode={setMode}
        onAsk={props.onAsk}
        windowControls={props.windowControls}
      />
      <div class="mk-scroll">
        <Show
          when={parsed()}
          fallback={
            <NoteMissing path={props.path}>
              <For each={props.orphanHunks}>{(id) => props.renderHunk(id)}</For>
            </NoteMissing>
          }
        >
          {(p) => (
            <Show when={mode() !== 'source'} fallback={<pre class="mk-source">{props.source}</pre>}>
              <NoteArticle props={p()[0]} body={p()[1]} links={props.links} renderHunk={props.renderHunk} />
            </Show>
          )}
        </Show>
      </div>
    </div>
  );
};
