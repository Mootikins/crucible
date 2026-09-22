/**
 * Resolve a note conflict, region by region, in the note itself.
 *
 * The caller gives the merged text and one region per span the two writers
 * changed differently. Two callers do: the outbox conflicts
 * (`ConflictsPanel.tsx`), and a conflicted proposal in the diff pane. This is
 * where a person settles it: the merged text is the document, every region
 * carries the other writer's version and three choices, and Save is refused
 * until none is left open. The view writes nothing itself. It gives the
 * settled text to `onSave`.
 *
 * It replaces the conflict copy — a second note under a dated name that held
 * the stale writing and left two files to reconcile by hand. The writing never
 * leaves the note it was made in, and nothing is written until a person has
 * chosen everywhere the two writers disagree. A whole-note region (a write
 * this device queued before it kept its base text) is a region like any other,
 * so there is no one-tap path over the other writer's text.
 */
import { Component, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { notificationActions } from '@/stores/notificationStore';
import type { MergeRegion } from '@/lib/types';
import { theme } from '@/lib/theme';
import { getLanguageExtension } from './editor/CodeMirrorEditor';
import { editorThemeExtension } from './editor/editor-theme';
import {
  conflictRegions,
  seedConflictRegions,
  type TrackedRegion,
} from './editor/conflict-regions';

export interface ConflictViewProps {
  /** The note whose conflict this settles. */
  path: string;
  /** Our text merged with theirs, as far as the merge got. The document. */
  mergedContent: string;
  /** Every span the two writers changed differently. */
  regions: MergeRegion[];
  /**
   * The identity of the conflict: the hash of the text it was merged against.
   * A new value is a new conflict, so the view builds its document again.
   */
  baseHash: string;
  /**
   * Write the settled text. The caller tells the person the outcome. A
   * rejection shows as an error, and the regions stay as they are.
   */
  onSave: (text: string) => Promise<void>;
  /** Told when the person leaves. Asked first when a region is still open. No Close button without it. */
  onClose?: () => void;
  /** The text of the save button. The default is "Save". */
  saveLabel?: string;
  /** The line under the name. The default speaks of the outbox note. */
  hint?: string;
}

export const ConflictView: Component<ConflictViewProps> = (props) => {
  const [host, setHost] = createSignal<HTMLDivElement>();
  const [open, setOpen] = createSignal<TrackedRegion[]>([]);
  const [total, setTotal] = createSignal(0);
  /** The regions are in the view. Until then nothing is settled, whatever the count says. */
  const [seeded, setSeeded] = createSignal(false);
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [at, setAt] = createSignal(0);
  let view: EditorView | undefined;
  /** The hash of the conflict the editor was built over, while it is built. */
  let builtFor: string | undefined;

  /**
   * Every region of the conflict ON SCREEN is chosen.
   *
   * The hash is part of the question: a conflict that moved on is a different
   * document with different regions, and a Save over the one the view still
   * holds would write it against the newer base and take the newest writer's
   * text with no question asked.
   */
  const settled = () => seeded() && open().length === 0 && props.baseHash === builtFor;
  const name = () => props.path.split('/').pop() ?? props.path;

  // The document is the MERGED text: this writer's text, with the other
  // writer's folded in everywhere the two did not collide.
  //
  // It is built again when the conflict moves: a resolution the daemon refused
  // comes back with a new base and new regions, and a document built over the
  // old one settles nothing about the new difference.
  createEffect(() => {
    const el = host();
    const hash = props.baseHash;
    if (!el) return;
    if (view && builtFor === hash) return;
    const merged = props.mergedContent;
    const regions = props.regions;
    view?.destroy();
    setSeeded(false);
    view = new EditorView({
      state: EditorState.create({
        doc: merged,
        extensions: [
          lineNumbers(),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          editorThemeExtension(theme()),
          getLanguageExtension(props.path) ?? [],
          conflictRegions({ onChange: setOpen }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) setText(update.state.doc.toString());
          }),
          EditorView.theme({ '&': { height: '100%' }, '.cm-scroller': { overflow: 'auto' } }),
        ],
      }),
      parent: el,
    });
    setText(merged);
    setTotal(regions.length);
    setAt(0);
    seedConflictRegions(view, regions);
    builtFor = hash;
    setSeeded(true);
  });

  onCleanup(() => {
    view?.destroy();
    view = undefined;
    builtFor = undefined;
  });

  /** Scroll to the next region still open, wrapping at the end. */
  const go = (step: number) => {
    const list = open();
    if (!view || list.length === 0) return;
    const next = (at() + step + list.length) % list.length;
    setAt(next);
    view.dispatch({ effects: EditorView.scrollIntoView(list[next].from, { y: 'center' }) });
  };

  const save = () => {
    if (busy() || !settled()) return;
    setBusy(true);
    void props
      .onSave(text())
      .catch((e: Error) => notificationActions.addNotification('error', e.message))
      .finally(() => setBusy(false));
  };

  /**
   * Leaving with a region open loses nothing — the conflict stays where it
   * waits — but it also settles nothing, and the writing is still only here.
   * The same primitive the review's reject uses, for the same reason.
   */
  const close = () => {
    if (
      !settled() &&
      !window.confirm(
        `Leave ${name()} without choosing?\n\n` +
          'Your writing stays as a conflict until you settle every region.',
      )
    ) {
      return;
    }
    props.onClose?.();
  };

  return (
    <div class="flex h-full min-h-0 flex-col" data-testid="conflict-view">
      <div class="shrink-0 border-b border-hairline px-3 py-2">
        <div class="flex items-center gap-2">
          <span class="min-w-0 flex-1 truncate text-xs font-mono text-shell-ink" title={props.path}>
            {name()}
          </span>
          <span class="shrink-0 text-floor text-muted-dark" data-testid="conflict-counter">
            {total() - open().length} of {total()} {total() === 1 ? 'region' : 'regions'} resolved
          </span>
        </div>
        <p class="mt-1 text-floor text-muted-dark">
          {props.hint ??
            'Your text is in the note. Choose what to keep where the other writer said something else.'}
        </p>
        <div class="mt-1.5 flex items-center gap-1.5">
          <button
            type="button"
            title="Scroll to the previous region still open"
            data-testid="conflict-prev"
            disabled={open().length === 0}
            onClick={() => go(-1)}
            class="min-w-11 min-h-11 rounded border border-hairline px-2 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50"
          >
            Previous
          </button>
          <button
            type="button"
            title="Scroll to the next region still open"
            data-testid="conflict-next"
            disabled={open().length === 0}
            onClick={() => go(1)}
            class="min-w-11 min-h-11 rounded border border-hairline px-2 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50"
          >
            Next
          </button>
          <span class="ml-auto" />
          <Show when={props.onClose}>
            <button
              type="button"
              title="Leave this conflict where it is"
              data-testid="conflict-close"
              onClick={close}
              class="min-w-11 min-h-11 rounded border border-hairline px-2 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash"
            >
              Close
            </button>
          </Show>
          {/* Every region must be settled first: a Save that wrote over an
              unchosen region would be the silent overwrite this whole surface
              exists to refuse. */}
          <button
            type="button"
            title="Write the settled note"
            data-testid="conflict-save"
            disabled={busy() || !settled()}
            onClick={save}
            class="min-w-11 min-h-11 rounded border border-hairline px-2 text-floor text-shell-ink hover:bg-hover-wash disabled:opacity-50"
          >
            {props.saveLabel ?? 'Save'}
          </button>
        </div>
      </div>

      <div class="min-h-0 flex-1 overflow-hidden text-xs" ref={setHost} />
    </div>
  );
};
