import type { AnchoredEdit } from '@/lib/api';
import { applyAnchoredEdits } from '@/lib/offline/fold';
import type { OfflineStore } from '@/lib/offline/store';
import { sameDaemon } from '@/lib/offline/identity';
import type { MirroredNote } from '@/lib/offline/mirror';

/**
 * What this device wrote and the daemon has not received.
 *
 * The mirror is a copy of notes the daemon still has, so losing it costs
 * nothing. The outbox is the user's writing and exists ONLY here, so every
 * rule below is about not losing it:
 *
 * - A queued entry's `base` is immutable, and a mirror refresh skips a path
 *   the outbox holds. Otherwise: edit offline, reconnect, open the note (the
 *   mirror corrects to the other writer's text), drain against the corrected
 *   base, and the other writer's change disappears with no conflict copy.
 * - An entry is stamped with the daemon it came from and never drains into
 *   another. A key names a daemon, not a person.
 * - An entry clears only after its write lands — including a conflict copy.
 */

/** What every queued write names: which note, from which text, for which daemon. */
export type NoteWrite = {
  path: string;
  /** The disk hash it was edited FROM. Never updated once queued. */
  base: string;
  kiln: string;
  daemon: string;
};

/**
 * What the text of a write is: a whole write carries the note's full body,
 * and an anchored edit carries the lines to change.
 */
export type WriteKind =
  | { kind: 'whole'; /** The whole note as this device last had it. */ body: string }
  | { kind: 'anchored'; edits: AnchoredEdit[] };

/** A note write as a caller queues it. The queue stamps the time and the order. */
export type OutboxWrite = NoteWrite & WriteKind;

export type OutboxEntry = OutboxWrite & {
  queuedAt: number;
  /** Ordering, so two writes to one note replay as they were made. */
  sequence: number;
};

/**
 * What a device's IndexedDB can hold: an entry queued before entries had a
 * kind has none. The store is the one place that reads it, so this type
 * stays here.
 */
type StoredEntry = NoteWrite & { queuedAt: number; sequence: number } & (
    | WriteKind
    | { kind?: undefined; body: string }
  );

/**
 * Read a stored entry as a current one.
 *
 * An entry with no kind was queued before an anchored edit could be, so it
 * is a whole write. This is the ONE place that rule lives: every reader
 * goes through it, so no reader repeats the default.
 */
function withKind(stored: StoredEntry): OutboxEntry {
  return stored.kind === undefined ? { ...stored, kind: 'whole' } : stored;
}

/** What a drain needs from the network. Injected, so a test supplies it. */
export interface OutboxSink {
  /**
   * Answers the note's hash on success. `ok: false` with `current` says the
   * note moved on under a whole write; with `refused` it says the daemon
   * could not place an anchored edit.
   */
  write(entry: OutboxEntry): Promise<SinkAnswer>;
  /** Write the losing copy beside the note. Answers its path. */
  writeConflictCopy(entry: OutboxEntry): Promise<string>;
}

export type SinkAnswer =
  | { ok: true; hash: string }
  | { ok: false; current: string; refused?: true };

export interface DrainResult {
  sent: number;
  conflicted: string[];
  /** Anchored entries the daemon refused. There is no body to copy. */
  refusedEdits: string[];
  /** Entries left alone because they belong to a different daemon. */
  foreign: number;
  failed: number;
  /** Sent, but replaced by a newer save meanwhile: the newer one stays queued. */
  superseded: number;
}

/**
 * The next ordering number, read from what is stored.
 *
 * A module counter reset on every page load while the outbox survived it, so
 * a write queued after a reload sorted BEFORE writes queued before it, and
 * the drain replayed them out of order.
 */
async function nextSequence(store: OfflineStore): Promise<number> {
  const held = await store.list<OutboxEntry>('outbox');
  return held.reduce((top, e) => Math.max(top, e.value.sequence ?? 0), 0) + 1;
}

/**
 * What became of a queue. `folded` says the write joined a queued one instead
 * of taking the key alone. A refusal names the edit whose line is not in the
 * queued text; nothing is queued for it.
 */
export type QueueOutcome = { ok: true; folded: boolean } | { ok: false; index: number };

/**
 * Queue a note write. ONE entry per note.
 *
 * The first queued base is what lets the drain detect a remote change: a
 * whole write with base `h0` is refused when the daemon holds `h1`. A second
 * entry per note would break that. An anchored replay changes the daemon's
 * hash, so a later whole write for the same note with base `h0` would then
 * always be refused, or, if rebased, would overwrite the remote change. So a
 * write that arrives for a queued note FOLDS into the queued entry:
 *
 * | queued   | arriving | result                                        |
 * |----------|----------|-----------------------------------------------|
 * | nothing  | any      | queue it                                      |
 * | whole    | whole    | replace the body, keep the first base         |
 * | whole    | anchored | apply the edits to the queued body, stay whole|
 * | anchored | anchored | append the edits, keep the base               |
 * | anchored | whole    | the whole write replaces it, keep the base    |
 *
 * The last row is safe because the buffer the whole write came from already
 * holds the anchored change.
 */
