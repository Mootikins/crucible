import {
  getFileWithHash,
  listNotes,
  rawFileUrl,
  saveFileContent,
} from '@/lib/api';
import { daemonIdentity } from '@/lib/offline/identity';
import { keptMode } from '@/lib/offline/kept';
import { forgetKiln, mirrorKiln, readMirrored, type MirrorSource } from '@/lib/offline/mirror';
import { conflictCopyPath, drainOutbox, isQueued, queueWrite, type OutboxSink } from '@/lib/offline/outbox';
import { idbStore, type OfflineStore } from '@/lib/offline/store';

/**
 * The offline layer wired to the real app: the network adapters, and the
 * reads and writes that go through the mirror when there is no connection.
 *
 * The pure halves — the mirror, the outbox, the store — take their sources as
 * parameters and are tested without any of this.
 */

let store: OfflineStore | null = null;
/** The shipped store. Created on first use, so importing costs nothing. */
export function offlineStore(): OfflineStore {
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

export const networkSink: OutboxSink = {
  write: async (entry) => {
    // The whole note, anchored on the hash it was edited from: the daemon
    // compares that to the bytes on disk inside its own read-modify-write.
    const current = await getFileWithHash(entry.path).catch(() => null);
    if (current && current.content_hash !== entry.base) {
      return { ok: false, current: current.content_hash };
    }
    await saveFileContent(entry.path, entry.body);
    const after = await getFileWithHash(entry.path);
    return { ok: true, hash: after.content_hash };
  },
  writeConflictCopy: async (entry) => {
    const copy = conflictCopyPath(entry.path, new Date(entry.queuedAt));
    await saveFileContent(copy, entry.body);
    return copy;
  },
};

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
    const mirrored = await readMirrored(offlineStore(), path);
    if (mirrored) {
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

/** Save a note: to the daemon when it answers, to the outbox when it does not. */
export async function writeNote(opts: {
  path: string;
  body: string;
  base: string;
  kiln: string | null;
}): Promise<{ queued: boolean }> {
  let failure: unknown = null;
  if (isOnline()) {
    try {
      await saveFileContent(opts.path, opts.body);
      return { queued: false };
    } catch (error) {
      // The daemon answered and refused. That is not offline writing, and
      // queueing it would report a save that can never land.
      if (!neverAnswered(error)) throw error;
      failure = error;
    }
  }
  try {
    await queueWrite(offlineStore(), {
      path: opts.path,
      body: opts.body,
      base: opts.base,
      kiln: opts.kiln ?? '',
      daemon: await daemonIdentity(offlineStore()),
    });
    return { queued: true };
  } catch (queueError) {
    // No store on this browser — private mode, or no IndexedDB. The save
    // failed and nothing can hold the writing, so the caller must see the
    // SAVE's failure and keep the buffer dirty. Swallowing it here would tell
    // a user their note was safe when it is in neither place.
    throw failure ?? queueError;
  }
}

/** Send everything queued for the daemon now answering. */
export async function syncNow() {
  return drainOutbox(offlineStore(), networkSink, await daemonIdentity(offlineStore()));
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
