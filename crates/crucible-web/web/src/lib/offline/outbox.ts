import type { AnchoredEdit, MergeRegion } from '@/lib/api';
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
 * - An entry keeps the TEXT its base names, not only the hash. That text is
 *   the third one a three-way merge needs, and this device is the only party
 *   that holds it: without it a write the daemon refuses as stale can only be
 *   refused whole or overwrite the other writer.
 * - An entry clears only after its writing is safe somewhere the daemon holds.
 */

/** What every queued write names: which note, from which text, for which daemon. */
export type NoteWrite = {
  path: string;
  /** The disk hash it was edited FROM. Never updated once queued. */
  base: string;
  /**
   * The note as it was at `base`: the text this write was made FROM.
   *
   * The pair is one fact — `base` is the hash of this text — so a fold keeps
   * both or neither. Absent for an entry a device queued before writes kept
   * it, and for a caller that does not hold it. Such an entry is never merged
   * and never overwrites the other writer: it becomes a conflict over the
   * whole note.
   */
  baseText?: string;
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
   *
   * `opts.baseText` asks the daemon to MERGE a stale write against the disk
   * instead of refusing it. The drain sends it only on the retry: the first
   * attempt asks whether the note moved at all, and an anchored entry's first
   * attempt is still its anchored replay.
   */
  write(entry: OutboxEntry, opts?: { baseText: string }): Promise<SinkAnswer>;
  /** Write the losing copy beside the note. Answers its path. */
  writeConflictCopy(entry: OutboxEntry): Promise<string>;
}

/** The daemon took the write. `merged` says it merged our text with the disk. */
type SinkWrote =
  | { ok: true; hash: string; merged?: false }
  | { ok: true; hash: string; merged: true; content: string };

/**
 * The daemon would not take the write.
 *
 * `refused` names an anchored edit it could not place. Otherwise the note
 * moved on: `currentContent` is what it holds now, and `mergedContent` with
 * `regions` is how far a merge got, when one was asked for.
 */
type SinkRefused = {
  ok: false;
  current: string;
  refused?: true;
  currentContent?: string;
  mergedContent?: string;
  regions?: MergeRegion[];
};

export type SinkAnswer = SinkWrote | SinkRefused;

/** A write the daemon accepted: which note, from which base, to which hash. */
export type Landed = { path: string; base: string; hash: string };

/**
 * A whole write the daemon refused as stale, and could not merge cleanly.
 *
 * The base names the buffer the write came from, so the editor can mark that
 * buffer and no other. The three texts are what resolving it needs, without a
 * second round trip that would race the same way.
 */
export type Conflicted = {
  path: string;
  base: string;
  /** The conflict copy that now holds the text. */
  copy: string;
  /** The hash the note has now: what a resolution must be written against. */
  currentHash: string;
  /** The note as the daemon holds it now. Empty when it could not be read. */
  currentContent: string;
  /** Our text merged with theirs, as far as the merge got. */
  mergedContent: string;
  /** Every span the two writers changed differently. Never empty. */
  regions: MergeRegion[];
};

