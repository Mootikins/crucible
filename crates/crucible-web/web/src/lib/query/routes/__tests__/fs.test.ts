import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { QueryKey } from '@tanstack/solid-query';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { onlyEventSource, installFakeEventSource } from '@/test-utils/sse';
import { keys } from '../../keys';
import { fsEvents } from '../../sse';
import { installFsEventRoute } from '../fs';

let env: TestQueryEnv;
let invalidated: QueryKey[];
let stop: (() => void) | null = null;

/**
 * Opens the filesystem stream and answers the source the route reads.
 *
 * The route runs inside the stream, not beside it, so every case drives it the
 * way the daemon does: one frame on the wire.
 */
function openStream() {
  stop = fsEvents().subscribe(() => {});
  return onlyEventSource();
}

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv();
  installFsEventRoute();
  invalidated = [];
  // Only the calls that NAME a key. The route also invalidates by predicate,
  // for the entries whose key it cannot spell from a path alone; those are
  // driven against the real cache in the second block below, because a spy
  // never runs the predicate that decides what they reach.
  vi.spyOn(env.client, 'invalidateQueries').mockImplementation((filters) => {
    if (filters?.queryKey) invalidated.push(filters.queryKey as QueryKey);
    return Promise.resolve();
  });
});

afterEach(() => {
  stop?.();
  stop = null;
  vi.restoreAllMocks();
  env.restore();
});

describe('the fs event route', () => {
  // The bytes moved, so a reader of the file is holding the text from before
  // them; the folder moved too, because a listing carries a size and a
  // modified time.
  it('invalidates the file and its folder when a file changes', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', path: '/kiln/notes/a.md', kind: 'modified' });

    expect(invalidated).toEqual([keys.fsFile('/kiln/notes/a.md'), keys.fsDir('/kiln/notes')]);
  });

  // A created file is the same two keys: the folder gained a row, and a reader
  // that asked for the path before it existed holds the absence.
  it('invalidates the file and its folder when a file is created', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', path: '/kiln/notes/new.md', kind: 'created' });

    expect(invalidated).toEqual([keys.fsFile('/kiln/notes/new.md'), keys.fsDir('/kiln/notes')]);
  });

  it('invalidates the file and its folder when a file is deleted', () => {
    const source = openStream();

    source.emit('fs_deleted', { type: 'deleted', path: '/kiln/notes/gone.md' });

    expect(invalidated).toEqual([keys.fsFile('/kiln/notes/gone.md'), keys.fsDir('/kiln/notes')]);
  });

  // A move names both ends, and both folders lost or gained a row by it.
  it('invalidates both ends of a move and both folders', () => {
    const source = openStream();

    source.emit('fs_moved', {
      type: 'moved',
      from: '/kiln/notes/a.md',
      to: '/kiln/archive/a.md',
    });

    expect(invalidated).toEqual([
      keys.fsFile('/kiln/notes/a.md'),
      keys.fsFile('/kiln/archive/a.md'),
      keys.fsDir('/kiln/notes'),
      keys.fsDir('/kiln/archive'),
    ]);
  });

  // A rename in place has one folder, and it is asked for once. Twice would
  // be two refetches of one listing for one event.
  it('invalidates one folder once when a rename stays in it', () => {
    const source = openStream();

    source.emit('fs_moved', { type: 'moved', from: '/kiln/notes/a.md', to: '/kiln/notes/b.md' });

    expect(invalidated).toEqual([
      keys.fsFile('/kiln/notes/a.md'),
      keys.fsFile('/kiln/notes/b.md'),
      keys.fsDir('/kiln/notes'),
    ]);
  });

  // **The negative.** A path directly under the root has a folder too, and it
  // is the root. An empty string there would name a folder nothing lists.
  it('names the root as the folder of a path directly under it', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', path: '/a.md', kind: 'modified' });

    expect(invalidated).toEqual([keys.fsFile('/a.md'), keys.fsDir('/')]);
  });

  it('writes nothing for a frame it cannot read', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', kind: 'modified' });

    expect(invalidated).toEqual([]);
  });
});

/**
 * A markdown file is a NOTE, and the kiln's note entries are wrong when one
 * appears, moves or goes.
 *
 * These cases drive the REAL cache rather than a spy, because what is under
 * test is which held entries the route reaches — and a spy over
 * `invalidateQueries` never runs the predicate that decides.
 */
describe('the fs event route and the notes of a kiln', () => {
  const KILN = '/kiln';

  /** Puts a note index and a resolved MISS in the cache, both fresh. */
  function hold(): void {
    env.client.setQueryData(keys.notesList(KILN), []);
    env.client.setQueryData(keys.notesResolve(KILN, 'ghost'), null);
    env.client.setQueryData(keys.notesList('/other'), []);
  }

  const isStale = (key: QueryKey) =>
    env.client.getQueryCache().find({ queryKey: key })!.isStale();

  beforeEach(() => {
    // The default spy answers every invalidation without performing it.
    vi.restoreAllMocks();
    hold();
  });

  /**
   * The failure this exists for: a link written before the note it names is
   * resolved to nothing, that miss is held, and nothing else was ever going to
   * drop it. The link read as broken for five minutes after the note was on
   * disk.
   */
  it('drops the index and the held misses of the kiln a note was written in', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', path: `${KILN}/Ghost.md`, kind: 'created' });

    expect(isStale(keys.notesList(KILN))).toBe(true);
    expect(isStale(keys.notesResolve(KILN, 'ghost'))).toBe(true);
  });

  it('drops them at both ends of a move', () => {
    env.client.setQueryData(keys.notesList('/other'), []);
    const source = openStream();

    source.emit('fs_moved', { type: 'moved', from: `${KILN}/a.md`, to: '/other/a.md' });

    expect(isStale(keys.notesList(KILN))).toBe(true);
    expect(isStale(keys.notesList('/other'))).toBe(true);
  });

  // The negative, twice over: a file that is not a note changes no note
  // entry, and a note in one vault changes nothing about another.
  it('leaves the notes of a kiln alone when the file is not a note', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', path: `${KILN}/src/main.rs`, kind: 'modified' });

    expect(isStale(keys.notesList(KILN))).toBe(false);
    expect(isStale(keys.notesResolve(KILN, 'ghost'))).toBe(false);
  });

  it('leaves another kiln alone', () => {
    const source = openStream();

    source.emit('fs_changed', { type: 'changed', path: `${KILN}/Ghost.md`, kind: 'created' });

    expect(isStale(keys.notesList('/other'))).toBe(false);
  });
});
