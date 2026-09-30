import { Accessor, Setter, createSignal } from 'solid-js';
import { fetchKilnNotesOnce } from '@/lib/query/notes';
import { fetchDirOnce } from '@/lib/query/fs';
import { fetchSlashCommandsOnce } from '@/lib/query/commands';
import { fuzzyScore } from '@/lib/fuzzy';
import type { FileEntry } from '@/lib/types';
import type { SessionCommand } from '@/lib/slash-commands';

type TriggerType = '@' | '#' | '/' | '[[';

export interface AutocompleteItem {
  id: string;
  label: string;
  insertText: string;
  /** Secondary line in the popup — a command's description, say. */
  detail?: string;
}

interface TriggerMatch {
  trigger: TriggerType;
  start: number;
  query: string;
}

interface UseAutocompleteOptions {
  input: Accessor<string>;
  setInput: Setter<string>;
  kilnPath: Accessor<string | null | undefined>;
  /**
   * The workspace of the session. `@` lists its files one folder at a time.
   * Absent (or null) leaves only the kiln files in the `@` list.
   */
  workspacePath?: Accessor<string | null | undefined>;
  /**
   * The session whose command catalog `/` lists. Absent (or null) leaves
   * `/` with nothing to list: a draft has no session and so no catalog yet.
   */
  sessionId?: Accessor<string | null | undefined>;
  textareaRef: Accessor<HTMLTextAreaElement | undefined>;
}

/** The source of a command that the daemon runs, for the popup's detail line. */
function commandSource(command: SessionCommand): string | null {
  switch (command.kind) {
    case 'builtin':
      return null;
    case 'mode':
    case 'plugin':
    case 'skill':
    case 'agent':
      return command.kind;
  }
}

/**
 * The session's commands as popup rows.
 *
 * The catalog itself is held by `lib/query/commands.ts`, one entry per
 * session. A refusal leaves no data, so the next keystroke asks again.
 */
function loadCommandItems(sessionId: string | null | undefined): Promise<AutocompleteItem[]> {
  if (!sessionId) return Promise.resolve([]);
  return fetchSlashCommandsOnce(sessionId).then((commands) =>
    commands.map((c) => {
      const source = commandSource(c);
      return {
        id: `command:${c.name}`,
        label: `/${c.name}`,
        // Commands taking an argument keep the trailing space so the user can
        // type straight into it; nullary ones don't (nothing follows).
        insertText: c.input_hint ? `${c.name} ` : c.name,
        detail: source ? `${c.description} (${source})` : c.description,
      };
    }),
  );
}

/**
 * Test seam: drop the held command list.
 *
 * Re-exported rather than moved, because every caller of it is a test of this
 * hook. The list is the query layer's; the seam stays where the tests reach it.
 */
export { resetCommandCache } from '@/lib/query/commands';

function toAutocompleteItems(entries: FileEntry[], prefix: string): AutocompleteItem[] {
  return entries.map((entry) => ({
    id: `${prefix}:${entry.path}`,
    label: entry.name,
    insertText: entry.name,
  }));
}

/**
 * The `@` rows of a kiln. The daemon resolves a mention under each root of the
 * session, so the row inserts the path under the kiln root, not the basename.
 */
function toMentionItems(entries: FileEntry[]): AutocompleteItem[] {
  return entries.map((entry) => ({
    id: `file:${entry.path}`,
    label: entry.path,
    insertText: entry.path,
  }));
}

/**
 * A line suffix at the end of an `@` query: `:12`, `:12-14`, or a part of one
 * that the user still types (`:`, `:12-`). The daemon attaches only those lines.
 */
const LINE_SUFFIX = /:\d*(?:-\d*)?$/;

/** Split an `@` query into the path to complete and its line suffix. */
function splitLineSuffix(query: string): { path: string; suffix: string } {
  const match = LINE_SUFFIX.exec(query);
  if (!match) return { path: query, suffix: '' };
  return { path: query.slice(0, match.index), suffix: match[0] };
}

function extractTagItems(entries: FileEntry[]): AutocompleteItem[] {
  const tags = new Set<string>();
  for (const entry of entries) {
    const raw = `${entry.name} ${entry.path}`;
    for (const token of raw.split(/[^a-zA-Z0-9_-]+/)) {
      const normalized = token.trim().toLowerCase();
      if (normalized.length >= 2) tags.add(normalized);
    }
  }
  return [...tags]
    .sort((a, b) => a.localeCompare(b))
    .map((tag) => ({
      id: `tag:${tag}`,
      label: `#${tag}`,
      insertText: tag,
    }));
}

