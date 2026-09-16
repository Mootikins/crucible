import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { keys } from '../keys';
import { installPluginEventRoute } from '../routes/plugins';
import { pluginEvents } from '../sse';
import {
  useExecutePluginOption,
  useInstallPlugin,
  usePluginCommands,
  usePluginList,
  usePluginOption,
  usePluginOptions,
  usePluginPublications,
  useReloadPlugin,
  useRemovePlugin,
  useRunPluginCommand,
  useSetPluginOption,
} from '../plugins';

/**
 * The plugin cache, which three panels used to hold three copies of.
 *
 * `PluginPanel` kept a list and an options tree, `PluginsSection` kept a second
 * list, and `SettingsModal` kept a second options tree. An install refreshed
 * none of them and a reload refreshed one, so a plugin added in settings stayed
 * invisible in the panel. Every case below asks the same question of the new
 * layer: does one write reach every reader?
 */

const ROW = {
  name: 'demo-plugin',
  version: '1.2.3',
  source: 'User',
  state: 'Active',
  dir: '/tmp/demo',
  tools: 3,
  commands: 1,
  handlers: 2,
  services: 0,
};

const TREE = { demo: { type: 'group', args: [{ key: 'flux', type: 'input' }] } };

const COMMAND = { plugin: 'demo-plugin', name: 'demo_run', effect: 'read' as const };

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

/** Counts what each read answered, so a refetch is visible in the answer. */
let generation: number;

/** The routes every case shares, each answering a new value per call. */
function routes(): Record<string, () => unknown> {
  return {
    'GET /api/plugins': () => ({ plugins: [{ ...ROW, tools: generation }] }),
    'GET /api/plugins/options': () => ({ options: TREE }),
    'GET /api/plugins/commands': () => ({ commands: [COMMAND] }),
    'POST /api/plugins': () => ({
      name: 'new-plugin',
      outcome: { kind: 'cloned', dest: '/tmp/new' },
      manifest: '/tmp/plugins.installed.json',
      installed: true,
      loaded: true,
      tools: 1,
      commands: 0,
      services: 0,
      error: null,
    }),
    'POST /api/plugins/demo-plugin/reload': () => ({
      name: 'demo-plugin',
      tools: 3,
      commands: 1,
      handlers: 2,
      services: 0,
    }),
    'DELETE /api/plugins/demo-plugin': () => ({
      name: 'demo-plugin',
      manifest: '/tmp/plugins.installed.json',
      purged_dir: null,
      purge_error: null,
      kept_dir: '/tmp/demo',
    }),
  };
}