export interface DrainResult {
  sent: number;
  /**
   * One row per entry the daemon accepted, whole or anchored, in the order
   * they landed. A landed write moves the daemon's hash, so an open buffer
   * whose base is the entry's base must move to the answered hash. Without
   * this, the next save from that buffer is refused as stale for the user's
   * own queued write. `sent` counts only what cleared; a superseded entry
   * landed too, and its buffer moved the same way.
   */
  landed: Landed[];
  conflicted: Conflicted[];
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
 * | anchored | anchored | compose the edits, keep the base              |
 * | anchored | whole    | the whole write replaces it, keep the base    |
 *
 * The last row is safe because the buffer the whole write came from already
 * holds the anchored change.
 *
 * The anchored row COMPOSES, because the daemon resolves every edit of a
 * batch against the ORIGINAL text, and counts an edit whose `replace` is
 * already present as applied. An appended [tick, untick] replayed as a tick:
 * the untick's `expect` was absent, its `replace` was present, and the daemon
 * reported success with the user's last action lost. See `composeEdits`.
 * A composition that leaves no edit clears the entry.
 */
export async function queueWrite(store: OfflineStore, entry: OutboxWrite): Promise<QueueOutcome> {
  const existing = await store.get<StoredEntry>('outbox', entry.path);
  const held = existing && withKind(existing);
  const folded: Fold = held ? fold(held, entry) : { ok: true, write: entry, folded: false };
  if (!folded.ok) return folded;
  if (folded.write.kind === 'anchored' && folded.write.edits.length === 0) {
    // The edits returned the note to the queued base. Nothing is owed.
    await store.remove('outbox', entry.path);
    return { ok: true, folded: folded.folded };
  }

  const sequence = await nextSequence(store);
  await store.put<OutboxEntry>('outbox', entry.path, {
    ...folded.write,
    // The base is what the user edited FROM, so the FIRST queue wins it — and
    // its TEXT comes with it. A first hash paired with a later text asks the
    // daemon to merge from a text that is not the base it names, which it
    // refuses as a caller bug.
    ...(held ? { base: held.base, baseText: held.baseText } : { base: entry.base, baseText: entry.baseText }),
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
    return { ok: true, write: { ...arriving, edits: composeEdits(held.edits, arriving.edits) }, folded: true };
  }
  const applied = applyAnchoredEdits(held.body, arriving.edits);
  if (!applied.ok) return applied;
  return { ok: true, write: { ...arriving, kind: 'whole', body: applied.text }, folded: true };
}

/**
 * Compose arriving edits into held ones, so the daemon can anchor every edit
 * of the batch on the original text.
 *
 * An arriving edit whose `expect` is a held edit's `replace` continues that
 * edit: the held edit keeps its anchor and takes the arriving `replace`. When
 * that makes `expect` equal `replace`, the line is back where the daemon has
 * it, and the edit is dropped. An arriving edit that continues no held edit
 * is appended.
 *
 * Which held edit an arriving one continues: the one with the same
 * `occurrence`. An arriving edit with NO `occurrence` names a line that is
 * unique in the buffer, so when exactly one held edit wrote that line, it is
 * the one, whatever occurrence it named against the original.
 */
function composeEdits(held: AnchoredEdit[], arriving: AnchoredEdit[]): AnchoredEdit[] {
  const out = [...held];
  for (const edit of arriving) {
    const at = continuedBy(out, edit);
    if (at === -1) {
      out.push(edit);
      continue;
    }
    const first = out[at];
    if (first.expect === edit.replace) {
      out.splice(at, 1);
      continue;
    }
    out[at] = {
      expect: first.expect,
      replace: edit.replace,
      ...(first.occurrence === undefined ? {} : { occurrence: first.occurrence }),
    };
  }
  return out;
}

/** The index of the held edit that `arriving` continues, or -1. */
function continuedBy(held: AnchoredEdit[], arriving: AnchoredEdit): number {
  const wrote = held.map((e, i) => (e.replace === arriving.expect ? i : -1)).filter((i) => i !== -1);
  if (arriving.occurrence === undefined && wrote.length === 1) return wrote[0];
  return wrote.find((i) => held[i].occurrence === arriving.occurrence) ?? -1;
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

/** A queued write that carries the whole note. */
type WholeEntry = Extract<OutboxEntry, { kind: 'whole' }>;

/**
 * Our text as a whole note, to merge against the disk.
 *
 * A whole entry already is one. An anchored entry becomes one by applying its
 * edits to the text they were made from, which is the only text they are
 * guaranteed to anchor in. Null when this device did not keep that text, or
 * the edits no longer apply to it — neither is a merge anyone can make.
 */
function oursAsWhole(entry: OutboxEntry): string | null {
  if (entry.baseText === undefined) return null;
  if (entry.kind === 'whole') return entry.body;
  const folded = applyAnchoredEdits(entry.baseText, entry.edits);
  return folded.ok ? folded.text : null;
}

/** Lines the way the merge counts them: a trailing newline ends the last one. */
function lineCount(text: string): number {
  if (text === '') return 0;
  const parts = text.split('\n');
  return text.endsWith('\n') ? parts.length - 1 : parts.length;
}

/**
 * The one region a write with no base text becomes.
 *
 * Nothing here can merge without the text the write was made from, and
 * overwriting the other writer is not an answer either. So the whole note is
 * one span both sides changed, and a user chooses between the two texts.
 */
function wholeNoteRegion(ours: string, theirs: string): MergeRegion {
  return { start_line: 1, end_line: 1 + lineCount(ours), base: '', ours, theirs };
}

/**
 * Send what is queued, oldest first.
 *
 * A note the daemon changed meanwhile is MERGED, not copied aside: the daemon
 * holds the disk and the other writer's text, and the entry holds the text
 * this device edited from, so the retry carries that text and the daemon
 * merges the three. A merge that leaves a region writes nothing and comes
 * back as a conflict. A CRDT would need every writer to speak it, and the
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
    landed: [],
    conflicted: [],
    refusedEdits: [],
    foreign: 0,
    failed: 0,
    superseded: 0,
  };

  /** The daemon took it: name it, mirror what it now holds, clear the entry. */
  const land = async (entry: OutboxEntry, answer: SinkWrote) => {
    result.landed.push({ path: entry.path, base: entry.base, hash: answer.hash });
    // Only a whole write knows the note's text — and a merge answers with the
    // text the daemon wrote, which is neither writer's alone. An anchored
    // entry knows nothing, so the mirror keeps what it has until the next
    // read refreshes it.
    const body = answer.merged ? answer.content : entry.kind === 'whole' ? entry.body : null;
    if (body !== null) {
      await store.put<MirroredNote>('mirror', entry.path, {
        body,
        hash: answer.hash,
        kiln: entry.kiln,
        mirroredAt: Date.now(),
      });
    }
    if (!(await clearIfUnchanged(store, entry))) result.superseded += 1;
    else result.sent += 1;
  };

  /** The daemon answered, nothing could be merged: keep the text, report it. */
  const conflict = async (entry: WholeEntry, answer: SinkRefused) => {
    const copy = await sink.writeConflictCopy(entry);
    // Only now: if the copy failed, the writing is still queued.
    if (!(await clearIfUnchanged(store, entry))) result.superseded += 1;
    const currentContent = answer.currentContent ?? '';
    result.conflicted.push({
      path: entry.path,
      base: entry.base,
      copy,
      currentHash: answer.current,
      currentContent,
      mergedContent: answer.mergedContent ?? entry.body,
      regions: answer.regions?.length
        ? answer.regions
        : [wholeNoteRegion(entry.body, currentContent)],
    });
  };

  /** The refusal an anchored entry ends in: no body, so nothing to keep. */
  const refuseEdit = async (entry: OutboxEntry) => {
    if (!(await clearIfUnchanged(store, entry))) result.superseded += 1;
    result.refusedEdits.push(entry.path);
  };

  for (const entry of entries) {
    if (!sameDaemon(entry.daemon, daemon)) {
      result.foreign += 1;
      continue;
    }
    try {
      const answer = await sink.write(entry);
      if (answer.ok) {
        await land(entry, answer);
        continue;
      }
      if (entry.kind === 'anchored' && answer.refused === true) {
        // The daemon could not place the edit. There is no body to keep, and
        // the daemon answered, so a replay would be refused again.
        await refuseEdit(entry);
        continue;
      }

      // The note moved on. With the text this device edited FROM, the three
      // texts a merge needs are all in one place, so send ours whole and let
      // the daemon merge it against the disk.
      const ours = oursAsWhole(entry);
      if (ours !== null && entry.baseText !== undefined) {
        const whole: WholeEntry = {
          kind: 'whole',
          body: ours,
          path: entry.path,
          base: entry.base,
          baseText: entry.baseText,
          kiln: entry.kiln,
          daemon: entry.daemon,
          queuedAt: entry.queuedAt,
          sequence: entry.sequence,
        };
        const merged = await sink.write(whole, { baseText: entry.baseText });
        if (merged.ok) await land(whole, merged);
        else await conflict(whole, merged);
        continue;
      }

      if (entry.kind === 'anchored') {
        // No text to anchor in, so there is no whole note to keep and no
        // merge to make. Reported, like any edit the daemon would refuse.
        await refuseEdit(entry);
        continue;
      }
      await conflict(entry, answer);
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