export function fuzzyFilter(items: AutocompleteItem[], query: string): AutocompleteItem[] {
  if (!query.trim()) return items;
  // Rank with the same fuzzy scorer the command palette uses (subsequence match
  // + contiguity/word-boundary bonuses), not a naive substring includes, so the
  // note picker ranks results by relevance instead of raw daemon order.
  return items
    .map((item, index) => ({ item, index, score: fuzzyScore(item.label, query) }))
    .filter((r) => r.score !== null)
    .sort((a, b) => b.score! - a.score! || a.index - b.index)
    .map((r) => r.item);
}

function isWordTriggerBoundary(value: string, index: number): boolean {
  if (index <= 0) return true;
  return /\s/.test(value[index - 1]);
}

function detectTrigger(value: string, cursor: number): TriggerMatch | null {
  const beforeCursor = value.slice(0, cursor);
  const doubleBracketIndex = beforeCursor.lastIndexOf('[[');
  if (doubleBracketIndex >= 0) {
    const query = beforeCursor.slice(doubleBracketIndex + 2);
    if (!query.includes(']]')) {
      return { trigger: '[[', start: doubleBracketIndex, query };
    }
  }

  const candidates: Array<{ trigger: '@' | '#' | '/'; index: number }> = [
    { trigger: '@', index: beforeCursor.lastIndexOf('@') },
    { trigger: '#', index: beforeCursor.lastIndexOf('#') },
    { trigger: '/', index: beforeCursor.lastIndexOf('/') },
  ];
  candidates.sort((a, b) => b.index - a.index);

  for (const candidate of candidates) {
    if (candidate.index < 0) continue;
    if (!isWordTriggerBoundary(beforeCursor, candidate.index)) continue;
    const query = beforeCursor.slice(candidate.index + 1);
    if (/\s/.test(query)) continue;
    return { trigger: candidate.trigger, start: candidate.index, query };
  }

  return null;
}