export async function queueWrite(store: OfflineStore, entry: OutboxWrite): Promise<QueueOutcome> {
  const existing = await store.get<StoredEntry>('outbox', entry.path);
  const held = existing && withKind(existing);
  const folded: Fold = held ? fold(held, entry) : { ok: true, write: entry, folded: false };
  if (!folded.ok) return folded;

  const sequence = await nextSequence(store);
  await store.put<OutboxEntry>('outbox', entry.path, {
    ...folded.write,
    // The base is what the user edited FROM, so the FIRST queue wins it.
    base: held?.base ?? entry.base,
    queuedAt: Date.now(),
    // A REPLACEMENT takes a new sequence. A drain that is mid-flight over the
    // old one compares this before it deletes, and leaves the newer writing
    // alone. Reusing the sequence made the two indistinguishable.
    sequence,
  });
  return { ok: true, folded: folded.folded };
}

type Fold = { ok: true; write: OutboxWrite; folded: boolean } | { ok: false; index: number };

/** The fold table above, for a note that already has a queued entry. */
function fold(held: OutboxEntry, arriving: OutboxWrite): Fold {
  if (arriving.kind === 'whole') return { ok: true, write: arriving, folded: false };
  if (held.kind === 'anchored') {
    return { ok: true, write: { ...arriving, edits: [...held.edits, ...arriving.edits] }, folded: true };
  }
  const applied = applyAnchoredEdits(held.body, arriving.edits);
  if (!applied.ok) return applied;
  return { ok: true, write: { ...arriving, kind: 'whole', body: applied.text }, folded: true };
}

/** Whether a path has writing the daemon has not received. */
export async function isQueued(store: OfflineStore, path: string): Promise<boolean> {
  return (await store.get<OutboxEntry>('outbox', path)) !== null;
}

/** The writing queued for a path, or null. What a read must prefer. */
export async function readQueued(store: OfflineStore, path: string): Promise<OutboxEntry | null> {
  const held = await store.get<StoredEntry>('outbox', path);
  return held && withKind(held);
}

export async function queuedCount(store: OfflineStore): Promise<number> {
  return (await store.list('outbox')).length;
}

/**
 * Send what is queued, oldest first.
 *
 * A note the daemon changed meanwhile becomes a conflict copy beside it, and
 * the entry clears only once that copy lands. Obsidian resolves a sync
 * conflict the same way; a CRDT would need every writer to speak it, and the
 * agent, the TUI and any editor never will.
 */
export async function drainOutbox(
  store: OfflineStore,
  sink: OutboxSink,
  daemon: string,
): Promise<DrainResult> {
  const entries = (await store.list<StoredEntry>('outbox')).map((e) => withKind(e.value));
  entries.sort((a, b) => a.sequence - b.sequence);

  const result: DrainResult = {
    sent: 0,
    conflicted: [],
    refusedEdits: [],
    foreign: 0,
    failed: 0,
    superseded: 0,
  };

  for (const entry of entries) {
    if (!sameDaemon(entry.daemon, daemon)) {
      result.foreign += 1;
      continue;
    }
    try {
      const answer = await sink.write(entry);
      if (answer.ok) {
        // Only a whole write knows the note's text. An anchored entry does
        // not, so the mirror keeps what it has until the next read refreshes it.
        if (entry.kind === 'whole') {
          await store.put<MirroredNote>('mirror', entry.path, {
            body: entry.body,
            hash: answer.hash,
            kiln: entry.kiln,
            mirroredAt: Date.now(),
          });
        }
        if (!(await clearIfUnchanged(store, entry))) result.superseded += 1;
        else result.sent += 1;
      } else if (entry.kind === 'anchored') {
        // The daemon could not place the edit. There is no body to keep, so
        // no conflict copy; and the daemon answered, so a replay would be
        // refused again. The entry clears and the refusal is reported.
        if (!(await clearIfUnchanged(store, entry))) result.superseded += 1;
        result.refusedEdits.push(entry.path);
      } else {
        const copy = await sink.writeConflictCopy(entry);
        // Only now: if the copy failed, the writing is still queued.
        if (!(await clearIfUnchanged(store, entry))) result.superseded += 1;
        result.conflicted.push(copy);
      }
    } catch {
      // Still offline, or the write failed. It stays queued.
      result.failed += 1;
    }
  }
  return result;
}

/**
 * Drop an entry ONLY if it is still the one that was sent.
 *
 * A drain awaits the network, and a save during that await replaces the
 * entry at the same key. Removing by path alone then deleted the NEWER
 * writing — which existed nowhere else, because the editor had already
 * cleared its buffer. `sequence` is stable for the life of an entry and is
 * reissued when a queue replaces one, so it identifies which is which.
 *
 * Answers false when the entry was replaced, and leaves the newer one queued.
 */
async function clearIfUnchanged(store: OfflineStore, sent: OutboxEntry): Promise<boolean> {
  const held = await store.get<OutboxEntry>('outbox', sent.path);
  if (!held || held.sequence !== sent.sequence || held.queuedAt !== sent.queuedAt) return false;
  await store.remove('outbox', sent.path);
  return true;
}

/** The name a losing copy takes, beside the note it lost to. */
export function conflictCopyPath(path: string, when: Date, device = 'phone'): string {
  const stamp = when.toISOString().slice(0, 10);
  const dot = path.lastIndexOf('.');
  const stem = dot > path.lastIndexOf('/') ? path.slice(0, dot) : path;
  const extension = dot > path.lastIndexOf('/') ? path.slice(dot) : '';
  return `${stem} (conflict, ${device}, ${stamp})${extension}`;
}
