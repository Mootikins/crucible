import {
  getFileWithHash,
  saveFileIfUnchanged,
  listNotes,
  patchKilnFile,
  rawFileUrl,
  saveFileContent,
  type AnchoredEdit,
  type PatchRefused,
} from '@/lib/api';
import { applyAnchoredEdits } from '@/lib/offline/fold';
import { daemonIdentity } from '@/lib/offline/identity';
import { keptMode } from '@/lib/offline/kept';
import { notificationActions } from '@/stores/notificationStore';
import { forgetKiln, mirrorKiln, readMirrored, type MirrorSource } from '@/lib/offline/mirror';
import {
  conflictCopyPath,
  drainOutbox,
  isQueued,
  queueWrite,
  queuedCount,
  readQueued,
  type Conflicted,
  type Landed,
  type NoteWrite,
  type OutboxSink,
  type QueueOutcome,
  type WriteKind,
} from '@/lib/offline/outbox';
import { idbStore, type OfflineStore } from '@/lib/offline/store';

/**
 * The offline layer wired to the real app: the network adapters, and the
 * reads and writes that go through the mirror when there is no connection.
 *
 * The pure halves — the mirror, the outbox, the store — take their sources as
 * parameters and are tested without any of this.
 */

let store: OfflineStore | null = null;
/**
 * The shipped store. Created on first use, so importing costs nothing.
 *
 * Private to this layer. A component that held the store assembled the
 * outbox on its own, and the facade below is the one door to it.
 */
function offlineStore(): OfflineStore {
  return (store ??= idbStore());
}

/** Test seam: swap the store, and put it back. */
export function setOfflineStore(next: OfflineStore | null): void {
  store = next;
}

/** A kiln-relative note path, made absolute. Already-absolute passes through. */
export function joinKiln(kiln: string, notePath: string): string {
  if (notePath.startsWith('/')) return notePath;
  return `${kiln.replace(/\/$/, '')}/${notePath.replace(/^\.?\//, '')}`;
}

export const networkSource: MirrorSource = {
  // `NoteEntry.path` is RELATIVE to the kiln root. Everything downstream —
  // the read that fills the mirror, the key it is stored under, the directory
  // an attachment resolves against — works in absolute paths, because that is
  // what the editor holds. Joining here is what makes a kept kiln readable:
  // unjoined, every read 404s and every note lands under a key no lookup asks
  // for, so the kiln caches nothing and reports every file as failed.
  listNotes: async (kiln) =>
    (await listNotes(kiln)).map((note) => ({
      name: note.name,
      path: joinKiln(kiln, note.path),
      title: note.title,
      tags: note.tags,
    })),
  readNote: async (path) => getFileWithHash(path),
  readAttachment: async (path) => {
    const response = await fetch(rawFileUrl(path));
    if (!response.ok) throw new Error(`attachment ${path}: ${response.status}`);
    return await response.blob();
  },
};

/** Whether an error is the daemon saying the path is not there. */
function isMissing(error: unknown): boolean {
  return !!error && typeof error === 'object' && (error as { status?: number }).status === 404;
}

export const networkSink: OutboxSink = {
  write: async (entry, opts) => {
    if (entry.kind === 'anchored') {
      // No base on replay. The daemon applies the anchors to the current
      // text, and an edit already there counts as applied (WS-202,
      // `note_edit.rs`). A base would refuse every replay after any other
      // change to the note, and an anchored edit that finds its line is
      // safe by construction: it changes that line and nothing else.
      const answer = await patchKilnFile(entry.path, entry.edits, undefined);
      if (!answer.ok) return { ok: false, refused: true, current: answer.current_hash };
      return { ok: true, hash: answer.content_hash };
    }
    // The DAEMON compares the hash, inside its own write. This used to read
    // the note, compare in the browser and then PUT — three round trips with
    // a window in the middle, on the machine with the stale view of the disk.
    // Section 11 of the mobile note forbids exactly that; it was here only
    // because no route could refuse a stale base yet.
    //
    // An empty `current` means the note is gone. Its writing still belongs to
    // the user, so it becomes a conflict rather than resurrecting a note that
    // was deleted.
    //
    // A `baseText` asks the route to MERGE a stale write against the disk
    // rather than refuse it. The drain sends it on the retry, which is the
    // only call that has anything to merge.
    const answer = await saveFileIfUnchanged(entry.path, entry.body, entry.base, opts?.baseText);
    if (answer.ok) {
      // The route answers with the hash of what it wrote. The old code issued
      // a THIRD read to learn it, which another writer could land inside —
      // storing a hash that described someone else's bytes beside this body.
      return answer.merged
        ? { ok: true, hash: answer.content_hash, merged: true, content: answer.content }
        : { ok: true, hash: answer.content_hash };
    }
    if (answer.regions?.length) {
      return {
        ok: false,
        current: answer.current_hash,
        currentContent: answer.current_content ?? '',
        mergedContent: answer.merged_content ?? entry.body,
        regions: answer.regions,
      };
    }
    // Refused without a merge, because this write carried no base text. The
    // refusal names a hash and nothing else, and whoever resolves it needs the
    // text the other writer put there — so read the note once, on the one path
    // that cannot merge. A read that fails leaves the text unknown.
    return { ok: false, current: answer.current_hash, currentContent: await currentText(entry.path) };
  },
  writeConflictCopy: (entry) => {
    // The drain reports a refused anchored entry instead of calling this: an
    // edit has no body to keep. A throw here keeps the entry queued.
    if (entry.kind === 'anchored') throw new Error(`${entry.path}: an anchored edit has no body to copy`);
    return writeConflictCopy(entry.path, entry.body, new Date(entry.queuedAt));
  },
};

