import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { FileEntry } from '@/lib/types';
import type { SessionCommand } from '@/lib/api';
import { notificationActions, notificationStore } from '@/stores/notificationStore';
import {
  fuzzyFilter,
  resetCommandCache,
  useAutocomplete,
  type AutocompleteItem,
} from '@/hooks/useAutocomplete';

/**
 * Nothing in `@/lib/api` is stubbed. The command list is held by
 * `lib/query/commands.ts` now, so the hook runs the real `listSessionCommands`
 * against the mocked fetch — which is what proves the list is fetched once and
 * that a refusal does not poison the cache.
 */

/** What each route answers next. */
let commands: SessionCommand[] = [];
let files: FileEntry[] = [];
let notes: FileEntry[] = [];
/** What `fs.list_dir` answers, by the `rel_path` it is asked for. */
let dirs: Record<string, Array<{ name: string; rel_path: string; is_dir: boolean }>> = {};
/** When true, `POST /api/rpc/session.commands` refuses once. */
let refuseCommands = false;
/** When true, `fs.list_dir` refuses the root, as the daemon does for a root it did not admit. */
let refuseDirs = false;

let env: TestQueryEnv;

function installEnv(): void {
  env = createTestQueryEnv({
    'POST /api/rpc/session.commands': () => {
      if (!refuseCommands) return { commands };
      refuseCommands = false;
      return new Response(JSON.stringify({ error: { code: 503, message: 'offline' } }), {
        status: 503,
      });
    },
    // `list_notes` reaches the browser through `POST /api/rpc/{method}`
    // now ([[Simplification Plan#Step 19]] item 3): `useListKilnFiles`
    // (file mentions) and `useListKilnNotes` (wikilinks) both call it,
    // wire-identically — `GET /api/kiln/files` and `/api/kiln/notes`,
    // which used to answer the same projection under two names, are gone.
    // Each test below populates only one of `files`/`notes`, so one mock
    // answers both hooks correctly; only the "once for every composer"
    // test needs both at once, and it asserts the call count, not content.
    'POST /api/rpc/list_notes': () =>
      [...files, ...notes].map((e) => ({ name: e.name, path: e.path })),
    'POST /api/rpc/fs.list_dir': (request: Request) => {
      if (refuseDirs) {
        return new Response(
          JSON.stringify({ error: { code: 422, message: 'root is not a registered project' } }),
          { status: 422 },
        );
      }
      return request
        .clone()
        .json()
        .then((body: { rel_path?: string }) => {
          const rel = body.rel_path ?? '';
          const entries = (dirs[rel] ?? []).map((e) => ({
            ...e,
            size: 0,
            modified: null,
            status: null,
          }));
          return { entries, truncated: false };
        });
    },
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
function harness(initial = '', workspace: string | null = null) {
  const [input, setInput] = createSignal(initial);
  const textarea = document.createElement('textarea');
  const auto = useAutocomplete({
    input,
    setInput: setInput as never,
    kilnPath: () => '/kiln',
    workspacePath: () => workspace,
    sessionId: () => 's-1',
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
      { name: 'help', description: 'Show available commands', kind: 'builtin', command: 'help' },
      { name: 'models', description: 'List available models', kind: 'plugin', plugin: 'alpha' },
      {
        name: 'model',
        description: 'Switch to a different model',
        input_hint: '[name]',
        kind: 'builtin',
        command: 'model',
      },
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

  it('serves commands from the session catalog, not a hardcoded list', async () => {
    await createRoot(async (dispose) => {
      const { auto, type } = harness();
      await type('/');
      expect(env.fetch.calls('POST /api/rpc/session.commands')).toBe(1);
      // Descriptions come across for the popup's second line, with the source
      // of a command that the daemon runs.
      expect(auto.items().find((i) => i.label === '/help')?.detail).toBe('Show available commands');
      expect(auto.items().find((i) => i.label === '/models')?.detail).toBe(
        'List available models (plugin)',
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
      expect(env.fetch.calls('POST /api/rpc/session.commands')).toBe(1);

      // The seam: a daemon restarted under a browser that stayed open serves a
      // different set, and this is what lets the next keystroke see it.
      await resetCommandCache();
      await first.type('/m');
      expect(env.fetch.calls('POST /api/rpc/session.commands')).toBe(2);
      dispose();
    });
  });

  // The `@` list used to call `listFiles` straight through `lib/api`, so each
  // composer held its own copy and asked the daemon again. `listFiles` and
  // `listKilnNotes` both reshape one `list_notes` row now
  // ([[Simplification Plan#Step 19]] item 3) — the same listing serves the
  // `@` mentions, the `[[` wikilinks and the tags — so one fetch, under one
  // key, now serves two composers in one kiln.
  it('fetches the kiln files and notes once for every composer', async () => {
    await createRoot(async (dispose) => {
      notes = [{ name: 'Two.md', path: 'Two.md' } as FileEntry];

      const first = harness();
      await first.type('@');
      const second = harness();
      await second.type('@T');

      expect(second.auto.isOpen()).toBe(true);
      expect(env.fetch.calls('POST /api/rpc/list_notes')).toBe(1);
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

describe('useAutocomplete file mentions', () => {
  beforeEach(() => {
    commands = [];
    files = [{ name: 'Wikilinks.md', path: 'Help/Wikilinks.md', is_dir: false }];
    notes = [];
    dirs = {
      '': [
        { name: 'src', rel_path: 'src', is_dir: true },
        { name: 'README.md', rel_path: 'README.md', is_dir: false },
      ],
      src: [{ name: 'main.rs', rel_path: 'src/main.rs', is_dir: false }],
    };
    refuseCommands = false;
    refuseDirs = false;
    notificationActions.clearAll();
    installEnv();
  });

  // The daemon resolves `@path` against its roots. A basename names no file
  // there, so the completer must insert the path under the root.
  it('inserts the root-relative path', async () => {
    await createRoot(async (dispose) => {
      const { auto, type, input } = harness();
      await type('see @wiki');
      const index = auto.items().findIndex((i) => i.insertText === 'Help/Wikilinks.md');
      expect(index).toBeGreaterThanOrEqual(0);
      auto.complete(index);
      expect(input()).toBe('see @Help/Wikilinks.md');
      dispose();
    });
  });

  it('lists the files of the session workspace, one folder at a time', async () => {
    await createRoot(async (dispose) => {
      const { auto, type, input } = harness('', '/ws');
      await type('@src/ma');
      expect(auto.isOpen()).toBe(true);
      const index = auto.items().findIndex((i) => i.insertText === 'src/main.rs');
      expect(index).toBeGreaterThanOrEqual(0);
      auto.complete(index);
      expect(input()).toBe('@src/main.rs');
      dispose();
    });
  });

  // The user types `@` often. A workspace that is not an admitted root is not
  // an error that the user can correct here, so the completer shows no toast.
  it('a refused workspace shows no toast and lists the kiln files', async () => {
    await createRoot(async (dispose) => {
      refuseDirs = true;
      const { auto, type } = harness('', '/ws');
      await type('@wiki');
      expect(env.fetch.calls('POST /api/rpc/fs.list_dir')).toBeGreaterThan(0);
      expect(auto.isOpen()).toBe(true);
      expect(auto.items().map((i) => i.insertText)).toEqual(['Help/Wikilinks.md']);
      expect(notificationStore.notifications.filter((n) => !n.dismissed)).toEqual([]);
      dispose();
    });
  });

  it('completing a path keeps a line suffix', async () => {
    await createRoot(async (dispose) => {
      const { auto, type, input } = harness('', '/ws');

      // The suffix typed before the completion is accepted.
      await type('@READ:12-14');
      expect(auto.isOpen()).toBe(true);
      auto.complete(auto.items().findIndex((i) => i.insertText === 'README.md'));
      expect(input()).toBe('@README.md:12-14');

      // The suffix after the cursor stays, with no space before it.
      await type('@READ:12 why', '@READ'.length);
      expect(auto.isOpen()).toBe(true);
      auto.complete(auto.items().findIndex((i) => i.insertText === 'README.md'));
      expect(input()).toBe('@README.md:12 why');
      dispose();
    });
  });
});
