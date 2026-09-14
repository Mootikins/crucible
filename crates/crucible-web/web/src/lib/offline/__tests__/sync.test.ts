import { describe, it, expect, vi, beforeEach } from 'vitest';

const net = vi.hoisted(() => ({
  read: vi.fn(),
  save: vi.fn(),
  guardedSave: vi.fn(),
  patch: vi.fn(),
  list: vi.fn(async (_kiln: string) => [] as unknown[]),
  online: true,
}));
vi.mock('@/lib/api', () => ({
  getFileWithHash: (p: string) => net.read(p),
  saveFileContent: (p: string, c: string) => net.save(p, c),
  saveFileIfUnchanged: (...args: unknown[]) => net.guardedSave(...args),
  patchKilnFile: (p: string, e: unknown, b?: string) => net.patch(p, e, b),
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
import { type OutboxEntry } from '@/lib/offline/outbox';
import {
  editNote,
  networkSink,
  networkSource,
  onNoteConflicted,
  onNoteLanded,
  pendingCount,
  readNote,
  setOfflineStore,
  syncNow,
  warmIdentity,
  writeNote,
} from '@/lib/offline/sync';

// The store the facade holds during a test. `setOfflineStore` is the seam.
let store = memoryStore();
const KILN = '/kilns/notes';
const PATH = `${KILN}/Note.md`;

beforeEach(() => {
  localStorage.clear();
  net.read.mockReset();
  net.save.mockReset();
  net.guardedSave.mockReset();
  net.patch.mockReset();
  store = memoryStore();
  setOfflineStore(store);
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
    net.guardedSave.mockRejectedValue(new Error('Failed to fetch'));
    await writeNote({ path: PATH, body: 'mine, edited', base: 'base', kiln: KILN });

    net.read.mockResolvedValue({ content: 'theirs', content_hash: 'other' });
    await readNote(PATH, KILN);

    net.read.mockRejectedValue(new Error('offline'));
    expect((await readNote(PATH, KILN)).content_hash).toBe('base');
  });
});

describe('writeNote', () => {
  it('saves through the daemon when it answers', async () => {
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h2' });
    expect(await writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).toEqual({
      queued: false,
      stale: false,
      hash: 'h2',
    });
  });

  // The daemon compares the base. A whole write that dropped it wrote blind,
  // and the drain was the only path that let the daemon refuse a stale one.
  it('sends the base it was edited from when the daemon answers', async () => {
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h2' });
    const out = await writeNote({ path: PATH, body: 'new', base: 'h1', kiln: KILN });
    expect(net.guardedSave).toHaveBeenCalledWith(PATH, 'new', 'h1');
    expect(net.save, 'the guarded route is the only save').not.toHaveBeenCalled();
    expect(out).toEqual({ queued: false, stale: false, hash: 'h2' });
  });

  // The daemon answered, so this is not offline writing. The user is present
  // to decide, so nothing is written and nothing is queued.
  it('refuses a stale save online and queues nothing', async () => {
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: 'h9' });
    const out = await writeNote({ path: PATH, body: 'new', base: 'h1', kiln: KILN });
    expect(out).toEqual({ queued: false, stale: true, current: 'h9' });
    expect(await pendingCount()).toBe(0);
    // A blind write after the refusal would pass the two lines above.
    expect(net.save).not.toHaveBeenCalled();
  });

  it('queues the writing when the daemon never answered', async () => {
    net.guardedSave.mockRejectedValue(new Error('Failed to fetch'));
    expect(await writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).toEqual({
      queued: true,
    });
  });

  // A daemon that ANSWERS with a refusal is not offline writing: queueing it
  // would report a save that can never land and hide the reason.
  it('raises a refusal the daemon answered with, and queues nothing', async () => {
    const refused = Object.assign(new Error('Project files are read-only'), { status: 403 });
    net.guardedSave.mockRejectedValue(refused);
    await expect(writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).rejects.toThrow(
      'read-only',
    );
    expect(await pendingCount()).toBe(0);
  });

  // The text the write was made from is the third text a merge needs, and it
  // exists only here. A queued write that dropped it can only be refused.
  it('queues the base text the write was made from', async () => {
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'edited', base: 'h0', baseText: 'original', kiln: KILN });
    const [held] = await store.list<OutboxEntry>('outbox');
    expect(held.value).toMatchObject({ base: 'h0', baseText: 'original', body: 'edited' });
  });

  // Private mode: the save failed AND nothing can hold the writing. Telling a
  // user it is safe would be a lie; the buffer must stay dirty.
  it('raises the save failure when there is nowhere to queue it', async () => {
    setOfflineStore(null); // no store: idbStore() will reach for indexedDB
    net.guardedSave.mockRejectedValue(new Error('disk is full')); // no status: never answered
    await expect(writeNote({ path: PATH, body: 'x', base: 'h', kiln: KILN })).rejects.toThrow(
      'disk is full',
    );
  });
});

