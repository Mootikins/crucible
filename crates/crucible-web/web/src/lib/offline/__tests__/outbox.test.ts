import { describe, it, expect, vi, beforeEach } from 'vitest';
import { memoryStore, type OfflineStore } from '@/lib/offline/store';
import {
  conflictCopyPath,
  drainOutbox,
  isQueued,
  queueWrite,
  queuedCount,
  readQueued,
  type OutboxEntry,
  type OutboxSink,
} from '@/lib/offline/outbox';

const DAEMON = 'http://host|/etc/crucible';
const KILN = '/kilns/notes';
const PATH = `${KILN}/Note.md`;

/** A sink that accepts everything. */
const okSink = (): OutboxSink => ({
  write: async () => ({ ok: true, hash: 'h1' }),
  writeConflictCopy: async () => 'copy',
});

let store: OfflineStore;
beforeEach(() => {
  store = memoryStore();
});

const queue = (over: Partial<Extract<OutboxEntry, { kind: 'whole' }>> = {}) =>
  queueWrite(store, {
    kind: 'whole',
    path: PATH,
    body: 'edited on the phone',
    base: 'base-hash',
    kiln: KILN,
    daemon: DAEMON,
    ...over,
  });

function sink(over: Partial<OutboxSink> = {}): OutboxSink {
  return {
    write: async () => ({ ok: true, hash: 'new-hash' }),
    writeConflictCopy: async (entry) => `${entry.path} (conflict)`,
    ...over,
  };
}

describe('queueWrite', () => {
  it('holds the writing until it is sent', async () => {
    await queue();
    expect(await isQueued(store, PATH)).toBe(true);
    expect(await queuedCount(store)).toBe(1);
  });

  // The base is what the user edited FROM. A later write to the same note is
  // still based on that, and moving it would let a drain overwrite whatever
  // the other writer did in between.
  it('keeps the base of the first queued write', async () => {
    await queue({ base: 'first-base' });
    await queue({ base: 'second-base', body: 'edited again' });
    const entry = await store.get<OutboxEntry>('outbox', PATH);
    expect(entry?.base).toBe('first-base');
    expect(entry?.kind === 'whole' && entry.body).toBe('edited again');
  });

  it('a queued anchored entry keeps its kind and edits', async () => {
    const edits = [{ expect: '- [ ] milk', replace: '- [x] milk' }];
    await queueWrite(store, { kind: 'anchored', path: PATH, edits, base: 'h0', kiln: KILN, daemon: DAEMON });
    const held = await readQueued(store, PATH);
    expect(held).toMatchObject({ kind: 'anchored', edits, base: 'h0' });
  });

  /**
   * One entry per note. The first base detects a remote change on drain, so a
   * second write to the same note FOLDS into the queued one instead of taking
   * a second key with a base of its own.
   */
  describe('folds a second write to the same note', () => {
    const tick = { expect: '- [ ] milk', replace: '- [x] milk' };
    const anchored = (edits = [tick], base = 'h1') =>
      queueWrite(store, { kind: 'anchored', path: PATH, edits, base, kiln: KILN, daemon: DAEMON });

    it('folds an anchored edit into a queued whole write and keeps the base', async () => {
      await queue({ body: '- [ ] milk\n- [ ] eggs\n', base: 'h0' });
      const out = await anchored();
      expect(out).toEqual({ ok: true, folded: true });
      expect(await readQueued(store, PATH)).toMatchObject({
        kind: 'whole',
        body: '- [x] milk\n- [ ] eggs\n',
        base: 'h0',
      });
      expect(await queuedCount(store)).toBe(1);
    });

    // The line is not in the queued text, so no fold is honest. The caller
    // gets the index and reverts; nothing is queued for a daemon to refuse.
    it('refuses an anchored edit whose line is not in the queued text', async () => {
      await queue({ body: '- [ ] eggs\n', base: 'h0' });
      const out = await anchored([tick]);
      expect(out).toEqual({ ok: false, index: 0 });
      expect(await readQueued(store, PATH)).toMatchObject({ kind: 'whole', body: '- [ ] eggs\n' });
    });

    it('appends a second anchored edit to a queued anchored entry', async () => {
      const untick = { expect: '- [x] milk', replace: '- [ ] milk' };
      await anchored([tick], 'h0');
      const out = await anchored([untick], 'h1');
      expect(out).toEqual({ ok: true, folded: true });
      expect(await readQueued(store, PATH)).toMatchObject({
        kind: 'anchored',
        edits: [tick, untick],
        base: 'h0',
      });
    });

    // The buffer the whole write came from already holds the tick.
    it('a whole write replaces a queued anchored entry and keeps the base', async () => {
      await anchored([tick], 'h0');
      const out = await queue({ body: '- [x] milk\n', base: 'h1' });
      expect(out).toEqual({ ok: true, folded: false });
      expect(await readQueued(store, PATH)).toMatchObject({
        kind: 'whole',
        body: '- [x] milk\n',
        base: 'h0',
      });
    });

    it('a first write is queued, not folded', async () => {
      expect(await anchored()).toEqual({ ok: true, folded: false });
      expect(await queuedCount(store)).toBe(1);
    });
  });

  // An entry a device queued before entries had a kind is still in its
  // IndexedDB. It was a whole write, because that was the only kind.
  it('an entry with no kind reads as a whole write', async () => {
    await store.put('outbox', PATH, {
      path: PATH,
      body: 'from before',
      base: 'h0',
      kiln: KILN,
      daemon: DAEMON,
      queuedAt: 1,
      sequence: 1,
    });
    const held = await readQueued(store, PATH);
    expect(held).toMatchObject({ kind: 'whole', body: 'from before' });

    const seen: OutboxEntry[] = [];
    await drainOutbox(store, sink({ write: async (e) => { seen.push(e); return { ok: true, hash: 'h1' }; } }), DAEMON);
    expect(seen[0]?.kind, 'the drain reads the same rule').toBe('whole');
  });
});

