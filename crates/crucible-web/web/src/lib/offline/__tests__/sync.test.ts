import { describe, it, expect, vi, beforeEach } from 'vitest';

const net = vi.hoisted(() => ({
  read: vi.fn(),
  save: vi.fn(),
  list: vi.fn(async (_kiln: string) => [] as unknown[]),
  online: true,
}));
vi.mock('@/lib/api', () => ({
  getFileWithHash: (p: string) => net.read(p),
  saveFileContent: (p: string, c: string) => net.save(p, c),
  getFileContent: async () => '',
  listNotes: (k: string) => net.list(k),
  rawFileUrl: (p: string) => `/raw?${p}`,
  getConfig: async () => {
    // The real one is a plain fetch with no service-worker cache, so it
    // THROWS offline. A mock that always resolves hides the whole defect.
    if (!net.online) throw new TypeError('Failed to fetch');
    return { config_root: '/etc/crucible' };
  },
}));

import { memoryStore } from '@/lib/offline/store';
import { keptActions } from '@/lib/offline/kept';
import {
  networkSink,
  networkSource,
  readNote,
  setOfflineStore,
  syncNow,
  warmIdentity,
  writeNote,
} from '@/lib/offline/sync';

const KILN = '/kilns/notes';
const PATH = `${KILN}/Note.md`;

beforeEach(() => {
  localStorage.clear();
  net.read.mockReset();
  net.save.mockReset();
  setOfflineStore(memoryStore());
  keptActions.keep(KILN, 'notes');
});

describe('readNote', () => {
  it('reads from the network and keeps the copy current', async () => {
    net.read.mockResolvedValue({ content: '# Fresh', content_hash: 'h1' });
    const first = await readNote(PATH, KILN);
    expect(first).toMatchObject({ content: '# Fresh', fromMirror: false });

    // The network is gone; the kept copy answers.
    net.read.mockRejectedValue(new Error('offline'));
    const second = await readNote(PATH, KILN);
    expect(second).toMatchObject({ content: '# Fresh', content_hash: 'h1', fromMirror: true });
  });

  // A mirror miss must not hide why the read failed — a 404 is not "offline".
  it('raises the read failure when nothing is kept', async () => {
    net.read.mockRejectedValue(new Error('File not found: Missing.md'));
    await expect(readNote(`${KILN}/Missing.md`, KILN)).rejects.toThrow('File not found');
  });

  // The mirror must not move under writing the daemon has not received.
  it('leaves a queued note alone when the network answers differently', async () => {
    net.read.mockResolvedValue({ content: 'mine', content_hash: 'base' });
    await readNote(PATH, KILN);
    net.save.mockRejectedValue(new Error('Failed to fetch'));
    await writeNote({ path: PATH, body: 'mine, edited', base: 'base', kiln: KILN });

    net.read.mockResolvedValue({ content: 'theirs', content_hash: 'other' });
    await readNote(PATH, KILN);

    net.read.mockRejectedValue(new Error('offline'));
    expect((await readNote(PATH, KILN)).content_hash).toBe('base');
  });
});

describe('writeNote', () => {
  it('saves through the daemon when it answers', async () => {
    net.save.mockResolvedValue(undefined);
    expect(await writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).toEqual({
      queued: false,
    });
  });

  it('queues the writing when the daemon never answered', async () => {
    net.save.mockRejectedValue(new Error('Failed to fetch'));
    expect(await writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).toEqual({
      queued: true,
    });
  });

  // A daemon that ANSWERS with a refusal is not offline writing: queueing it
  // would report a save that can never land and hide the reason.
  it('raises a refusal the daemon answered with, and queues nothing', async () => {
    const refused = Object.assign(new Error('Project files are read-only'), { status: 403 });
    net.save.mockRejectedValue(refused);
    await expect(writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).rejects.toThrow(
      'read-only',
    );
  });

  // Private mode: the save failed AND nothing can hold the writing. Telling a
  // user it is safe would be a lie; the buffer must stay dirty.
  it('raises the save failure when there is nowhere to queue it', async () => {
    setOfflineStore(null); // no store: idbStore() will reach for indexedDB
    net.save.mockRejectedValue(new Error('disk is full')); // no status: never answered
    await expect(writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).rejects.toThrow(
      'disk is full',
    );
  });
});

/**
 * The wire shape, not a convenient one.
 *
 * `GET /api/notes` sends a path RELATIVE to the kiln. The mirror is read by
 * the ABSOLUTE path the editor holds, so a source that forwards the wire's
 * path unchanged stores every note under a key no read asks for — and its
 * own fill read 404s first. The old fixture for this used absolute paths and
 * could not see either failure.
 */
