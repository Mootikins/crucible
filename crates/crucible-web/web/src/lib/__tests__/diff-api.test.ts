import { describe, it, expect, afterEach } from 'vitest';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { getDiffFile, getDiffset } from '../diff-api';
import type { DiffFileEntry, Diffset, DiffsetSource } from '../diffset';

// The mock `fetch` answers the two diff routes. Each case asserts the query
// that went on the wire.

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
    'GET /api/diff': { body: recordSet },
    'GET /api/diff/file': { body: { base_text: 'a\n', current_text: 'b\n' } },
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
    expect(sent.path).toBe('/api/diff');
    expect(sent.query.get('session')).toBe('chat-1');
    expect(sent.query.has('root')).toBe(false);
    expect(sent.query.has('base')).toBe(false);
    expect(sent.query.has('head')).toBe(false);
  });

  it('sends the root of the entry with each file of a session record', async () => {
    serve();
    const text = await getDiffFile(record, entry('/work/b', 'notes/x.md'));
    expect(text).toEqual({ base_text: 'a\n', current_text: 'b\n' });

    const sent = await env.fetch.sent(0);
    expect(sent.path).toBe('/api/diff/file');
    expect(sent.query.get('session')).toBe('chat-1');
    expect(sent.query.get('root')).toBe('/work/b');
    expect(sent.query.get('path')).toBe('notes/x.md');
    expect(sent.query.has('from')).toBe(false);
    expect(sent.query.has('base')).toBe(false);
  });

  it('sends the old path of a renamed file in a session record', async () => {
    serve();
    await getDiffFile(
      record,
      entry('/work', 'new.md', { status: { kind: 'renamed', from: 'old.md' } }),
    );

    const sent = await env.fetch.sent(0);
    expect(sent.query.get('from')).toBe('old.md');
    expect(sent.query.get('root')).toBe('/work');
  });

  it('asks for a branch by its root, and sends no session', async () => {
    serve();
    const branch: DiffsetSource = { kind: 'branch', root: '/repo', base: 'main', head: 'topic' };
    await getDiffFile(branch, entry('/repo', 'a.rs'));

    const sent = await env.fetch.sent(0);
    expect(sent.query.get('root')).toBe('/repo');
    expect(sent.query.get('base')).toBe('main');
    expect(sent.query.get('head')).toBe('topic');
    expect(sent.query.has('session')).toBe(false);
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
    expect(list.path).toBe('/api/diff');
    expect(list.query.get('proposal')).toBe(proposal.id);
    expect(list.query.has('root')).toBe(false);
    const file = await env.fetch.sent(1);
    expect(file.path).toBe('/api/diff/file');
    expect(file.query.get('proposal')).toBe(proposal.id);
    expect(file.query.get('root')).toBe('/kiln');
    expect(file.query.get('path')).toBe('a.md');
    expect(file.query.has('session')).toBe(false);
  });
});
