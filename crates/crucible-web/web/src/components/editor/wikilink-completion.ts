/**
 * `[[note]]` completion for markdown buffers.
 *
 * The editor could already decorate and follow wikilinks, but offered no way to
 * *write* one without knowing the note's exact title — you had to leave the
 * buffer and look it up. This closes that loop with the same note list the chat
 * composer completes against.
 */
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from '@codemirror/autocomplete';
import type { Extension } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { listKilnNotes } from '@/lib/api';
import { inCodeContext } from './md-context';

/** Note lists are stable for a buffer's lifetime; one fetch per kiln. */
const noteCache = new Map<string, Promise<{ name: string; path: string }[]>>();

function notesFor(kiln: string) {
  let pending = noteCache.get(kiln);
  if (!pending) {
    // A rejected fetch must not be cached — a later keystroke retries.
    pending = listKilnNotes(kiln).catch((err) => {
      noteCache.delete(kiln);
      throw err;
    });
    noteCache.set(kiln, pending);
  }
  return pending;
}

/** Test seam: drop memoised note lists. */
export function resetWikilinkNoteCache(): void {
  noteCache.clear();
}

/**
 * Replace the typed query with the note name and close the link.
 *
 * Written as an `apply` rather than a plain label so the trailing `]]` is
 * added exactly once — the user may have typed it already.
 */
function applyNote(name: string) {
  return (view: EditorView, _completion: Completion, from: number, to: number) => {
    const alreadyClosed = view.state.sliceDoc(to, to + 2) === ']]';
    const insert = alreadyClosed ? name : `${name}]]`;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + insert.length + (alreadyClosed ? 2 : 0) },
    });
  };
}

/**
 * Completion source for `[[`. Exported for tests; use {@link wikilinkCompletion}
 * to build the editor extension.
 */
export function wikilinkCompletionSource(
  kiln: () => string | undefined,
): CompletionSource {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    // `[[` plus anything that isn't a closing bracket or a line break — so a
    // finished `[[Tags]]` stops matching and prose never triggers.
    const token = context.matchBefore(/\[\[[^\]\n]*/);
    if (!token) return null;

    const kilnPath = kiln();
    if (!kilnPath) return null;

    // `[[mcp.upstreams]]` in a fenced TOML block is code, not a knowledge link
    // — same rule the wikilink decorations follow.
    if (inCodeContext(context.state, token.from)) return null;

    let notes: { name: string; path: string }[];
    try {
      notes = await notesFor(kilnPath);
    } catch {
      return null;
    }

    return {
      // Past the `[[`, so accepting replaces only the query.
      from: token.from + 2,
      options: notes.map((note) => ({
        label: note.name,
        // Two notes can share a title across folders; the path disambiguates.
        detail: note.path,
        apply: applyNote(note.name),
      })),
      // Keep filtering as the user types instead of re-running the source,
      // and drop out the moment a `]` or newline arrives.
      validFor: /^[^\]\n]*$/,
    };
  };
}

/** Editor extension: `[[` completion against `kiln`'s notes. */
export function wikilinkCompletion(kiln: () => string | undefined): Extension {
  return autocompletion({
    override: [wikilinkCompletionSource(kiln)],
    // The list should appear on `[[` without an explicit Ctrl-Space.
    activateOnTyping: true,
    icons: false,
  });
}