/** The note as the daemon holds it now, or undefined when it cannot be read. */
async function currentText(path: string): Promise<string | undefined> {
  try {
    return (await getFileWithHash(path)).content;
  } catch {
    // The note is gone, or the network went down between the two calls. The
    // conflict is still real and is still reported; only its other text is
    // unknown.
    return undefined;
  }
}

/**
 * Keep a stale write's text beside the note. Answers the path it took.
 *
 * The drain calls this for a queued write the daemon refused. The editor
 * calls it when the user chooses to keep a refused save. One function, so
 * the editor does not reach for the API to write a copy on its own.
 *
 * A free name, not merely a dated one. The stamp is a DATE, so a second
 * conflict on the same note on the same day produced the same path and the
 * PUT destroyed the first copy — the only place that writing existed.
 */
export async function writeConflictCopy(path: string, body: string, when: Date): Promise<string> {
  const copy = await freeConflictPath(path, when);
  await saveFileContent(copy, body);
  return copy;
}

/** The first conflict-copy path nothing occupies. */
async function freeConflictPath(path: string, when: Date): Promise<string> {
  const base = conflictCopyPath(path, when);
  for (let n = 1; n <= 50; n += 1) {
    const candidate = n === 1 ? base : numbered(base, n);
    try {
      await getFileWithHash(candidate);
    } catch (error) {
      // Nothing there: the name is free. Any other failure means we cannot
      // tell, and guessing risks overwriting — fall through to a stamp that
      // cannot collide.
      if (isMissing(error)) return candidate;
      break;
    }
  }
  return numbered(conflictCopyPath(path, when), Date.now());
}

function numbered(path: string, n: number): string {
  const dot = path.lastIndexOf('.');
  return dot > path.lastIndexOf('/')
    ? `${path.slice(0, dot)} ${n}${path.slice(dot)}`
    : `${path} ${n}`;
}

/**
 * Whether a failure means "the daemon never answered".
 *
 * The API client stamps `status` on an error it built from a RESPONSE, so a
 * 403 on a read-only project, a 413, a 500 — anything the daemon said — has
 * one. A fetch that never reached it does not.
 *
 * Only the second kind may be queued. Queueing the first would tell a user
 * their note was saved, retry forever against a daemon that will refuse it
 * again, and hide a real error behind an offline badge.
 */
function neverAnswered(error: unknown): boolean {
  return !(error && typeof error === 'object' && 'status' in error);
}

/** Whether the browser believes it can reach anything. */
export function isOnline(): boolean {
  return typeof navigator === 'undefined' || navigator.onLine !== false;
}

/** Which kiln a path belongs to, among the kilns this device keeps. */
export function kilnOf(path: string, kilns: readonly string[]): string | null {
  let best: string | null = null;
  for (const kiln of kilns) {
    if (path === kiln || path.startsWith(`${kiln}/`)) {
      if (!best || kiln.length > best.length) best = kiln;
    }
  }
  return best;
}

/**
 * Read a note: the network when it answers, the mirror when it does not.
 *
 * The mirror is also written on a successful read, so opening a note in a kept
 * kiln keeps it current without a second fetch.
 */