/** One tick: the line the user ticked, as the editor had it. */
const TICK = { expect: '- [ ] milk', replace: '- [x] milk' };

describe('editNote', () => {
  it('applies an anchored edit through the daemon when it answers', async () => {
    net.patch.mockResolvedValue({ ok: true, content_hash: 'h2' });
    const out = await editNote({ path: PATH, edits: [TICK], base: 'h1', kiln: KILN });
    expect(net.patch).toHaveBeenCalledWith(PATH, [TICK], 'h1');
    expect(out).toEqual({ queued: false, ok: true, hash: 'h2' });
  });

  // A refusal is an answer. The caller reverts the tick and names the
  // reason; queueing it would replay a refusal forever.
  it('returns the daemon refusal of an anchored edit and queues nothing', async () => {
    net.patch.mockResolvedValue({
      ok: false,
      failed: [{ reason: 'no such line', index: 0 }],
      current_hash: 'h9',
      stale_base: true,
    });
    const out = await editNote({ path: PATH, edits: [TICK], base: 'h1', kiln: KILN });
    expect(out).toMatchObject({ queued: false, ok: false, stale_base: true, current_hash: 'h9' });
    expect(await pendingCount()).toBe(0);
  });

  it('queues an anchored edit when the daemon never answered', async () => {
    net.patch.mockRejectedValue(new TypeError('Failed to fetch'));
    const out = await editNote({ path: PATH, edits: [TICK], base: 'h1', kiln: KILN });
    expect(out).toEqual({ queued: true });
    const [entry] = await store.list<OutboxEntry>('outbox');
    expect(entry.value).toMatchObject({ kind: 'anchored', edits: [TICK], base: 'h1', path: PATH });
  });

  // A whole write is queued for the note, and the ticked line is not in its
  // body. Queueing the tick would send the daemon an edit this device already
  // knows cannot apply. The answer has the refusal's shape, so the caller's
  // revert path runs unchanged.
  it('refuses an edit whose line is not in the queued whole write', async () => {
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: '- [ ] eggs\n', base: 'h0', kiln: KILN });
    net.patch.mockRejectedValue(new TypeError('Failed to fetch'));
    const out = await editNote({ path: PATH, edits: [TICK], base: 'h0', kiln: KILN });
    expect(out).toEqual({
      queued: false,
      ok: false,
      failed: [{ reason: 'the line is not in the queued text', index: 0 }],
      current_hash: '',
      stale_base: false,
    });
    expect(net.patch, 'never sent: the daemon is not answering').toHaveBeenCalledTimes(1);
    expect(await pendingCount()).toBe(1);
  });

  // The same rule as a whole write: a status means the daemon answered.
  it('raises a refusal the daemon answered with, and queues nothing', async () => {
    net.patch.mockRejectedValue(Object.assign(new Error('read-only'), { status: 403 }));
    await expect(editNote({ path: PATH, edits: [TICK], base: 'h1', kiln: KILN })).rejects.toThrow(
      'read-only',
    );
    expect(await pendingCount()).toBe(0);
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
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    expect(await writeNote({ path: PATH, body: 'OFFLINE EDIT', base: 'h0', kiln: KILN })).toEqual({
      queued: true,
    });

    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h1' });

    const result = await syncNow();
    expect(result.foreign, 'a write this device queued is not from a foreign daemon').toBe(0);
    expect(result.sent).toBe(1);
    expect(net.guardedSave).toHaveBeenCalledWith(PATH, 'OFFLINE EDIT', 'h0');
  });

  /** Queue one tick while the daemon is away. */
  async function queueTickOffline() {
    net.read.mockResolvedValue({ content: '- [ ] milk\n', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.patch.mockRejectedValue(new TypeError('Failed to fetch'));
    expect(await editNote({ path: PATH, edits: [TICK], base: 'h0', kiln: KILN })).toEqual({
      queued: true,
    });
    net.online = true;
  }

  // The daemon places the anchor in the current text, and counts an edit
  // already there as applied. A base would refuse every replay after any
  // other change to the note.
  it('replays a queued anchored entry against the current note with no base', async () => {
    await queueTickOffline();
    net.patch.mockResolvedValue({ ok: true, content_hash: 'h1' });

    const result = await syncNow();
    expect(net.patch).toHaveBeenLastCalledWith(PATH, [TICK], undefined);
    expect(result.sent).toBe(1);
    expect(await pendingCount()).toBe(0);
  });

  it('reports an anchored refusal without writing a conflict copy', async () => {
    await queueTickOffline();
    net.patch.mockResolvedValue({
      ok: false,
      failed: [{ reason: 'no such line', index: 0 }],
      current_hash: 'h9',
      stale_base: false,
    });

    const result = await syncNow();
    expect(result.refusedEdits).toEqual([PATH]);
    expect(result.conflicted).toEqual([]);
    expect(net.save, 'there is no body to copy').not.toHaveBeenCalled();
    expect(await pendingCount(), 'the daemon answered; a replay would be refused again').toBe(0);
  });
});