beforeEach(() => {
  generation = 1;
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('the plugin roster', () => {
  it('fetches once for two callers of each key', async () => {
    env = createTestQueryEnv(routes());

    const read = inRoot(() => ({
      list: usePluginList(),
      listAgain: usePluginList(),
      options: usePluginOptions(),
      optionsAgain: usePluginOptions(),
      commands: usePluginCommands(),
      commandsAgain: usePluginCommands(),
    }));

    await vi.waitFor(() => {
      expect(read.list.data).toHaveLength(1);
      expect(read.options.data).toEqual(TREE);
      expect(read.commands.data).toEqual([COMMAND]);
    });
    expect(read.listAgain.data).toEqual(read.list.data);
    expect(read.optionsAgain.data).toEqual(TREE);
    expect(read.commandsAgain.data).toEqual([COMMAND]);

    expect(env.fetch.calls('GET /api/plugins')).toBe(1);
    expect(env.fetch.calls('GET /api/plugins/options')).toBe(1);
    expect(env.fetch.calls('GET /api/plugins/commands')).toBe(1);
  });

  // The hazard the comment at `PluginPanel.tsx:164` described: an install that
  // refreshed nothing, and a reload that refreshed one panel's list.
  it('refreshes the list, the trees and the commands after an install', async () => {
    env = createTestQueryEnv(routes());

    const read = inRoot(() => ({
      list: usePluginList(),
      options: usePluginOptions(),
      commands: usePluginCommands(),
      install: useInstallPlugin(),
    }));
    await vi.waitFor(() => expect(read.list.data?.[0]?.tools).toBe(1));

    generation = 2;
    await read.install.mutateAsync({ url: 'user/repo' });

    await vi.waitFor(() => expect(read.list.data?.[0]?.tools).toBe(2));
    expect(env.fetch.calls('GET /api/plugins')).toBe(2);
    expect(env.fetch.calls('GET /api/plugins/options')).toBe(2);
    expect(env.fetch.calls('GET /api/plugins/commands')).toBe(2);
  });

  it('refreshes the same three keys after a reload', async () => {
    env = createTestQueryEnv(routes());

    const read = inRoot(() => ({
      list: usePluginList(),
      options: usePluginOptions(),
      commands: usePluginCommands(),
      reload: useReloadPlugin(),
    }));
    await vi.waitFor(() => expect(read.list.data?.[0]?.tools).toBe(1));

    generation = 3;
    await read.reload.mutateAsync('demo-plugin');

    await vi.waitFor(() => expect(read.list.data?.[0]?.tools).toBe(3));
    expect(env.fetch.calls('GET /api/plugins/options')).toBe(2);
    expect(env.fetch.calls('GET /api/plugins/commands')).toBe(2);
  });

  it('refreshes the same three keys after a removal', async () => {
    env = createTestQueryEnv(routes());

    const read = inRoot(() => ({
      list: usePluginList(),
      options: usePluginOptions(),
      commands: usePluginCommands(),
      remove: useRemovePlugin(),
    }));
    await vi.waitFor(() => expect(read.list.data?.[0]?.tools).toBe(1));

    generation = 4;
    await read.remove.mutateAsync({ name: 'demo-plugin', purge: true });

    await vi.waitFor(() => expect(read.list.data?.[0]?.tools).toBe(4));
    expect(env.fetch.calls('DELETE /api/plugins/demo-plugin')).toBe(1);
    expect(env.fetch.calls('GET /api/plugins/options')).toBe(2);
    expect(env.fetch.calls('GET /api/plugins/commands')).toBe(2);
  });

  // A refused read must reach the caller as an error. The panel used to answer
  // an empty roster, which reads as "no plugins installed".
  it('surfaces a refusal rather than an empty roster', async () => {
    env = createTestQueryEnv({
      'GET /api/plugins': apiError(500, 'the plugin host is unreachable'),
    });

    const list = inRoot(() => usePluginList());

    await vi.waitFor(() => expect(list.isError).toBe(true));
    expect(list.error?.message).toContain('Failed to list plugins');
    expect(list.data).toBeUndefined();
  });
});

describe('one plugin option', () => {
  /** Answers the stored value, and takes what a `set` writes. */
  function optionRoutes(stored: { value: unknown }, refuse = false) {
    return {
      'POST /api/plugins/demo-plugin/option': async (request: Request) => {
        const body = (await request.json()) as { action: string; value?: unknown };
        if (body.action === 'get') return { value: stored.value };
        if (refuse) return new Response(JSON.stringify({ error: { code: 422, message: 'refused' } }), { status: 422 });
        if (body.action === 'set') stored.value = body.value;
        return {};
      },
    };
  }

  it('re-reads the plugin, not only the row, after a write', async () => {
    const stored = { value: 'first' };
    env = createTestQueryEnv(optionRoutes(stored));

    const read = inRoot(() => ({
      flux: usePluginOption(
        () => 'demo-plugin',
        () => ['flux'],
      ),
      sibling: usePluginOption(
        () => 'demo-plugin',
        () => ['other'],
      ),
      set: useSetPluginOption(
        () => 'demo-plugin',
        () => ['flux'],
      ),
    }));
    await vi.waitFor(() => expect(read.flux.data).toBe('first'));
    await vi.waitFor(() => expect(read.sibling.data).toBe('first'));
    const before = env.fetch.calls('POST /api/plugins/demo-plugin/option');

    await read.set.mutateAsync('second');

    // A `values` or `disabled` function can read the option just written, so
    // every row of the plugin is asked again, not only the one that changed.
    await vi.waitFor(() => expect(read.flux.data).toBe('second'));
    expect(env.fetch.calls('POST /api/plugins/demo-plugin/option')).toBeGreaterThan(before + 1);
  });

  it('paints the new value at once and puts the old one back on a refusal', async () => {
    const stored = { value: 'first' };
    env = createTestQueryEnv(optionRoutes(stored, true));

    const read = inRoot(() => ({
      flux: usePluginOption(
        () => 'demo-plugin',
        () => ['flux'],
      ),
      set: useSetPluginOption(
        () => 'demo-plugin',
        () => ['flux'],
      ),
    }));
    await vi.waitFor(() => expect(read.flux.data).toBe('first'));

    await expect(read.set.mutateAsync('second')).rejects.toThrow();

    await vi.waitFor(() => expect(read.flux.data).toBe('first'));
  });

  it('asks the plugin again after an execute', async () => {
    const stored = { value: 'first' };
    env = createTestQueryEnv(optionRoutes(stored));

    const read = inRoot(() => ({
      flux: usePluginOption(
        () => 'demo-plugin',
        () => ['flux'],
      ),
      press: useExecutePluginOption(
        () => 'demo-plugin',
        () => ['run'],
      ),
    }));
    await vi.waitFor(() => expect(read.flux.data).toBe('first'));
    const before = env.fetch.calls('POST /api/plugins/demo-plugin/option');

    stored.value = 'after the press';
    await read.press.mutateAsync();

    await vi.waitFor(() => expect(read.flux.data).toBe('after the press'));
    expect(env.fetch.calls('POST /api/plugins/demo-plugin/option')).toBeGreaterThan(before);
  });
});

describe('a plugin command', () => {
  it('hands back what the plugin answered, and names the caller', async () => {
    let seen: { name?: string; args?: unknown } = {};
    let caller: string | null = null;
    env = createTestQueryEnv({
      'POST /api/plugins/command': async (request: Request) => {
        caller = request.headers.get('X-Crucible-Plugin');
        seen = (await request.json()) as { name?: string; args?: unknown };
        return { ok: true };
      },
    });

    const run = inRoot(() => useRunPluginCommand());
    const answer = await run.mutateAsync({
      command: 'kanban_move',
      args: { file: 'a.md' },
      caller: 'kanban',
    });

    expect(answer).toEqual({ ok: true });
    expect(seen).toEqual({ name: 'kanban_move', args: { file: 'a.md' } });
    // The block declares itself as the plugin it draws for; the route reads
    // that header. A command sent without it is indistinguishable from the app.
    expect(caller).toBe('kanban');
  });
});

describe('a publication', () => {
  beforeEach(() => {
    installFakeEventSource();
  });

  it('re-reads the block the daemon named, and leaves a sibling key alone', async () => {
    let answered = 0;
    env = createTestQueryEnv({
      'GET /api/plugins/publications': (request: Request) => {
        answered += 1;
        const key = new URL(request.url).searchParams.get('key') ?? '';
        return { publications: { [key]: { board: `value ${answered}` } } };
      },
    });
    installPluginEventRoute();

    const read = inRoot(() => ({
      rows: usePluginPublications('board', 'rows'),
      columns: usePluginPublications('board', 'columns'),
    }));
    // A block holds the stream open while it is on screen; the route does the
    // cache write from inside it.
    const stop = pluginEvents().subscribe(() => {});
    await vi.waitFor(() => {
      expect(read.rows.data).toBeDefined();
      expect(read.columns.data).toBeDefined();
    });
    const columnsBefore = read.columns.data;
    const reads = env.fetch.calls('GET /api/plugins/publications');

    onlyEventSource().emit('publication_changed', { plugin: 'board', key: 'rows' });

    await vi.waitFor(() =>
      expect(env.fetch.calls('GET /api/plugins/publications')).toBe(reads + 1),
    );
    expect(read.columns.data).toEqual(columnsBefore);
    expect(read.rows.data).not.toEqual(columnsBefore);
    stop();
  });

  it('holds the key the invalidation names', () => {
    expect(keys.pluginPublications('board', 'rows')).toEqual([
      'plugins',
      'publications',
      'board',
      'rows',
    ]);
  });
});