describe('drainOutbox', () => {
  it('sends what is queued and updates the mirror to match', async () => {
    await queue();
    const result = await drainOutbox(store, sink(), DAEMON);

    expect(result.sent).toBe(1);
    expect(await isQueued(store, PATH)).toBe(false);
    expect(await store.get<{ body: string; hash: string }>('mirror', PATH)).toMatchObject({
      body: 'edited on the phone',
      hash: 'new-hash',
    });
  });

  // An anchored entry carries no body, so the drain has no text to mirror.
  // The mirror keeps what it has until the next online read refreshes it.
  it('leaves the mirror alone after an anchored entry is sent', async () => {
    await store.put('mirror', PATH, { body: '- [ ] milk\n', hash: 'h0', kiln: KILN, mirroredAt: 1 });
    await queueWrite(store, {
      kind: 'anchored',
      path: PATH,
      edits: [{ expect: '- [ ] milk', replace: '- [x] milk' }],
      base: 'h0',
      kiln: KILN,
      daemon: DAEMON,
    });
    const result = await drainOutbox(store, sink(), DAEMON);
    expect(result.sent).toBe(1);
    expect(await isQueued(store, PATH)).toBe(false);
    expect(await store.get('mirror', PATH)).toMatchObject({ body: '- [ ] milk\n', hash: 'h0' });
  });

  // A refused anchored entry has no body to keep, so no conflict copy is
  // written. The refusal is reported on its own, and the entry clears: the
  // daemon answered, and a replay would be refused again.
  it('reports a refused anchored entry and writes no conflict copy', async () => {
    await queueWrite(store, {
      kind: 'anchored',
      path: PATH,
      edits: [{ expect: '- [ ] milk', replace: '- [x] milk' }],
      base: 'h0',
      kiln: KILN,
      daemon: DAEMON,
    });
    const writeConflictCopy = vi.fn(async () => 'copy');
    const result = await drainOutbox(
      store,
      sink({ write: async () => ({ ok: false, refused: true, current: 'h9' }), writeConflictCopy }),
      DAEMON,
    );
    expect(result.refusedEdits).toEqual([PATH]);
    expect(result.conflicted).toEqual([]);
    expect(writeConflictCopy).not.toHaveBeenCalled();
    expect(await isQueued(store, PATH)).toBe(false);
  });

  it('sends in the order the writes were made', async () => {
    const seen: string[] = [];
    await queue({ path: `${KILN}/A.md` });
    await queue({ path: `${KILN}/B.md` });
    await drainOutbox(store, sink({ write: async (e) => { seen.push(e.path); return { ok: true, hash: 'h' }; } }), DAEMON);
    expect(seen).toEqual([`${KILN}/A.md`, `${KILN}/B.md`]);
  });

  it('writes a conflict copy when the note moved on', async () => {
    await queue();
    const result = await drainOutbox(
      store,
      sink({ write: async () => ({ ok: false, current: 'other-hash' }) }),
      DAEMON,
    );
    expect(result.conflicted).toEqual([`${PATH} (conflict)`]);
    expect(await isQueued(store, PATH)).toBe(false);
  });

  // If the copy fails after the entry is cleared, the user's text is gone.
  it('keeps the writing when the conflict copy itself fails', async () => {
    await queue();
    const result = await drainOutbox(
      store,
      sink({
        write: async () => ({ ok: false, current: 'other' }),
        writeConflictCopy: async () => {
          throw new Error('network gone');
        },
      }),
      DAEMON,
    );
    expect(result.failed).toBe(1);
    expect(await isQueued(store, PATH)).toBe(true);
  });

  it('keeps the writing when the network is still down', async () => {
    await queue();
    const result = await drainOutbox(
      store,
      sink({ write: async () => { throw new Error('offline'); } }),
      DAEMON,
    );
    expect(result.failed).toBe(1);
    expect(await isQueued(store, PATH)).toBe(true);
  });

  // A key names a daemon, not a person: draining here would put this user's
  // edits into a different installation's kiln.
  it('never drains into a daemon the writing did not come from', async () => {
    await queue();
    const write = vi.fn();
    const result = await drainOutbox(store, sink({ write }), 'http://host|/somewhere/else');
    expect(write).not.toHaveBeenCalled();
    expect(result.foreign).toBe(1);
    expect(await isQueued(store, PATH)).toBe(true);
  });

  it('drains nothing while the daemon is unknown', async () => {
    await queue();
    const result = await drainOutbox(store, sink(), '');
    expect(result.foreign).toBe(1);
    expect(await isQueued(store, PATH)).toBe(true);
  });
});

