import { describe, it, expect, afterEach } from 'vitest';
import { createRoot } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { keys } from '../keys';
import { fsEvents } from '../sse';
import { installFsEventRoute } from '../routes/fs';
import {
  absFolderPath,
  folderOf,
  fetchDirOnce,
  useFsMkdir,
  useFsMove,
  useFsTrash,
  useGetFileContent,
  useListDir,
  useSaveFileContent,
} from '../fs';

/**
 * The filesystem cache, from both ends: what a reader asks for, and what a
 * write or an event makes wrong.
 *
 * The load-bearing fact under every case is the KEY. The daemon's stream says
 * `/kiln/notes/a.md` and nothing else — no root, no relative path — so a
 * listing held under the pair the daemon's list route takes (`root`,
 * `rel_path`) is a listing no event can reach. Every listing here is held
 * under the absolute folder the stream names, and the first three cases are
 * what proves the two agree.
 */

const ROOT = '/proj';

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
let stopStream: (() => void) | null = null;
/** The `rel_path` of every `GET /api/fs/list` this test answered, in order. */
let listed: string[] = [];
/** The body of every write route this test answered, in order. */
let wrote: { path: string; body: Record<string, unknown> }[] = [];

/** The routes of the filesystem: one recording list, and the four writes. */
function fsRoutes() {
  listed = [];
  wrote = [];
  const record = (path: string) => async (request: Request) => {
    wrote.push({ path, body: (await request.json()) as Record<string, unknown> });
    return {};
  };
  return {
    'GET /api/fs/list': (request: Request) => {
      const rel = new URL(request.url).searchParams.get('rel_path') ?? '';
      listed.push(rel);
      return { entries: [{ rel_path: `${rel}/a.md`, name: 'a.md', is_dir: false }], truncated: false };
    },
    'GET /api/kiln/file': () => ({ content: 'on disk\n', content_hash: 'h1' }),
    'PUT /api/kiln/file': record('PUT'),
    'POST /api/fs/move': record('move'),
    'POST /api/fs/mkdir': record('mkdir'),
    'POST /api/fs/trash': record('trash'),
  };
}

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

/** Opens the shared filesystem stream with its route installed. */
function openStream() {
  installFakeEventSource();
  installFsEventRoute();
  stopStream = fsEvents().subscribe(() => {});
  return onlyEventSource();
}

/** How many times the daemon was asked to list one relative folder. */
const listCount = (rel: string) => listed.filter((seen) => seen === rel).length;

afterEach(() => {
  stopStream?.();
  stopStream = null;
  dispose?.();
  dispose = null;
  env?.restore();
});

describe('absFolderPath', () => {
  // The helper exists for ONE reason: the key a reader writes must be the
  // string the stream's route invalidates. Asserting the two functions against
  // each other is what holds that, rather than two lists of expected strings
  // that can drift apart one at a time.
  it('answers the folder the stream route names for a file in it', () => {
    const cases: [string, string][] = [
      ['/proj', ''],
      ['/proj', 'src'],
      ['/proj', 'src/deep'],
      ['/proj/', 'src/'],
      ['/', 'src'],
    ];
    for (const [root, rel] of cases) {
      const folder = absFolderPath(root, rel);
      expect(folderOf(`${folder}/a.md`)).toBe(folder);
    }
  });

  it('answers the root itself for the root folder', () => {
    expect(absFolderPath('/proj', '')).toBe('/proj');
    expect(absFolderPath('/proj/', '')).toBe('/proj');
    expect(absFolderPath('/', '')).toBe('/');
  });
});

describe('useListDir', () => {
  it('asks once for two readers of one folder', async () => {
    env = createTestQueryEnv(fsRoutes());

    const both = inRoot(() => ({
      first: useListDir(() => ({ root: ROOT, relPath: 'src' })),
      second: useListDir(() => ({ root: ROOT, relPath: 'src' })),
    }));

    await waitFor(() => expect(both.first.data).toBeDefined());
    await waitFor(() => expect(both.second.data).toBeDefined());
    expect(listCount('src')).toBe(1);
  });

  it('holds the listing under the absolute folder the stream names', async () => {
    env = createTestQueryEnv(fsRoutes());

    const query = inRoot(() => useListDir(() => ({ root: ROOT, relPath: 'src' })));
    await waitFor(() => expect(query.data).toBeDefined());

    expect(env.client.getQueryData(keys.fsDir('/proj/src'))).toEqual(query.data);
  });

  it('asks again when the stream says a file in that folder changed', async () => {
    env = createTestQueryEnv(fsRoutes());
    const source = openStream();
    const query = inRoot(() => useListDir(() => ({ root: ROOT, relPath: 'src' })));
    await waitFor(() => expect(query.data).toBeDefined());
    expect(listCount('src')).toBe(1);

    source.emit('fs_changed', { type: 'changed', path: '/proj/src/a.md', kind: 'modified' });

    await waitFor(() => expect(listCount('src')).toBe(2));
  });

  // **The negative.** A key that carried the root and the relative path apart
  // would match no event at all; a key that matched every event would refetch
  // every folder on every write. This is the half of the pair that says the
  // match is on the folder and not on the stream.
  it('leaves a folder the event did not name alone', async () => {
    env = createTestQueryEnv(fsRoutes());
    const source = openStream();
    const query = inRoot(() => useListDir(() => ({ root: ROOT, relPath: 'src' })));
    await waitFor(() => expect(query.data).toBeDefined());

    source.emit('fs_changed', { type: 'changed', path: '/proj/other/a.md', kind: 'modified' });

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(listCount('src')).toBe(1);
  });

  it('holds nothing while there is no folder to ask about', async () => {
    env = createTestQueryEnv(fsRoutes());

    const query = inRoot(() => useListDir(() => null));

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(query.data).toBeUndefined();
    expect(listed).toEqual([]);
  });
});