/**
 * A landed write moves the daemon's hash. The editor holds the open buffers
 * and this layer holds the drain, so the drain names each landed write to
 * whoever listens. The editor moves the buffer's base from that.
 */
describe('onNoteLanded', () => {
  it('calls a listener once per landed row with the base and the answered hash', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'a', base: 'h0', kiln: KILN });
    net.patch.mockRejectedValue(new TypeError('Failed to fetch'));
    await editNote({ path: `${KILN}/B.md`, edits: [TICK], base: 'hb', kiln: KILN });
    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h1' });
    net.patch.mockResolvedValue({ ok: true, content_hash: 'hb2' });

    const listener = vi.fn();
    const stop = onNoteLanded(listener);
    await syncNow();
    stop();

    expect(listener).toHaveBeenCalledTimes(2);
    expect(listener).toHaveBeenCalledWith({ path: PATH, base: 'h0', hash: 'h1' });
    expect(listener).toHaveBeenCalledWith({ path: `${KILN}/B.md`, base: 'hb', hash: 'hb2' });
  });

  it('never calls a listener that unsubscribed', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'a', base: 'h0', kiln: KILN });
    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h1' });

    const gone = vi.fn();
    const kept = vi.fn();
    onNoteLanded(gone)();
    const stop = onNoteLanded(kept);
    await syncNow();
    stop();

    expect(gone).not.toHaveBeenCalled();
    expect(kept).toHaveBeenCalledTimes(1);
  });

  it('a listener that throws does not stop the others or the sync', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'a', base: 'h0', kiln: KILN });
    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h1' });
    const quiet = vi.spyOn(console, 'error').mockImplementation(() => {});

    const throws = vi.fn(() => {
      throw new Error('listener broke');
    });
    const after = vi.fn();
    const stopThrows = onNoteLanded(throws);
    const stopAfter = onNoteLanded(after);
    const result = await syncNow();
    stopThrows();
    stopAfter();
    quiet.mockRestore();

    expect(result.sent).toBe(1);
    expect(throws).toHaveBeenCalledTimes(1);
    expect(after).toHaveBeenCalledWith({ path: PATH, base: 'h0', hash: 'h1' });
  });

  it('calls no listener when nothing landed', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'a', base: 'h0', kiln: KILN });
    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: 'h9' });
    net.read.mockRejectedValue(answered(404));
    net.save.mockResolvedValue(undefined);

    const listener = vi.fn();
    const stop = onNoteLanded(listener);
    await syncNow();
    stop();

    expect(listener).not.toHaveBeenCalled();
  });
});

/** An error shaped like the API client's: a status means the daemon answered. */
const answered = (status: number) => Object.assign(new Error(`HTTP ${status}`), { status });

/**
 * A drained conflict clears the entry and writes a copy, and nothing else on
 * screen says so. The editor holds the open buffers, so the drain names each
 * conflicted write to whoever listens, with the base the entry was made from.
 */