describe('conflictCopyPath', () => {
  it('names the copy beside the note, keeping its extension', () => {
    expect(conflictCopyPath(`${KILN}/Release Notes.md`, new Date('2026-09-12T10:00:00Z'))).toBe(
      `${KILN}/Release Notes (conflict, phone, 2026-09-12).md`,
    );
  });

  it('handles a name with no extension', () => {
    expect(conflictCopyPath(`${KILN}/README`, new Date('2026-09-12T10:00:00Z'))).toBe(
      `${KILN}/README (conflict, phone, 2026-09-12)`,
    );
  });

  /**
   * A drain awaits the network. A save during that await replaces the entry
   * at the same key, and removing by path alone deleted the NEWER writing —
   * which existed nowhere else, because the editor had already gone clean.
   */
  it('does not delete a write that arrived while the send was in flight', async () => {
    const store = memoryStore();
    await queueWrite(store, { kind: 'whole', path: PATH, body: 'first', base: 'h0', kiln: KILN, daemon: DAEMON });

    let release: () => void = () => {};
    const inFlight = new Promise<void>((r) => (release = r));
    const sink: OutboxSink = {
      write: async () => {
        await inFlight; // the user saves again while this is out
        return { ok: true, hash: 'h1' };
      },
      writeConflictCopy: async () => 'copy',
    };

    const draining = drainOutbox(store, sink, DAEMON);
    await queueWrite(store, {
      kind: 'whole',
      path: PATH,
      body: 'SECOND EDIT',
      base: 'h0',
      kiln: KILN,
      daemon: DAEMON,
    });
    release();
    const result = await draining;

    const held = await store.get<OutboxEntry>('outbox', PATH);
    expect(held?.kind === 'whole' && held.body, 'the newer writing must survive the older send').toBe(
      'SECOND EDIT',
    );
    expect(result.superseded).toBe(1);
    expect(result.sent).toBe(0);
  });

  it('still clears an entry nothing replaced', async () => {
    const store = memoryStore();
    await queueWrite(store, { kind: 'whole', path: PATH, body: 'only', base: 'h0', kiln: KILN, daemon: DAEMON });
    const result = await drainOutbox(store, okSink(), DAEMON);

    expect(await store.get('outbox', PATH)).toBeNull();
    expect(result).toMatchObject({ sent: 1, superseded: 0 });
  });

  // The outbox survives a reload; a module counter did not, so a write queued
  // after one sorted before writes queued before it.
  it('orders by what is stored, not by a counter that a reload resets', async () => {
    const store = memoryStore();
    await queueWrite(store, { kind: 'whole', path: `${KILN}/a.md`, body: 'a', base: '', kiln: KILN, daemon: DAEMON });
    const first = await store.get<OutboxEntry>('outbox', `${KILN}/a.md`);

    // A fresh page load cannot lower the next sequence below what is held.
    await queueWrite(store, { kind: 'whole', path: `${KILN}/b.md`, body: 'b', base: '', kiln: KILN, daemon: DAEMON });
    const second = await store.get<OutboxEntry>('outbox', `${KILN}/b.md`);

    expect(second!.sequence).toBeGreaterThan(first!.sequence);
  });
});
