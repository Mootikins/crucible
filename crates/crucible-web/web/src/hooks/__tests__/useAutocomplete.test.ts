import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRoot, createSignal } from 'solid-js';

const listSlashCommandsMock = vi.fn();
const listFilesMock = vi.fn();
const listKilnNotesMock = vi.fn();

vi.mock('@/lib/api', () => ({
  listSlashCommands: () => listSlashCommandsMock(),
  listFiles: (p: string) => listFilesMock(p),
  listKilnNotes: (p: string) => listKilnNotesMock(p),
}));

import {
  fuzzyFilter,
  useAutocomplete,
  resetCommandCache,
  type AutocompleteItem,
} from '@/hooks/useAutocomplete';

const item = (label: string): AutocompleteItem => ({ id: label, label, insertText: label });

describe('useAutocomplete fuzzyFilter', () => {
  it('ranks by relevance, not raw daemon order', () => {
    // Daemon order puts the weaker match first; fuzzy ranking must reorder.
    const items = [item('Meeting Notes'), item('Notes'), item('Nonsense')];
    const out = fuzzyFilter(items, 'notes').map((i) => i.label);
    // Exact/tighter match ranks above the looser one; non-matches drop.
    expect(out[0]).toBe('Notes');
    expect(out).toContain('Meeting Notes');
    expect(out).not.toContain('Nonsense');
  });

  it('returns all items unchanged for an empty query', () => {
    const items = [item('b'), item('a')];
    expect(fuzzyFilter(items, '').map((i) => i.label)).toEqual(['b', 'a']);
  });
});

/** Drive the hook the way a textarea does, without mounting a component. */
function harness(initial = '') {
  const [input, setInput] = createSignal(initial);
  const textarea = document.createElement('textarea');
  const auto = useAutocomplete({
    input,
    setInput: setInput as never,
    kilnPath: () => '/kiln',
    textareaRef: () => textarea,
  });
  const type = async (value: string, cursor = value.length) => {
    textarea.value = value;
    textarea.selectionStart = cursor;
    await auto.onInput({ currentTarget: textarea, target: textarea } as never);
  };
  return { auto, input, type, textarea };
}

describe('useAutocomplete slash commands', () => {
  beforeEach(() => {
    resetCommandCache();
    vi.clearAllMocks();
    listSlashCommandsMock.mockResolvedValue([
      { name: 'help', args: '', description: 'Show available commands' },
      { name: 'models', args: '', description: 'List available models' },
      { name: 'model', args: '<name>', description: 'Switch to a different model' },
    ]);
    listFilesMock.mockResolvedValue([]);
    listKilnNotesMock.mockResolvedValue([]);
  });

  it('opens the popup when the user types "/"', async () => {
    await createRoot(async (dispose) => {
      const { auto, type } = harness();
      await type('/');
      expect(auto.isOpen()).toBe(true);
      expect(auto.items().map((i) => i.label)).toContain('/models');
      dispose();
    });
  });

  it('serves commands from the server, not a hardcoded list', async () => {
    await createRoot(async (dispose) => {
      const { auto, type } = harness();
      await type('/');
      expect(listSlashCommandsMock).toHaveBeenCalled();
      // Descriptions come across for the popup's second line.
      expect(auto.items().find((i) => i.label === '/help')?.detail).toBe(
        'Show available commands',
      );
      dispose();
    });
  });

  it('narrows the list as the command name is typed', async () => {
    await createRoot(async (dispose) => {
      const { auto, type } = harness();
      await type('/mod');
      const labels = auto.items().map((i) => i.label);
      expect(labels).toContain('/model');
      expect(labels).not.toContain('/help');
      dispose();
    });
  });

  it('inserts the command and leaves a space for commands taking an argument', async () => {
    await createRoot(async (dispose) => {
      const { auto, type, input } = harness();
      await type('/model');
      const index = auto.items().findIndex((i) => i.label === '/model');
      auto.complete(index);
      expect(input()).toBe('/model ');
      expect(auto.isOpen()).toBe(false);
      dispose();
    });
  });

  it('does not treat a path separator as a command trigger', async () => {
    await createRoot(async (dispose) => {
      const { auto, type } = harness();
      await type('see src/lib');
      expect(auto.isOpen()).toBe(false);
      dispose();
    });
  });

  it('stays closed when the command fetch fails, and retries on the next keystroke', async () => {
    await createRoot(async (dispose) => {
      listSlashCommandsMock.mockRejectedValueOnce(new Error('offline'));
      const { auto, type } = harness();
      await type('/');
      expect(auto.isOpen()).toBe(false);

      // A rejected fetch must not poison the cache.
      await type('/h');
      expect(auto.isOpen()).toBe(true);
      expect(auto.items().map((i) => i.label)).toContain('/help');
      dispose();
    });
  });
});

describe('useAutocomplete wikilinks', () => {
  beforeEach(() => {
    resetCommandCache();
    vi.clearAllMocks();
    listSlashCommandsMock.mockResolvedValue([]);
    listFilesMock.mockResolvedValue([]);
    listKilnNotesMock.mockResolvedValue([
      { name: 'Wikilinks', path: 'Help/Wikilinks.md' },
      { name: 'Tags', path: 'Help/Tags.md' },
    ]);
  });

  it('opens on "[[" and completes to a closed wikilink', async () => {
    await createRoot(async (dispose) => {
      const { auto, type, input } = harness();
      await type('see [[wiki');
      expect(auto.isOpen()).toBe(true);

      const index = auto.items().findIndex((i) => i.label === 'Wikilinks');
      expect(index).toBeGreaterThanOrEqual(0);
      auto.complete(index);
      expect(input()).toBe('see [[Wikilinks]]');
      dispose();
    });
  });

  it('stays closed once the link is already closed', async () => {
    await createRoot(async (dispose) => {
      const { auto, type } = harness();
      await type('see [[Tags]] then');
      expect(auto.isOpen()).toBe(false);
      dispose();
    });
  });
});