describe('onNoteConflicted', () => {
  it('calls a listener once per conflicted row with the base and the copy', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'mine', base: 'h0', kiln: KILN });
    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: 'h9' });
    net.read.mockRejectedValue(answered(404)); // the copy's name is free
    net.save.mockResolvedValue(undefined);

    const listener = vi.fn();
    const stop = onNoteConflicted(listener);
    const result = await syncNow();
    stop();

    expect(result.conflicted).toHaveLength(1);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(
      expect.objectContaining({
        path: PATH,
        base: 'h0',
        copy: expect.stringMatching(/\/Note \(conflict, phone, \d{4}-\d{2}-\d{2}\)\.md$/),
      }),
    );
    expect(net.save).toHaveBeenCalledWith(listener.mock.calls[0][0].copy, 'mine');
  });

  it('never calls a listener that unsubscribed', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    net.online = false;
    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'mine', base: 'h0', kiln: KILN });
    net.online = true;
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: 'h9' });
    net.read.mockRejectedValue(answered(404));
    net.save.mockResolvedValue(undefined);

    const gone = vi.fn();
    onNoteConflicted(gone)();
    await syncNow();

    expect(gone).not.toHaveBeenCalled();
  });
});

/**
 * `networkSink.write` had no test at all, and it is the whole of the drain.
 *
 * `PUT /api/kiln/file` carries no hash and writes blind, so the compare in
 * here is the only guard there is.
 */
/**
 * The sink sends the base and lets the DAEMON compare it.
 *
 * It used to read the note, compare in the browser, then PUT — three round
 * trips with a window in the middle, which is what section 11 of the mobile
 * note forbids. The compare now lives inside the route's write.
 */
describe('networkSink lets the daemon refuse a stale write', () => {
  const entry: OutboxEntry = {
    kind: 'whole',
    path: PATH,
    body: 'mine',
    base: 'h0',
    kiln: KILN,
    daemon: 'd',
    queuedAt: Date.now(),
    sequence: 1,
  };

  it('sends the base it was edited from, and never reads first', async () => {
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h1' });
    expect(await networkSink.write(entry)).toEqual({ ok: true, hash: 'h1' });
    expect(net.guardedSave).toHaveBeenCalledWith(PATH, 'mine', 'h0', undefined);
    expect(net.read, 'the daemon compares; the browser must not').not.toHaveBeenCalled();
  });

  it('takes the hash the route answered with, rather than reading it back', async () => {
    // A third read to learn the hash could land after ANOTHER writer, storing
    // a hash that described someone else's bytes beside this body.
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'written' });
    const answer = await networkSink.write(entry);
    expect(answer).toEqual({ ok: true, hash: 'written' });
    expect(net.read).not.toHaveBeenCalled();
  });

  it('conflicts when the daemon says the note moved on', async () => {
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: 'MOVED' });
    expect(await networkSink.write(entry)).toEqual({ ok: false, current: 'MOVED' });
  });

  // The daemon answers an empty hash when the file is gone. The writing is
  // still the user's, so it becomes a conflict copy rather than resurrecting
  // a note that was deleted.
  it('conflicts rather than resurrecting a note that was deleted', async () => {
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: '' });
    expect(await networkSink.write(entry)).toEqual({ ok: false, current: '' });
  });

  it('raises anything else, leaving the entry queued', async () => {
    net.guardedSave.mockRejectedValue(answered(500));
    await expect(networkSink.write(entry)).rejects.toThrow();
  });

  const anchored: OutboxEntry = { ...entry, kind: 'anchored', edits: [TICK] };

  it('replays an anchored entry through the patch route, with no base', async () => {
    net.patch.mockResolvedValue({ ok: true, content_hash: 'h1' });
    expect(await networkSink.write(anchored)).toEqual({ ok: true, hash: 'h1' });
    expect(net.patch).toHaveBeenCalledWith(PATH, [TICK], undefined);
    expect(net.guardedSave, 'an anchored entry has no body to PUT').not.toHaveBeenCalled();
  });

  it('answers a refusal when the daemon cannot place the anchored edit', async () => {
    net.patch.mockResolvedValue({
      ok: false,
      failed: [{ reason: 'no such line', index: 0 }],
      current_hash: 'h9',
      stale_base: false,
    });
    expect(await networkSink.write(anchored)).toEqual({ ok: false, refused: true, current: 'h9' });
  });

  it('takes a free name when a conflict copy already exists for today', async () => {
    net.read
      .mockResolvedValueOnce({ content: '', content_hash: 'x' }) // dated name taken
      .mockRejectedValueOnce(answered(404)); // the numbered one is free
    net.save.mockResolvedValue(undefined);

    const copy = await networkSink.writeConflictCopy(entry);
    expect(copy).toMatch(/ 2\.md$/);
    expect(net.save).toHaveBeenCalledWith(copy, 'mine');
  });
});

