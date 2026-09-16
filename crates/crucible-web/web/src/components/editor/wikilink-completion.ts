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
import { fetchKilnNotesOnce } from '@/lib/query/notes';
import { inCodeContext } from './md-context';

interface Note {
  name: string;
  path: string;
}

/** `Help/Index.md` → `Help/Index` — the link target, not the filename. */
function linkTarget(path: string): string {
  return path.replace(/\.md$/i, '');
}

/**
 * The shortest target that unambiguously identifies each note.
 *
 * A bare title is what people write by hand, so it stays the default — but two
 * notes can share a title across folders, and then a bare `[[Index]]` resolves
 * to whichever the kiln picks first. Those get path-qualified; showing the
 * path only in the `detail` line would distinguish them on screen and not in
 * the document.
 */
function uniqueTargets(notes: Note[]): Map<Note, string> {
  const counts = new Map<string, number>();
  for (const note of notes) counts.set(note.name, (counts.get(note.name) ?? 0) + 1);
  return new Map(
    notes.map((note) => [note, counts.get(note.name)! > 1 ? linkTarget(note.path) : note.name]),
  );
}

/**
 * Replace the typed query with the link target and close the link.
 *
 * Written as an `apply` rather than a plain label for two reasons: the
 * trailing `]]` is added exactly once (the user may have typed it already),
 * and the inserted target is not always the option's label — see
 * {@link uniqueTargets}.
 */
function applyNote(target: string) {
  return (view: EditorView, _completion: Completion, from: number, to: number) => {
    const alreadyClosed = view.state.sliceDoc(to, to + 2) === ']]';
    const insert = alreadyClosed ? target : `${target}]]`;
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

    let notes: Note[];
    try {
      notes = await fetchKilnNotesOnce(kilnPath);
    } catch {
      return null;
    }

    const targets = uniqueTargets(notes);
    return {
      // Past the `[[`, so accepting replaces only the query.
      from: token.from + 2,
      options: notes.map((note) => ({
        label: note.name,
        // Two notes can share a title across folders; the path disambiguates.
        detail: note.path,
        apply: applyNote(targets.get(note)!),
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
