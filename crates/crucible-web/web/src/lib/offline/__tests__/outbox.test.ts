import { describe, it, expect, vi, beforeEach } from 'vitest';
import { memoryStore, type OfflineStore } from '@/lib/offline/store';
import {
  conflictCount,
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
    ...over,
  };
}

/** One tick, as the reading view makes it. */
const TICK = { expect: '- [ ] milk', replace: '- [x] milk' };

/**
 * A daemon that refuses a write with no base text and merges one with it.
 *
 * `calls` records what each attempt carried, because the whole point is that
 * the SECOND attempt carries the text this device edited from.
 */
function mergingSink(merged: string) {
  const calls: { entry: OutboxEntry; opts?: { baseText: string } }[] = [];
  const sink: OutboxSink = {
    write: async (entry, opts) => {
      calls.push({ entry, opts });
      if (!opts) return { ok: false, current: 'their-hash' };
      return { ok: true, hash: 'merged-hash', merged: true, content: merged };
    },
  };
  return { sink, calls };
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

    // The daemon anchors every edit of a batch on the ORIGINAL text, and it
    // counts an edit whose `replace` is already there as applied. A queued
    // [tick, untick] replays as a tick. So the second edit composes into the
    // first, and a round trip to the original text leaves nothing to send.
    it('a tick then an untick queues no edit', async () => {
      const untick = { expect: '- [x] milk', replace: '- [ ] milk' };
      await anchored([tick], 'h0');
      const out = await anchored([untick], 'h1');
      expect(out).toEqual({ ok: true, folded: true });
      expect(await readQueued(store, PATH)).toBeNull();
      expect(await queuedCount(store)).toBe(0);
    });

    it('a tick then a different edit queues two', async () => {
      const eggs = { expect: '- [ ] eggs', replace: '- [x] eggs' };
      await anchored([tick], 'h0');
      const out = await anchored([eggs], 'h1');
      expect(out).toEqual({ ok: true, folded: true });
      expect(await readQueued(store, PATH)).toMatchObject({
        kind: 'anchored',
        edits: [tick, eggs],
        base: 'h0',
      });
    });

    // The second edit expects the line the first one wrote. The daemon never
    // sees that line, so the queue holds one edit from the original line to
    // the last text.
    it('a tick then a retick of the same line composes to one', async () => {
      const retick = { expect: '- [x] milk', replace: '- [x] oat milk' };
      await anchored([tick], 'h0');
      const out = await anchored([retick], 'h1');
      expect(out).toEqual({ ok: true, folded: true });
      expect(await readQueued(store, PATH)).toMatchObject({
        kind: 'anchored',
        edits: [{ expect: '- [ ] milk', replace: '- [x] oat milk' }],
        base: 'h0',
      });
    });

    // An arriving edit with no occurrence names a line that is unique in the
    // buffer. When exactly one held edit wrote that line, the held edit is the
    // one it continues, whatever occurrence the held edit named.
    it('an untick of a unique line composes with the held edit that wrote it', async () => {
      const second = { expect: '- [ ] milk', replace: '- [x] milk', occurrence: 1 };
      const untick = { expect: '- [x] milk', replace: '- [ ] milk' };
      await anchored([second], 'h0');
      await anchored([untick], 'h1');
      expect(await readQueued(store, PATH)).toBeNull();
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

  // A refused anchored entry has no body to keep, so there is nothing to hold
  // as a conflict. The refusal is reported on its own, and the entry clears:
  // the daemon answered, and a replay would be refused again.
  it('reports a refused anchored entry and leaves no conflict', async () => {
    await queueWrite(store, {
      kind: 'anchored',
      path: PATH,
      edits: [{ expect: '- [ ] milk', replace: '- [x] milk' }],
      base: 'h0',
      kiln: KILN,
      daemon: DAEMON,
    });
    const result = await drainOutbox(
      store,
      sink({ write: async () => ({ ok: false, refused: true, current: 'h9' }) }),
      DAEMON,
    );
    expect(result.refusedEdits).toEqual([PATH]);
    expect(result.conflicted).toEqual([]);
    expect(await conflictCount(store)).toBe(0);
    expect(await isQueued(store, PATH)).toBe(false);
  });

  /**
   * A write that lands changes the daemon's hash. An open buffer whose base
   * is the entry's base must learn the new hash, or its next save is refused
   * as stale for the user's own tick. The drain names each landed write.
   */
  describe('names each write the daemon accepted', () => {
    it('reports a landed whole write with its base and the answered hash', async () => {
      await queue({ base: 'h0' });
      const result = await drainOutbox(store, sink({ write: async () => ({ ok: true, hash: 'h1' }) }), DAEMON);
      expect(result.landed).toEqual([{ path: PATH, base: 'h0', hash: 'h1' }]);
    });

    it('reports a landed anchored entry the same way', async () => {
      await queueWrite(store, {
        kind: 'anchored',
        path: PATH,
        edits: [{ expect: '- [ ] milk', replace: '- [x] milk' }],
        base: 'h0',
        kiln: KILN,
        daemon: DAEMON,
      });
      const result = await drainOutbox(store, sink({ write: async () => ({ ok: true, hash: 'h1' }) }), DAEMON);
      expect(result.landed).toEqual([{ path: PATH, base: 'h0', hash: 'h1' }]);
    });

    it('reports nothing for a refused, conflicted or failed entry', async () => {
      await queue({ path: `${KILN}/A.md` });
      await queue({ path: `${KILN}/B.md` });
      await queueWrite(store, {
        kind: 'anchored',
        path: `${KILN}/C.md`,
        edits: [{ expect: '- [ ] milk', replace: '- [x] milk' }],
        base: 'h0',
        kiln: KILN,
        daemon: DAEMON,
      });
      const result = await drainOutbox(
        store,
        sink({
          write: async (e) => {
            if (e.path.endsWith('A.md')) return { ok: false, current: 'other' };
            if (e.path.endsWith('B.md')) throw new Error('offline');
            return { ok: false, refused: true, current: 'h9' };
          },
        }),
        DAEMON,
      );
      expect(result.landed).toEqual([]);
    });
  });

  it('sends in the order the writes were made', async () => {
    const seen: string[] = [];
    await queue({ path: `${KILN}/A.md` });
    await queue({ path: `${KILN}/B.md` });
    await drainOutbox(store, sink({ write: async (e) => { seen.push(e.path); return { ok: true, hash: 'h' }; } }), DAEMON);
    expect(seen).toEqual([`${KILN}/A.md`, `${KILN}/B.md`]);
  });

  // The note moved on and nothing could be merged. The writing is the user's
  // and exists only here, so it STAYS, marked as a conflict for a person to
  // settle. It used to be copied to a second note and cleared.
  it('keeps a stale write as a conflict when the note moved on', async () => {
    await queue();
    const result = await drainOutbox(
      store,
      sink({ write: async () => ({ ok: false, current: 'other-hash', currentContent: 'theirs\n' }) }),
      DAEMON,
    );
    expect(result.conflicted[0]).toMatchObject({ path: PATH, base: 'base-hash', kiln: KILN });
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

/**
 * A stale write is MERGED, not lost.
 *
 * The daemon holds the note and the other writer's text; this device holds
 * the text the write was made from. All three meet only when the browser
 * sends its base text, so the drain retries with it.
 */
describe('a stale write is retried with the base text it was made from', () => {
  it('a stale whole write is retried with its base text and lands when the merge is clean', async () => {
    await queue({ body: 'A\nB\nC\nD\n', base: 'h0', baseText: 'A\nB\nC\n' });
    const { sink, calls } = mergingSink('A\nB2\nC\nD\n');

    const result = await drainOutbox(store, sink, DAEMON);

    expect(calls.map((c) => c.opts)).toEqual([undefined, { baseText: 'A\nB\nC\n' }]);
    expect(result.sent).toBe(1);
    expect(result.landed).toEqual([{ path: PATH, base: 'h0', hash: 'merged-hash' }]);
    expect(result.conflicted).toEqual([]);
    expect(await isQueued(store, PATH)).toBe(false);
    // The daemon wrote the MERGED text, so that is what this device now has.
    expect(await store.get<{ body: string; hash: string }>('mirror', PATH)).toMatchObject({
      body: 'A\nB2\nC\nD\n',
      hash: 'merged-hash',
    });
  });

  // An anchored entry has no body. Its edits anchor in the text they were made
  // from, so that text plus the edits IS our whole note.
  it('a stale anchored entry is retried as a whole write made from its base text', async () => {
    await queueWrite(store, {
      kind: 'anchored',
      path: PATH,
      edits: [TICK],
      base: 'h0',
      baseText: '- [ ] milk\n- [ ] eggs\n',
      kiln: KILN,
      daemon: DAEMON,
    });
    const { sink, calls } = mergingSink('- [x] milk\n- [ ] eggs\n');

    const result = await drainOutbox(store, sink, DAEMON);

    expect(calls[0].entry.kind, 'the first attempt is still the anchored replay').toBe('anchored');
    expect(calls[1].entry).toMatchObject({ kind: 'whole', body: '- [x] milk\n- [ ] eggs\n' });
    expect(calls[1].opts).toEqual({ baseText: '- [ ] milk\n- [ ] eggs\n' });
    expect(result.sent).toBe(1);
    expect(result.refusedEdits).toEqual([]);
  });

  // The daemon merged and both sides had changed one span. Nothing is written
  // and the regions come back, because the server never picks a writer.
  it('a retry whose merge leaves regions is reported with them', async () => {
    await queue({ body: 'A\nMINE\n', base: 'h0', baseText: 'A\nB\n' });
    const result = await drainOutbox(
      store,
      sink({
        write: async (_entry, opts) =>
          opts
            ? {
                ok: false,
                current: 'h9',
                currentContent: 'A\nTHEIRS\n',
                mergedContent: 'A\nMINE\n',
                regions: [{ start_line: 2, end_line: 3, base: 'B\n', ours: 'MINE\n', theirs: 'THEIRS\n' }],
              }
            : { ok: false, current: 'h9' },
      }),
      DAEMON,
    );

    expect(result.sent).toBe(0);
    expect(result.conflicted[0]).toMatchObject({
      path: PATH,
      base: 'h0',
      currentHash: 'h9',
      currentContent: 'A\nTHEIRS\n',
      mergedContent: 'A\nMINE\n',
      regions: [{ start_line: 2, end_line: 3, ours: 'MINE\n', theirs: 'THEIRS\n' }],
    });
  });

  /**
   * An entry a device queued before writes kept their base text.
   *
   * Nothing here can merge it, and overwriting the other writer is not an
   * answer either. It becomes a conflict over the whole note, and the text is
   * still the user's.
   */
  it('an entry stored without a base text is reported as a conflict over the whole note, not dropped and not merged', async () => {
    await queue({ body: 'mine\nand more\n', base: 'h0' });
    const seen: (undefined | { baseText: string })[] = [];
    const result = await drainOutbox(
      store,
      sink({
        write: async (_entry, opts) => {
          seen.push(opts);
          return { ok: false, current: 'h9', currentContent: 'theirs\n' };
        },
      }),
      DAEMON,
    );

    expect(seen, 'there is nothing to merge from, so nothing is retried').toEqual([undefined]);
    expect(result.conflicted).toHaveLength(1);
    expect(result.conflicted[0]).toMatchObject({
      path: PATH,
      currentHash: 'h9',
      currentContent: 'theirs\n',
      mergedContent: 'mine\nand more\n',
      regions: [
        { start_line: 1, end_line: 3, base: '', ours: 'mine\nand more\n', theirs: 'theirs\n' },
      ],
    });
  });

  // The base hash and the base text are ONE fact. The first queue wins the
  // hash, so it must win the text: pairing the first hash with a later text
  // asks the daemon to merge from a text that is not the base it names.
  it('a folded write keeps the base text of the first queued write', async () => {
    await queue({ body: 'one', base: 'h0', baseText: 'ZERO' });
    await queue({ body: 'two', base: 'h1', baseText: 'ONE' });
    expect(await readQueued(store, PATH)).toMatchObject({ base: 'h0', baseText: 'ZERO', body: 'two' });
  });
});

/**
 * A conflict is an ENTRY, not a copy of the note under another name.
 *
 * A copy moved the user's text out of the note they wrote it in, cleared the
 * queue and left them to reconcile two files by hand. The writing stays where
 * it is, marked, with the three texts a person needs to settle it.
 */
describe('a merge the daemon could not settle stays queued as a conflict', () => {
  const REGION = { start_line: 2, end_line: 3, base: 'B\n', ours: 'MINE\n', theirs: 'THEIRS\n' };

  /** A daemon that refuses the first attempt and answers regions to the retry. */
  const regionSink = (seen: string[] = []): OutboxSink => ({
    write: async (entry, opts) => {
      seen.push(entry.path);
      return opts
        ? {
            ok: false,
            current: 'h9',
            currentContent: 'A\nTHEIRS\n',
            mergedContent: 'A\nMINE\n',
            regions: [REGION],
          }
        : { ok: false, current: 'h9' };
    },
  });

  it('a stale write with regions stays queued as conflicted and writes no copy', async () => {
    await queue({ body: 'A\nMINE\n', base: 'h0', baseText: 'A\nB\n' });
    const seen: string[] = [];

    const result = await drainOutbox(store, regionSink(seen), DAEMON);

    expect(result.conflicted).toEqual([
      {
        path: PATH,
        base: 'h0',
        kiln: KILN,
        currentHash: 'h9',
        currentContent: 'A\nTHEIRS\n',
        mergedContent: 'A\nMINE\n',
        regions: [REGION],
      },
    ]);
    // Two attempts on ONE note, and nothing written beside it.
    expect(seen).toEqual([PATH, PATH]);
    expect(await store.get<OutboxEntry>('outbox', PATH)).toMatchObject({
      state: 'conflicted',
      body: 'A\nMINE\n',
      currentHash: 'h9',
      currentContent: 'A\nTHEIRS\n',
      mergedContent: 'A\nMINE\n',
      regions: [REGION],
    });
  });

  // A conflict waits on a person. Counting it as an unsent edit would tell
  // the user the network still owes them a send that will never happen.
  it('a conflict is counted apart from what is still owed to the daemon', async () => {
    await queue({ body: 'A\nMINE\n', base: 'h0', baseText: 'A\nB\n' });
    await queue({ path: `${KILN}/Other.md` });

    await drainOutbox(
      store,
      sink({
        write: async (entry, opts) => {
          if (entry.path !== PATH) throw new Error('offline');
          return regionSink().write(entry, opts);
        },
      }),
      DAEMON,
    );

    expect(await conflictCount(store)).toBe(1);
    expect(await queuedCount(store), 'the other note is still owed to the daemon').toBe(1);
  });

  it('a conflicted entry is skipped by the next drain', async () => {
    await queue({ body: 'A\nMINE\n', base: 'h0', baseText: 'A\nB\n' });
    await drainOutbox(store, regionSink(), DAEMON);

    const seen: string[] = [];
    const again = await drainOutbox(store, sink({ write: async (e) => { seen.push(e.path); return { ok: true, hash: 'h1' }; } }), DAEMON);

    expect(seen, 'a conflict is settled by a person, not by another send').toEqual([]);
    expect(again).toMatchObject({ sent: 0, conflicted: [], failed: 0 });
    expect(await conflictCount(store)).toBe(1);
  });

  // The conflict's own base is the one the daemon already refused. A write
  // arriving for the note is a NEW write from a text the user has in front of
  // them, so it replaces the conflict with its own base.
  it('a write arriving for a conflicted note replaces it with its own base', async () => {
    await queue({ body: 'A\nMINE\n', base: 'h0', baseText: 'A\nB\n' });
    await drainOutbox(store, regionSink(), DAEMON);

    await queue({ body: 'settled\n', base: 'h9', baseText: 'A\nTHEIRS\n' });

    expect(await readQueued(store, PATH)).toMatchObject({
      body: 'settled\n',
      base: 'h9',
      baseText: 'A\nTHEIRS\n',
    });
    expect(await conflictCount(store)).toBe(0);
    expect(await queuedCount(store)).toBe(1);
  });
});

describe('an entry clears only when it is still the one that was sent', () => {
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