/**
 * The base text is what makes a stale write mergeable, and this device is the
 * only party that holds it.
 */
describe('networkSink merges a stale write when it is given the base text', () => {
  const entry: OutboxEntry = {
    kind: 'whole',
    path: PATH,
    body: 'mine',
    base: 'h0',
    baseText: 'was',
    kiln: KILN,
    daemon: 'd',
    queuedAt: Date.now(),
    sequence: 1,
  };

  it('sends the base text with a retried whole write', async () => {
    net.guardedSave.mockResolvedValue({ ok: true, content_hash: 'h2', merged: true, content: 'MERGED' });
    expect(await networkSink.write(entry, { baseText: 'was' })).toEqual({
      ok: true,
      hash: 'h2',
      merged: true,
      content: 'MERGED',
    });
    expect(net.guardedSave).toHaveBeenCalledWith(PATH, 'mine', 'h0', 'was');
  });

  it('hands back the regions the daemon could not settle', async () => {
    net.guardedSave.mockResolvedValue({
      ok: false,
      current_hash: 'h9',
      current_content: 'theirs',
      merged_content: 'mine',
      regions: [{ start_line: 1, end_line: 2, base: 'was', ours: 'mine', theirs: 'theirs' }],
    });
    expect(await networkSink.write(entry, { baseText: 'was' })).toEqual({
      ok: false,
      current: 'h9',
      currentContent: 'theirs',
      mergedContent: 'mine',
      regions: [{ start_line: 1, end_line: 2, base: 'was', ours: 'mine', theirs: 'theirs' }],
    });
  });

  // A write with no base text is refused, not merged, and the refusal carries
  // only a hash. The drain still has to say what the other writer put there.
  it('reads the note for the current text when there was no base text to merge with', async () => {
    net.guardedSave.mockResolvedValue({ ok: false, current_hash: 'h9' });
    net.read.mockResolvedValue({ content: 'THEIRS', content_hash: 'h9' });
    expect(await networkSink.write(entry)).toEqual({
      ok: false,
      current: 'h9',
      currentContent: 'THEIRS',
    });
  });
});

describe('a read prefers writing the daemon has not received', () => {
  it('returns the queued body, not the stale mirror', async () => {
    net.read.mockResolvedValue({ content: 'original', content_hash: 'h0' });
    await warmIdentity();
    await readNote(PATH, KILN); // the mirror now holds "original"

    net.guardedSave.mockRejectedValue(new TypeError('Failed to fetch'));
    await writeNote({ path: PATH, body: 'MY OFFLINE EDIT', base: 'h0', kiln: KILN });

    net.read.mockRejectedValue(new TypeError('Failed to fetch'));
    const back = await readNote(PATH, KILN);
    expect(back.content, 'the queued edit outranks the mirror').toBe('MY OFFLINE EDIT');
  });

  // An anchored entry holds no body. The best text there is, is the mirror
  // with the queued edit folded in: that is what the user will see once the
  // drain lands it.
  it('folds a queued anchored edit into the mirror text', async () => {
    net.read.mockResolvedValue({ content: '- [ ] milk\n- [ ] eggs\n', content_hash: 'h0' });
    await warmIdentity();
    await readNote(PATH, KILN);

    net.patch.mockRejectedValue(new TypeError('Failed to fetch'));
    await editNote({ path: PATH, edits: [TICK], base: 'h0', kiln: KILN });

    net.read.mockRejectedValue(new TypeError('Failed to fetch'));
    const back = await readNote(PATH, KILN);
    expect(back).toEqual({ content: '- [x] milk\n- [ ] eggs\n', content_hash: 'h0', fromMirror: true });
  });

  it('answers the mirror text when a queued edit no longer applies to it', async () => {
    net.read.mockResolvedValue({ content: '- [ ] eggs\n', content_hash: 'h0' });
    await warmIdentity();
    await readNote(PATH, KILN);

    net.patch.mockRejectedValue(new TypeError('Failed to fetch'));
    await editNote({ path: PATH, edits: [TICK], base: 'h0', kiln: KILN });

    net.read.mockRejectedValue(new TypeError('Failed to fetch'));
    expect((await readNote(PATH, KILN)).content).toBe('- [ ] eggs\n');
  });
});