export async function readNote(
  path: string,
  kiln: string | null,
): Promise<{ content: string; content_hash: string; fromMirror: boolean }> {
  let failure: unknown = null;
  if (isOnline()) {
    try {
      const fresh = await getFileWithHash(path);
      // A path the outbox holds keeps its queued text and its base: the
      // mirror must not move under writing the daemon has not received.
      if (kiln && keptMode(kiln)) {
        try {
          const db = offlineStore();
          if (!(await isQueued(db, path))) {
            await db.put('mirror', path, {
              body: fresh.content,
              hash: fresh.content_hash,
              kiln,
              mirroredAt: Date.now(),
            });
          }
        } catch {
          /* no store on this browser: the read still succeeded */
        }
      }
      return { ...fresh, fromMirror: false };
    } catch (error) {
      failure = error;
    }
  }
  try {
    const db = offlineStore();
    // Writing this device has not sent yet OUTRANKS the mirror. The mirror is
    // the daemon's copy; the outbox is the user's own text, and it exists
    // nowhere else. Reading past it showed the stale body after an offline
    // save — and because the buffer had already gone clean, editing from
    // there replaced the queued writing with an edit of the older text.
    // An anchored entry holds no body. The best text there is, is the mirror
    // with the queued edits folded in: what the user will see once the drain
    // lands them. When they no longer apply, the mirror stands as it is.
    // The preview's hash is the queued base, and its body is the frozen
    // mirror, so the two can differ by one hash when the mirror predates the
    // base.
    const queued = await readQueued(db, path);
    if (queued && queued.kind === 'whole') {
      return { content: queued.body, content_hash: queued.base, fromMirror: true };
    }
    const mirrored = await readMirrored(db, path);
    if (mirrored) {
      if (queued) {
        const folded = applyAnchoredEdits(mirrored.body, queued.edits);
        const content = folded.ok ? folded.text : mirrored.body;
        return { content, content_hash: queued.base, fromMirror: true };
      }
      return { content: mirrored.body, content_hash: mirrored.hash, fromMirror: true };
    }
  } catch {
    /* no store on this browser: fall through to the real failure */
  }
  // Nothing kept, or nowhere to keep it. The caller must see why the READ
  // failed — its retry is the read — rather than a storage error it cannot act
  // on or a mirror miss that hides a 404.
  throw failure ?? new Error(`${path} is not available offline`);
}

/**
 * What became of a whole write.
 *
 * `stale` is an answer, not a failure: the daemon compared the base and the
 * note moved on. `queued` means the daemon never answered, so the outbox holds
 * the writing until it does.
 */
export type WriteOutcome =
  | { queued: false; stale: false; hash: string }
  | { queued: false; stale: true; current: string }
  | { queued: true };

/**
 * Write a note: to the daemon when it answers, to the outbox when it does not.
 *
 * The write carries the base it was edited from, and the daemon compares it.
 * A stale base is REFUSED, not queued: the daemon answered, so this is not
 * offline writing, and the user is present to decide what happens to their
 * text.
 *
 * `baseText` is the note as it was at `base`. It is queued with the write, so
 * a drain that finds the note moved on can hand the daemon all three texts
 * and be merged instead of refused. A caller that does not hold it queues a
 * write that can only conflict.
 */
export async function writeNote(opts: {
  path: string;
  body: string;
  base: string;
  baseText?: string;
  kiln: string | null;
}): Promise<WriteOutcome> {
  return sendOrQueue(
    async () => {
      const answer = await saveFileIfUnchanged(opts.path, opts.body, opts.base);
      if (!answer.ok) return { queued: false, stale: true, current: answer.current_hash };
      return { queued: false, stale: false, hash: answer.content_hash };
    },
    {
      kind: 'whole',
      path: opts.path,
      body: opts.body,
      base: opts.base,
      baseText: opts.baseText,
      kiln: opts.kiln ?? '',
    },
    () => {
      throw new Error('a whole write replaces a queued write; it is never refused');
    },
  );
}

/**
 * What became of an anchored edit.
 *
 * A refusal is the daemon's answer, with the edit it could not place and the
 * hash the note has now. `queued` means the daemon never answered.
 */
export type EditOutcome =
  | { queued: false; ok: true; hash: string }
  | ({ queued: false } & PatchRefused)
  | { queued: true };

