import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { FileEntry } from '@/lib/types';
import type { SlashCommand } from '@/lib/api';
import {
  fuzzyFilter,
  resetCommandCache,
  useAutocomplete,
  type AutocompleteItem,
} from '@/hooks/useAutocomplete';

/**
 * Nothing in `@/lib/api` is stubbed. The command list is held by
 * `lib/query/commands.ts` now, so the hook runs the real `listSlashCommands`
 * against the mocked fetch — which is what proves the list is fetched once and
 * that a refusal does not poison the cache.
 */

/** What each route answers next. */
let commands: SlashCommand[] = [];
let files: FileEntry[] = [];
let notes: FileEntry[] = [];
/** When true, `GET /api/commands` refuses once. */
let refuseCommands = false;

let env: TestQueryEnv;

function installEnv(): void {
  env = createTestQueryEnv({
    'GET /api/commands': () => {
      if (!refuseCommands) return { commands };
      refuseCommands = false;
      return new Response(JSON.stringify({ error: { code: 503, message: 'offline' } }), {
        status: 503,
      });
    },
    'GET /api/kiln/files': () => ({ files }),
    'GET /api/kiln/notes': () => ({ files: notes }),
  });
}

afterEach(() => {
  env?.restore();
});

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
    commands = [
      { name: 'help', args: '', description: 'Show available commands' },
      { name: 'models', args: '', description: 'List available models' },
      { name: 'model', args: '<name>', description: 'Switch to a different model' },
    ];
    files = [];
    notes = [];
    refuseCommands = false;
    installEnv();
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
      expect(env.fetch.calls('GET /api/commands')).toBe(1);
      // Descriptions come across for the popup's second line.
      expect(auto.items().find((i) => i.label === '/help')?.detail).toBe(
        'Show available commands',
      );
      dispose();
    });
  });

  // The daemon serves the commands from the constant it dispatches on, so they
  // cannot change while it runs. Two composers, and a composer that opens the
  // popup twice, must cost one GET between them.
  it('fetches the list once for every composer, and again after a reset', async () => {
    await createRoot(async (dispose) => {
      const first = harness();
      await first.type('/');
      const second = harness();
      await second.type('/h');
      expect(second.auto.isOpen()).toBe(true);
      expect(env.fetch.calls('GET /api/commands')).toBe(1);

      // The seam: a daemon restarted under a browser that stayed open serves a
      // different set, and this is what lets the next keystroke see it.
      await resetCommandCache();
      await first.type('/m');
      expect(env.fetch.calls('GET /api/commands')).toBe(2);
      dispose();
    });
  });

  // The `@` list used to call `listFiles` straight through `lib/api`, so each
  // composer held its own copy and asked the daemon again. Both lists read
  // `lib/query/notes.ts` now, so two composers in one kiln cost one GET each.
  it('fetches the kiln files and notes once for every composer', async () => {
    await createRoot(async (dispose) => {
      files = [{ name: 'One.md', path: 'One.md' } as FileEntry];
      notes = [{ name: 'Two.md', path: 'Two.md' } as FileEntry];

      const first = harness();
      await first.type('@');
      const second = harness();
      await second.type('@O');

      expect(second.auto.isOpen()).toBe(true);
      expect(env.fetch.calls('GET /api/kiln/files')).toBe(1);
      expect(env.fetch.calls('GET /api/kiln/notes')).toBe(1);
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
      refuseCommands = true;
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
    commands = [];
    files = [];
    notes = [
      { name: 'Wikilinks', path: 'Help/Wikilinks.md', is_dir: false },
      { name: 'Tags', path: 'Help/Tags.md', is_dir: false },
    ];
    refuseCommands = false;
    installEnv();
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
