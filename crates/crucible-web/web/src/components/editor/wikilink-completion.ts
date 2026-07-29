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

interface Note {
  name: string;
  path: string;
}

/**
 * How long a fetched note list is reused.
 *
 * The cache exists to coalesce the burst of requests a user generates while
 * typing a link, not to be a store — so it is deliberately short-lived. A
 * long-lived cache goes stale the moment a note is created, renamed or moved,
 * and the completion list then silently omits it until the page is reloaded.
 */
const NOTE_TTL_MS = 5_000;

/** Per-source cache — not module-global, so one editor can't serve another stale notes. */
function createNoteLoader(): (kiln: string) => Promise<Note[]> {
  let cached: { kiln: string; at: number; notes: Promise<Note[]> } | null = null;

  return (kiln: string) => {
    if (cached && cached.kiln === kiln && Date.now() - cached.at < NOTE_TTL_MS) {
      return cached.notes;
    }
    // A rejected fetch must not be cached — a later keystroke retries.
    const notes = listKilnNotes(kiln).catch((err) => {
      cached = null;
      throw err;
    });
    cached = { kiln, at: Date.now(), notes };
    return notes;
  };
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
  const loadNotes = createNoteLoader();

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
      notes = await loadNotes(kilnPath);
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
