import { describe, it, expect, afterEach } from 'vitest';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { getDiffFile, getDiffset } from '../diff-api';
import type { DiffFileEntry, Diffset, DiffsetSource } from '../diffset';

// The mock `fetch` answers the two diff rows through `POST /api/rpc/{method}`.
// Each case asserts the JSON body that went on the wire: `diff.get` and
// `diff.file` both take the tagged `DiffsetSource` directly now
// (Simplification Plan step 19), so there is no flat query string to build.

const record: DiffsetSource = { kind: 'session_record', session: 'chat-1' };

const recordSet: Diffset = {
  id: 'session-chat-1',
  source: record,
  files: [],
  unreadable_roots: [],
};

function entry(root: string, path: string, over: Partial<DiffFileEntry> = {}): DiffFileEntry {
  return {
    root,
    path,
    status: { kind: 'modified' },
    added: 1,
    removed: 1,
    binary: false,
    too_large: false,
    ...over,
  };
}

let env: TestQueryEnv;

function serve(): void {
  env = createTestQueryEnv({
    'POST /api/rpc/diff.get': { body: recordSet },
    'POST /api/rpc/diff.file': { body: { base_text: 'a\n', current_text: 'b\n' } },
  });
}

afterEach(() => {
  env?.restore();
});

describe('diff-api', () => {
  it('asks for a session record by the session id', async () => {
    serve();
    expect(await getDiffset(record)).toEqual(recordSet);

    const sent = await env.fetch.sent(0);
    expect(sent.path).toBe('/api/rpc/diff.get');
    expect(sent.body).toEqual({ source: record });
  });

  it('sends the root of the entry with each file of a session record', async () => {
    serve();
    const text = await getDiffFile(record, entry('/work/b', 'notes/x.md'));
    expect(text).toEqual({ base_text: 'a\n', current_text: 'b\n' });

    const sent = await env.fetch.sent(0);
    expect(sent.path).toBe('/api/rpc/diff.file');
    expect(sent.body).toEqual({
      source: record,
      path: 'notes/x.md',
      root: '/work/b',
    });
  });

  it('sends the old path of a renamed file in a session record', async () => {
    serve();
    await getDiffFile(
      record,
      entry('/work', 'new.md', { status: { kind: 'renamed', from: 'old.md' } }),
    );

    const sent = await env.fetch.sent(0);
    expect(sent.body).toMatchObject({ from: 'old.md', root: '/work' });
  });

  it('asks for a branch by its root, and sends no root field', async () => {
    serve();
    const branch: DiffsetSource = { kind: 'branch', root: '/repo', base: 'main', head: 'topic' };
    await getDiffFile(branch, entry('/repo', 'a.rs'));

    const sent = await env.fetch.sent(0);
    expect(sent.body).toEqual({ source: branch, path: 'a.rs' });
  });

  it('asks for a proposal by its id, and sends the root of each file', async () => {
    serve();
    const proposal: DiffsetSource = {
      kind: 'proposal',
      id: '00000000-0000-0000-0000-000000000000',
    };
    await getDiffset(proposal);
    await getDiffFile(proposal, entry('/kiln', 'a.md'));

    const list = await env.fetch.sent(0);
    expect(list.path).toBe('/api/rpc/diff.get');
    expect(list.body).toEqual({ source: proposal });
    const file = await env.fetch.sent(1);
    expect(file.path).toBe('/api/rpc/diff.file');
    expect(file.body).toEqual({ source: proposal, path: 'a.md', root: '/kiln' });
  });
});
