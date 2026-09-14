/**
 * Resolve a note conflict, region by region, in the note itself.
 *
 * A write the daemon could neither take nor merge waits in the outbox with the
 * merged text and one region per span the two writers changed differently
 * (`lib/offline/outbox.ts`). This is where a person settles it: the merged text
 * is the document, every region carries the other writer's version and three
 * choices, and Save is refused until none is left open.
 *
 * It replaces the conflict copy — a second note under a dated name that held
 * the stale writing and left two files to reconcile by hand. The writing never
 * leaves the note it was made in, and nothing is written until a person has
 * chosen everywhere the two writers disagree. A whole-note region (a write
 * this device queued before it kept its base text) is a region like any other,
 * so there is no one-tap path over the other writer's text.
 */
import { Component, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { conflictActions, conflictStore } from '@/lib/conflicts';
import { notificationActions } from '@/stores/notificationStore';
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
  /** Told when the resolution is safe with the daemon, or queued for it. */
  onResolved?: () => void;
  /** Told when the person leaves. Asked first when a region is still open. */
  onClose?: () => void;
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

  const conflict = () => conflictStore.get(props.path);
  /**
   * Every region of the conflict ON SCREEN is chosen.
   *
   * The hash is part of the question: a conflict that moved on is a different
   * document with different regions, and a Save over the one the view still
   * holds would write it against the newer base and take the newest writer's
   * text with no question asked.
   */
  const settled = () =>
    seeded() && open().length === 0 && conflict()?.currentHash === builtFor;
  const name = () => props.path.split('/').pop() ?? props.path;

  onMount(() => {
    void conflictActions
      .refresh()
      .catch((e: Error) => notificationActions.addNotification('error', e.message));
  });

  // The conflict arrives from the outbox, so the editor is built when it does.
  // The document is the MERGED text: this device's writing, with the other
  // writer's folded in everywhere the two did not collide.
  //
  // It is built again when the conflict moves: a resolution the daemon refused
  // comes back as a new entry, with a new base and new regions, and a document
  // built over the old one settles nothing about the new difference.
  createEffect(() => {
    const el = host();
    const row = conflict();
    if (!el || !row) return;
    if (view && builtFor === row.currentHash) return;
    view?.destroy();
    setSeeded(false);
    view = new EditorView({
      state: EditorState.create({
        doc: row.mergedContent,
        extensions: [
          lineNumbers(),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          editorThemeExtension(theme()),
          getLanguageExtension(row.path) ?? [],
          conflictRegions({ onChange: setOpen }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) setText(update.state.doc.toString());
          }),
          EditorView.theme({ '&': { height: '100%' }, '.cm-scroller': { overflow: 'auto' } }),
        ],
      }),
      parent: el,
    });
    setText(row.mergedContent);
    setTotal(row.regions.length);
    setAt(0);
    seedConflictRegions(view, row.regions);
    builtFor = row.currentHash;
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
    void conflictActions
      .resolve(props.path, text())
      .then((outcome) => {
        if (outcome.queued) {
          notificationActions.addNotification(
            'info',
            `${name()} is settled. It goes out when the daemon answers.`,
          );
          props.onResolved?.();
          return;
        }
        if (outcome.stale) {
          // The note moved AGAIN between the merge and the choice. Nothing is
          // settled, so the conflict stays where it is.
          notificationActions.addNotification(
            'warning',
            `${name()} changed again while you were choosing. Open it again to settle the new difference.`,
          );
          return;
        }
        notificationActions.addNotification('info', `Resolved ${name()}.`);
        props.onResolved?.();
      })
      .catch((e: Error) => notificationActions.addNotification('error', e.message))
      .finally(() => setBusy(false));
  };

  /**
   * Leaving with a region open loses nothing — the conflict stays in the
   * outbox — but it also settles nothing, and the writing is still only here.
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
          Your text is in the note. Choose what to keep where the other writer said something else.
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
          <button
            type="button"
            title="Leave this conflict where it is"
            data-testid="conflict-close"
            onClick={close}
            class="ml-auto min-w-11 min-h-11 rounded border border-hairline px-2 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash"
          >
            Close
          </button>
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
            Save
          </button>
        </div>
      </div>

      <Show
        when={conflict()}
        fallback={
          <p class="p-3 text-xs text-muted-dark" data-testid="conflict-missing">
            Nothing waits for this note.
          </p>
        }
      >
        <div class="min-h-0 flex-1 overflow-hidden text-xs" ref={setHost} />
      </Show>
    </div>
  );
};
