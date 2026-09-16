import { Accessor, Setter, createSignal } from 'solid-js';
import { listFiles } from '@/lib/api';
import { fetchKilnNotesOnce } from '@/lib/query/notes';
import { fetchSlashCommandsOnce } from '@/lib/query/commands';
import { fuzzyScore } from '@/lib/fuzzy';
import type { FileEntry } from '@/lib/types';

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
  textareaRef: Accessor<HTMLTextAreaElement | undefined>;
}

/**
 * The commands as popup rows.
 *
 * The list itself is held by `lib/query/commands.ts`, under one key for the
 * whole browser. This hook used to memoise its own promise beside that cache,
 * which meant a second copy of a list that is static for the daemon's
 * lifetime, and its own hand-written rule for dropping the copy after a
 * refused fetch. The query does both: a refusal leaves no data, so the next
 * keystroke asks again.
 */
function loadCommandItems(): Promise<AutocompleteItem[]> {
  return fetchSlashCommandsOnce().then((commands) =>
    commands.map((c) => ({
      id: `command:${c.name}`,
      label: `/${c.name}`,
      // Commands taking an argument keep the trailing space so the user can
      // type straight into it; nullary ones don't (nothing follows).
      insertText: c.args ? `${c.name} ` : c.name,
      detail: c.description,
    })),
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

    // The notes come from the shared entry the link completion reads, so a
    // composer and an open editor in one kiln ask the daemon once between
    // them. The file list has no key of its own yet and stays a plain read.
    const [files, notes] = await Promise.all([listFiles(kiln), fetchKilnNotesOnce(kiln)]);
    const fileOptions = toAutocompleteItems(files, 'file');
    const noteOptions = toAutocompleteItems(notes, 'note');
    setFileItems([...fileOptions, ...noteOptions]);
    setNoteItems(noteOptions);
    setTagItems(extractTagItems(notes));
    setLoadedKiln(kiln);
  };

  const sourceItemsFor = (kind: TriggerType): AutocompleteItem[] => {
    if (kind === '@') return fileItems();
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

    try {
      if (match.trigger === '/') {
        setCommandItems(await loadCommandItems());
      } else {
        await ensureKilnData();
      }
    } catch {
      close();
      return;
    }

    const filtered = fuzzyFilter(sourceItemsFor(match.trigger), match.query);
    if (filtered.length === 0) {
      close();
      return;
    }

    setItems(filtered);
    setIsOpen(true);
    setSelectedIndex((prev) => Math.min(prev, filtered.length - 1));
  };

  const onInput = async (e: InputEvent & { currentTarget: HTMLTextAreaElement; target: HTMLTextAreaElement }) => {
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
      replacement = `@${selected.insertText}`;
    } else if (activeTrigger === '#') {
      replacement = selected.insertText.startsWith('#') ? selected.insertText : `#${selected.insertText}`;
    } else {
      replacement = `/${selected.insertText}`;
    }

    const needsSpace = activeTrigger !== '[[' && after.length > 0 && !/^\s/.test(after);
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