/**
 * Change a note's lines: through the daemon when it answers, through the
 * outbox when it does not.
 *
 * The edit carries the base it was made from, so a moved note is refused
 * whole. A refusal is returned, not queued, for the same reason a stale
 * whole write is: the daemon answered, and the user is present to decide.
 *
 * Offline, the edit folds into the write queued for the note. When the line
 * is not in the queued text, the fold is refused and the answer takes the
 * daemon refusal's shape with an empty `current_hash`, so the caller's one
 * revert path runs. Nothing is queued for it.
 */
export async function editNote(opts: {
  path: string;
  edits: AnchoredEdit[];
  base: string;
  /**
   * The note as it was at `base`. Queued with the edit: a replay the daemon
   * refuses is then merged as the whole note the edits make of this text,
   * rather than reported as an edit that can never be placed.
   */
  baseText?: string;
  kiln: string | null;
}): Promise<EditOutcome> {
  return sendOrQueue(
    async () => {
      const answer = await patchKilnFile(opts.path, opts.edits, opts.base);
      if (!answer.ok) return { queued: false, ...answer };
      return { queued: false, ok: true, hash: answer.content_hash };
    },
    {
      kind: 'anchored',
      path: opts.path,
      edits: opts.edits,
      base: opts.base,
      baseText: opts.baseText,
      kiln: opts.kiln ?? '',
    },
    (index) => ({
      queued: false,
      ok: false,
      failed: [{ reason: 'the line is not in the queued text', index }],
      current_hash: '',
      stale_base: false,
    }),
  );
}

/**
 * The one rule for a note write that may not reach the daemon.
 *
 * Online, `send` runs and its answer is the outcome. A throw the daemon never
 * answered queues `write` instead. A throw the daemon answered with is
 * raised: that is not offline writing, and queueing it would report a save
 * that can never land. Both `writeNote` and `editNote` go through here, so
 * the two kinds cannot drift apart on when they queue.
 *
 * The outbox holds one entry per note and folds a second write into it. A
 * fold it refuses is an answer, and `refusedFold` gives it the caller's shape.
 */
async function sendOrQueue<T>(
  send: () => Promise<T>,
  write: Omit<NoteWrite, 'daemon'> & WriteKind,
  refusedFold: (index: number) => T,
): Promise<T | { queued: true }> {
  let failure: unknown = null;
  if (isOnline()) {
    try {
      return await send();
    } catch (error) {
      if (!neverAnswered(error)) throw error;
      failure = error;
    }
  }
  let queued: QueueOutcome;
  try {
    queued = await queueWrite(offlineStore(), { ...write, daemon: await daemonIdentity(offlineStore()) });
  } catch (queueError) {
    // No store on this browser — private mode, or no IndexedDB. The write
    // failed and nothing can hold it, so the caller must see the SEND's
    // failure and keep the buffer dirty. Swallowing it here would tell a user
    // their note was safe when it is in neither place.
    throw failure ?? queueError;
  }
  return queued.ok ? { queued: true } : refusedFold(queued.index);
}

/** How many writes this device still owes the daemon. */
export async function pendingCount(): Promise<number> {
  return queuedCount(offlineStore());
}

export type { Conflicted, Landed };

/**
 * Who wants to know that a queued write landed.
 *
 * The editor holds the open buffers, and this layer holds the drain. A
 * landed write moves the daemon's hash, so a buffer that was edited from the
 * entry's base must move to the answered hash. Otherwise its next save is
 * refused as stale for the user's own queued write. The set lives here, in
 * the layer that drains, so the editor never learns the outbox's shape.
 */
const landedListeners = new Set<(row: Landed) => void>();

/** Hear each write the drain lands. Answers the function that stops it. */
export function onNoteLanded(listener: (row: Landed) => void): () => void {
  landedListeners.add(listener);
  return () => {
    landedListeners.delete(listener);
  };
}

/**
 * Who wants to know that a queued whole write was refused, and where its
 * text went.
 *
 * The write went clean at queue time. The drain wrote a conflict copy and
 * cleared the entry, so an open buffer made from the entry's base shows text
 * the note does not hold, and looks clean. A listener that answers `true`
 * told the user itself, with the buffer in view; the drain then says nothing
 * more about that row.
 */
const conflictedListeners = new Set<(row: Conflicted) => boolean | void>();

/** Hear each whole write the drain turned into a conflict copy. */
export function onNoteConflicted(listener: (row: Conflicted) => boolean | void): () => void {
  conflictedListeners.add(listener);
  return () => {
    conflictedListeners.delete(listener);
  };
}