describe('networkSource reads the wire shape', () => {
  it('makes a kiln-relative note path absolute', async () => {
    net.list.mockResolvedValue([
      { name: 'Seed', path: 'Seed.md', title: 'Seed', tags: [] },
      { name: 'Deep', path: 'sub/Deep.md', title: null, tags: [] },
    ]);

    expect((await networkSource.listNotes(KILN)).map((n) => n.path)).toEqual([
      `${KILN}/Seed.md`,
      `${KILN}/sub/Deep.md`,
    ]);
  });

  it('leaves a path that is already absolute alone', async () => {
    net.list.mockResolvedValue([{ name: 'A', path: `${KILN}/A.md`, title: null, tags: [] }]);
    expect((await networkSource.listNotes(KILN))[0].path).toBe(`${KILN}/A.md`);
  });
});

/**
 * The round trip the feature exists for: edit with no network, reconnect,
 * and watch the write land.
 *
 * This is the gate that was missing. `outbox.test.ts` proves the foreign
 * guard refuses a stranger, but it hand-writes `daemon` on every fixture, so
 * it never asked whether the PRODUCER can satisfy the guard it is testing.
 * It could not: the identity came from a live fetch that fails offline, so
 * every queued write was stamped empty and skipped by every drain, forever.
 */
describe('a write queued offline reaches the daemon on reconnect', () => {
  it('drains what it queued, rather than calling it foreign', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    // What the app does while the network is up: the badge warms on mount and
    // on reconnect, and keeping a kiln warms too.
    await warmIdentity();

    net.online = false;
    net.save.mockRejectedValue(new TypeError('Failed to fetch'));
    expect(await writeNote({ path: PATH, body: 'OFFLINE EDIT', base: 'h0', kiln: KILN })).toEqual({
      queued: true,
    });

    net.online = true;
    net.save.mockResolvedValue(undefined);
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });

    const result = await syncNow();
    expect(result.foreign, 'a write this device queued is not from a foreign daemon').toBe(0);
    expect(result.sent).toBe(1);
    expect(net.save).toHaveBeenCalledWith(PATH, 'OFFLINE EDIT');
  });
});

/** An error shaped like the API client's: a status means the daemon answered. */
const answered = (status: number) => Object.assign(new Error(`HTTP ${status}`), { status });

/**
 * `networkSink.write` had no test at all, and it is the whole of the drain.
 *
 * `PUT /api/kiln/file` carries no hash and writes blind, so the compare in
 * here is the only guard there is.
 */
describe('networkSink does not overwrite what it could not read', () => {
  const entry = {
    path: PATH,
    body: 'mine',
    base: 'h0',
    kiln: KILN,
    daemon: 'd',
    queuedAt: Date.now(),
    sequence: 1,
  };

  it('writes when the note still reads as the base it was edited from', async () => {
    net.read.mockResolvedValue({ content: '', content_hash: 'h0' });
    net.save.mockResolvedValue(undefined);
    expect(await networkSink.write(entry)).toEqual({ ok: true, hash: 'h0' });
    expect(net.save).toHaveBeenCalledWith(PATH, 'mine');
  });

  it('conflicts when the note moved on', async () => {
    net.read.mockResolvedValue({ content: '', content_hash: 'MOVED' });
    expect(await networkSink.write(entry)).toEqual({ ok: false, current: 'MOVED' });
    expect(net.save, 'a moved note must not be overwritten').not.toHaveBeenCalled();
  });

  // A deleted note must not come back, and the writing must not vanish with
  // it — so it goes to a conflict copy.
  it('conflicts rather than resurrecting a note that was deleted', async () => {
    net.read.mockRejectedValue(answered(404));
    expect(await networkSink.write(entry)).toEqual({ ok: false, current: '' });
    expect(net.save, 'a deleted note must not be written back').not.toHaveBeenCalled();
  });

  // The defect: a failed read was treated as "no current file", and the whole
  // body was written over whatever was really there.
  it('raises on a read it cannot interpret, leaving the entry queued', async () => {
    net.read.mockRejectedValue(answered(500));
    await expect(networkSink.write(entry)).rejects.toThrow();
    expect(net.save, 'a 500 is not consent to overwrite').not.toHaveBeenCalled();
  });

  it('takes a free name when a conflict copy already exists for today', async () => {
    // The dated name reads back, so it is taken; the numbered one does not.
    net.read
      .mockResolvedValueOnce({ content: '', content_hash: 'x' }) // base name taken
      .mockRejectedValueOnce(answered(404)); // " 2" is free
    net.save.mockResolvedValue(undefined);

    const copy = await networkSink.writeConflictCopy(entry);
    expect(copy).toMatch(/ 2\.md$/);
    expect(net.save).toHaveBeenCalledWith(copy, 'mine');
  });
});

describe('a read prefers writing the daemon has not received', () => {
  it('returns the queued body, not the stale mirror', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    await readNote(PATH, KILN); // the mirror now holds "original"

    net.save.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'MY OFFLINE EDIT', base: 'h0', kiln: KILN });

    net.read.mockRejectedValue(new TypeError('Failed to fetch'));
    const back = await readNote(PATH, KILN);
    expect(back.content, 'the queued edit outranks the mirror').toBe('MY OFFLINE EDIT');
  });
});
