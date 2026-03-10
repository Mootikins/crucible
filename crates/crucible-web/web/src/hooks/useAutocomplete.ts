import { Accessor, Setter, createSignal } from 'solid-js';
import { listFiles, listKilnNotes } from '@/lib/api';
import type { FileEntry } from '@/lib/types';

type TriggerType = '@' | '#' | '/' | '[[';

export interface AutocompleteItem {
  id: string;
  label: string;
  insertText: string;
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

const COMMAND_ITEMS: AutocompleteItem[] = [
  { id: 'command-search', label: '/search', insertText: 'search ' },
  { id: 'command-model', label: '/model', insertText: 'model ' },
  { id: 'command-help', label: '/help', insertText: 'help ' },
  { id: 'command-clear', label: '/clear', insertText: 'clear' },
  { id: 'command-export', label: '/export', insertText: 'export ' },
];

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

function fuzzyFilter(items: AutocompleteItem[], query: string): AutocompleteItem[] {
  const normalizedQuery = query.toLowerCase();
  if (!normalizedQuery) return items;
  return items.filter((item) => item.label.toLowerCase().includes(normalizedQuery));
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

    const [files, notes] = await Promise.all([listFiles(kiln), listKilnNotes(kiln)]);
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
    if (kind === '/') return COMMAND_ITEMS;
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

    if (match.trigger === '@' || match.trigger === '#' || match.trigger === '[[') {
      try {
        await ensureKilnData();
      } catch {
        close();
        return;
      }
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