export function useAutocomplete(options: UseAutocompleteOptions) {
  const [isOpen, setIsOpen] = createSignal(false);
  const [items, setItems] = createSignal<AutocompleteItem[]>([]);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [trigger, setTrigger] = createSignal<TriggerType | null>(null);
  const [triggerStart, setTriggerStart] = createSignal(0);
  const [cursorPosition, setCursorPosition] = createSignal(0);
  const [fileItems, setFileItems] = createSignal<AutocompleteItem[]>([]);
  const [noteItems, setNoteItems] = createSignal<AutocompleteItem[]>([]);
  const [tagItems, setTagItems] = createSignal<AutocompleteItem[]>([]);
  const [commandItems, setCommandItems] = createSignal<AutocompleteItem[]>([]);
  const [loadedKiln, setLoadedKiln] = createSignal<string | null>(null);
  const [workspaceItems, setWorkspaceItems] = createSignal<AutocompleteItem[]>([]);
  const [lineSuffix, setLineSuffix] = createSignal('');

  const close = () => {
    setIsOpen(false);
    setItems([]);
    setSelectedIndex(0);
    setTrigger(null);
  };

  const ensureKilnData = async () => {
    const kiln = options.kilnPath();
    if (!kiln) {
      setFileItems([]);
      setNoteItems([]);
      setTagItems([]);
      setLoadedKiln(null);
      return;
    }
    if (loadedKiln() === kiln) return;

    // `listFiles` and `listKilnNotes` both reshape one `list_notes` row now
    // ([[Simplification Plan#Step 19]] item 3) — the daemon answers the
    // same listing either way, so one fetch serves the `@` mentions, the
    // `[[` wikilinks and the tag extraction alike; a second call would only
    // ask the same question twice.
    const entries = await fetchKilnNotesOnce(kiln);
    const noteOptions = toAutocompleteItems(entries, 'note');
    setFileItems(toMentionItems(entries));
    setNoteItems(noteOptions);
    setTagItems(extractTagItems(entries));
    setLoadedKiln(kiln);
  };

  /**
   * The workspace folder that the `@` path names, as rows. The listing is one
   * level, so `@src/ma` lists `src` and the fuzzy filter picks from it.
   * A refused root lists nothing and shows no toast: the kiln rows still
   * complete.
   */
  const loadWorkspaceFolder = async (path: string) => {
    const root = options.workspacePath?.();
    if (!root) {
      setWorkspaceItems([]);
      return;
    }
    const cut = path.lastIndexOf('/');
    const folder = cut < 0 ? '' : path.slice(0, cut);
    try {
      const listing = await fetchDirOnce({ root, relPath: folder, notify: false });
      setWorkspaceItems(
        listing.entries.map((entry) => {
          const rel = entry.is_dir ? `${entry.rel_path}/` : entry.rel_path;
          return { id: `workspace:${rel}`, label: rel, insertText: rel };
        }),
      );
    } catch {
      setWorkspaceItems([]);
    }
  };

  const sourceItemsFor = (kind: TriggerType): AutocompleteItem[] => {
    if (kind === '@') {
      // One path can be in the workspace and in a kiln. The workspace row
      // wins, because the daemon tries the workspace first.
      const seen = new Set(workspaceItems().map((i) => i.insertText));
      return [...workspaceItems(), ...fileItems().filter((i) => !seen.has(i.insertText))];
    }
    if (kind === '#') return [...tagItems(), ...noteItems()];
    if (kind === '/') return commandItems();
    return noteItems();
  };

  const updateForValue = async (value: string, cursor: number) => {
    const match = detectTrigger(value, cursor);
    if (!match) {
      close();
      return;
    }

    setTrigger(match.trigger);
    setTriggerStart(match.start);
    setCursorPosition(cursor);

    const { path, suffix } =
      match.trigger === '@' ? splitLineSuffix(match.query) : { path: match.query, suffix: '' };
    setLineSuffix(suffix);

    try {
      if (match.trigger === '/') {
        setCommandItems(await loadCommandItems(options.sessionId?.()));
      } else if (match.trigger === '@') {
        await Promise.all([ensureKilnData(), loadWorkspaceFolder(path)]);
      } else {
        await ensureKilnData();
      }
    } catch {
      close();
      return;
    }

    const filtered = fuzzyFilter(sourceItemsFor(match.trigger), path);
    if (filtered.length === 0) {
      close();
      return;
    }

    setItems(filtered);
    setIsOpen(true);
    setSelectedIndex((prev) => Math.min(prev, filtered.length - 1));
  };

  const onInput = async (
    e: InputEvent & { currentTarget: HTMLTextAreaElement; target: HTMLTextAreaElement },
  ) => {
    const value = e.currentTarget.value;
    const cursor = e.currentTarget.selectionStart ?? value.length;
    options.setInput(value);
    await updateForValue(value, cursor);
  };

  const complete = (index = selectedIndex()) => {
    const selected = items()[index];
    const activeTrigger = trigger();
    const textarea = options.textareaRef();
    if (!selected || !activeTrigger || !textarea) return;

    const value = options.input();
    const start = triggerStart();
    const cursor = cursorPosition();
    const before = value.slice(0, start);
    const after = value.slice(cursor);

    let replacement = '';
    if (activeTrigger === '[[') {
      replacement = `[[${selected.insertText}]]`;
    } else if (activeTrigger === '@') {
      replacement = `@${selected.insertText}${lineSuffix()}`;
    } else if (activeTrigger === '#') {
      replacement = selected.insertText.startsWith('#')
        ? selected.insertText
        : `#${selected.insertText}`;
    } else {
      replacement = `/${selected.insertText}`;
    }

    // A line suffix after the cursor belongs to the path: `@READ|:12` must
    // become `@README.md:12`, not `@README.md :12`.
    const keepsSuffix = activeTrigger === '@' && /^:\d/.test(after);
    const needsSpace =
      activeTrigger !== '[[' && !keepsSuffix && after.length > 0 && !/^\s/.test(after);
    const insertText = needsSpace ? `${replacement} ` : replacement;
    const nextValue = `${before}${insertText}${after}`;
    const nextCursor = before.length + insertText.length;

    options.setInput(nextValue);
    queueMicrotask(() => {
      textarea.focus();
      textarea.setSelectionRange(nextCursor, nextCursor);
    });

    close();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (!isOpen() || items().length === 0) return;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % items().length);
      return;
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + items().length) % items().length);
      return;
    }

    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      complete();
      return;
    }

    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  };

  return {
    isOpen,
    items,
    selectedIndex,
    onKeyDown,
    onInput,
    complete,
    close,
  };
}