/** Send everything queued for the daemon now answering. */
export async function syncNow() {
  const result = await drainOutbox(offlineStore(), networkSink, await daemonIdentity(offlineStore()));
  for (const row of result.landed) {
    for (const listener of landedListeners) {
      // One listener's throw must not stop the others, or reject the sync.
      try {
        listener(row);
      } catch (error) {
        console.error('a landed-write listener threw', error);
      }
    }
  }
  // A conflict copy is the one outcome a user MUST be told about: their text
  // did not land on the note they wrote it in, and nothing else on screen
  // says so — the queue count drops either way. Every caller discarded this.
  for (const row of result.conflicted) {
    let told = false;
    for (const listener of conflictedListeners) {
      try {
        if (listener(row) === true) told = true;
      } catch (error) {
        console.error('a conflicted-write listener threw', error);
      }
    }
    if (told) continue;
    notificationActions.addNotification(
      'warning',
      `The note changed elsewhere. Your version was saved as ${row.copy.split('/').pop()}`,
    );
  }
  // A refused anchored edit leaves no copy behind, so this is the only trace.
  for (const path of result.refusedEdits) {
    notificationActions.addNotification(
      'warning',
      `A queued edit to ${path.split('/').pop()} was not applied. The line it changed has moved.`,
    );
  }
  if (result.foreign > 0) {
    notificationActions.addNotification(
      'warning',
      `${result.foreign} unsent edit(s) belong to a different daemon and were not sent.`,
    );
  }
  return result;
}

/** Fetch a kiln into the store, in the mode it is kept in. */
export async function cacheKiln(kiln: string, onProgress?: (done: number, total: number) => void) {
  const mode = keptMode(kiln);
  if (!mode) throw new Error(`${kiln} is not kept offline`);
  // Choosing to keep a kiln IS the declaration that this device will write
  // offline against this daemon. Learn its identity now, while it answers.
  await warmIdentity();
  return mirrorKiln(offlineStore(), networkSource, kiln, mode, (p) =>
    onProgress?.(p.done, p.total),
  );
}

/** Drop what a kiln kept. */
export async function dropKiln(kiln: string): Promise<void> {
  await forgetKiln(offlineStore(), kiln);
}

/** How much a kiln costs on this device, notes and attachments apart. */
export async function kilnSize(kiln: string): Promise<{ notes: number; attachments: number }> {
  const db = offlineStore();
  let notes = 0;
  for (const { value } of await db.list<{ kiln: string; body: string }>('mirror')) {
    if (value.kiln === kiln) notes += value.body.length * 2;
  }
  return { notes, attachments: await db.size('blobs', `${kiln}/`) };
}

/**
 * An attachment's URL: a blob from the store, else the network.
 *
 * A cached binary is handed back as a `blob:` URL, which carries none of the
 * headers `/api/file/raw` sets — no `nosniff`, no `Content-Disposition`, no
 * sandbox CSP — and inherits THIS origin. An `<img>` cannot run script even
 * for an SVG, so that is the only element allowed to take one. Anything
 * needing a document context keeps using `rawFileUrl`.
 */
export async function attachmentUrl(path: string, kiln: string | null): Promise<string> {
  const db = offlineStore();
  const cached = await db.get<Blob>('blobs', path);
  if (cached) return URL.createObjectURL(cached);
  if (!isOnline()) return rawFileUrl(path);
  const url = rawFileUrl(path);
  // In a kept kiln, an attachment opened once is kept — which is what
  // `notes` mode means by "fetched when first opened".
  if (kiln && keptMode(kiln)) {
    try {
      const blob = await networkSource.readAttachment(path);
      await db.put('blobs', path, blob);
      return URL.createObjectURL(blob);
    } catch {
      /* keep the network url */
    }
  }
  return url;
}

/**
 * Learn which daemon this is, while it can still be asked.
 *
 * Call this whenever the network is believed up. A write queued OFFLINE is
 * stamped with the identity, and that is the one moment it cannot be fetched
 * — so if nothing warmed it first, the stamp is empty and the write can never
 * drain. Reading a note does not warm it: a read needs no identity, and
 * spending a config fetch on every read to cover a write that may never come
 * is the wrong trade.
 *
 * Cheap to repeat and safe to ignore: a failure means the daemon is not
 * answering, which is the case the remembered value already covers.
 */
export async function warmIdentity(): Promise<void> {
  await daemonIdentity(offlineStore()).catch(() => undefined);
}