describe('fetchDirOnce', () => {
  // The panel's tree is a recursion over the folders the user left expanded,
  // so it cannot mount an observer per folder. It reads the same entries the
  // hook does, and a folder already held is not asked for twice.
  it('answers a held folder without asking the daemon again', async () => {
    env = createTestQueryEnv(fsRoutes());

    const first = await fetchDirOnce({ root: ROOT, relPath: 'src' });
    const second = await fetchDirOnce({ root: ROOT, relPath: 'src' });

    expect(second).toEqual(first);
    expect(listCount('src')).toBe(1);
  });

  it('asks again once the stream made the folder wrong', async () => {
    env = createTestQueryEnv(fsRoutes());
    const source = openStream();
    await fetchDirOnce({ root: ROOT, relPath: 'src' });

    source.emit('fs_changed', { type: 'changed', path: '/proj/src/a.md', kind: 'modified' });
    await fetchDirOnce({ root: ROOT, relPath: 'src' });

    expect(listCount('src')).toBe(2);
  });
});

describe('the filesystem writes', () => {
  /** Mounts one reader per folder and waits for both to answer. */
  async function readBoth(a: string, b: string) {
    const both = inRoot(() => ({
      from: useListDir(() => ({ root: ROOT, relPath: a })),
      to: useListDir(() => ({ root: ROOT, relPath: b })),
    }));
    await waitFor(() => expect(both.from.data).toBeDefined());
    await waitFor(() => expect(both.to.data).toBeDefined());
    return both;
  }

  // The stream says so too, a moment later. The writer invalidates anyway:
  // the user who dropped the row is looking at the panel now, and a tree that
  // waits for the daemon's event shows the file in its old folder until it
  // arrives — or forever, on a root the daemon does not watch.
  it('asks for the source and the target folder again after a move', async () => {
    env = createTestQueryEnv(fsRoutes());
    await readBoth('src', 'dst');
    const move = inRoot(() => useFsMove());

    await move.mutateAsync({ root: ROOT, kind: 'project', fromRel: 'src/a.md', toRel: 'dst/a.md' });

    await waitFor(() => expect(listCount('src')).toBe(2));
    await waitFor(() => expect(listCount('dst')).toBe(2));
    expect(wrote[0]!.body).toMatchObject({ from_rel: 'src/a.md', to_rel: 'dst/a.md' });
  });

  it('asks for the parent folder again after a new folder', async () => {
    env = createTestQueryEnv(fsRoutes());
    await readBoth('src', 'dst');
    const mkdir = inRoot(() => useFsMkdir());

    await mkdir.mutateAsync({ root: ROOT, kind: 'project', relPath: 'src/new' });

    await waitFor(() => expect(listCount('src')).toBe(2));
    expect(listCount('dst')).toBe(1);
  });

  it('asks for the parent folder again after a trash', async () => {
    env = createTestQueryEnv(fsRoutes());
    await readBoth('src', 'dst');
    const trash = inRoot(() => useFsTrash());

    await trash.mutateAsync({ root: ROOT, kind: 'project', relPath: 'src/a.md' });

    await waitFor(() => expect(listCount('src')).toBe(2));
    expect(listCount('dst')).toBe(1);
  });

  // A save writes bytes AND a row: the listing beside it carries the size and
  // the modified time of the file, and a new note is a name that was not there.
  it('makes the file and its folder wrong after a save', async () => {
    env = createTestQueryEnv(fsRoutes());
    const both = await readBoth('src', 'dst');
    const read = inRoot(() => useGetFileContent(() => '/proj/src/a.md'));
    await waitFor(() => expect(read.data).toBe('on disk\n'));
    expect(env.fetch.calls('GET /api/kiln/file')).toBe(1);
    const save = inRoot(() => useSaveFileContent());

    await save.mutateAsync({ path: '/proj/src/a.md', content: 'typed\n' });

    await waitFor(() => expect(listCount('src')).toBe(2));
    await waitFor(() => expect(env.fetch.calls('GET /api/kiln/file')).toBe(2));
    expect(listCount('dst')).toBe(1);
    expect(both.to.data).toBeDefined();
  });
});

describe('useGetFileContent', () => {
  it('asks once for two readers of one file', async () => {
    env = createTestQueryEnv(fsRoutes());

    const both = inRoot(() => ({
      first: useGetFileContent(() => '/proj/src/a.md'),
      second: useGetFileContent(() => '/proj/src/a.md'),
    }));

    await waitFor(() => expect(both.first.data).toBe('on disk\n'));
    await waitFor(() => expect(both.second.data).toBe('on disk\n'));
    expect(env.fetch.calls('GET /api/kiln/file')).toBe(1);
  });

  it('asks again when the stream says the file changed', async () => {
    env = createTestQueryEnv(fsRoutes());
    const source = openStream();
    const query = inRoot(() => useGetFileContent(() => '/proj/src/a.md'));
    await waitFor(() => expect(query.data).toBe('on disk\n'));

    source.emit('fs_changed', { type: 'changed', path: '/proj/src/a.md', kind: 'modified' });

    await waitFor(() => expect(env.fetch.calls('GET /api/kiln/file')).toBe(2));
  });
});
