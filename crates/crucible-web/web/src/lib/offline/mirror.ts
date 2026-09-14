import type { OfflineStore } from '@/lib/offline/store';
import type { OfflineMode } from '@/lib/offline/kept';

/**
 * The copy of a kept kiln on this device.
 *
 * A kiln is kept whole — every note, no cap, no eviction, no expiry — because
 * a user who wants a kiln on a plane wants the kiln, not the part a heuristic
 * guessed at (decision log, 2026-09-11). Attachments follow the kiln's mode.
 *
 * Nothing here decides WHEN to run. The caller does, so a test can.
 */

export interface MirroredNote {
  body: string;
  /** The disk hash the daemon answered with, which an edit anchors against. */
  hash: string;
  kiln: string;
  mirroredAt: number;
}

interface MirroredIndex {
  kiln: string;
  notes: { name: string; path: string; title: string | null; tags: string[] }[];
  indexedAt: number;
}

/** What a mirror run needs from the network. Injected, so a test supplies it. */
export interface MirrorSource {
  listNotes(kiln: string): Promise<{ name: string; path: string; title: string | null; tags: string[] }[]>;
  readNote(path: string): Promise<{ content: string; content_hash: string }>;
  readAttachment(path: string): Promise<Blob>;
}

export interface MirrorProgress {
  done: number;
  total: number;
}

/** Attachments a note points at: markdown images and wikilink embeds. */
export function attachmentsIn(body: string, notePath: string): string[] {
  const dir = notePath.slice(0, notePath.lastIndexOf('/'));
  const found = new Set<string>();
  const add = (target: string) => {
    const clean = target.trim().split(/[#?]/)[0];
    if (!clean || /^[a-z]+:\/\//i.test(clean) || clean.endsWith('.md')) return;
    found.add(clean.startsWith('/') ? clean : `${dir}/${clean}`);
  };
  for (const match of body.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)) add(match[1]);
  for (const match of body.matchAll(/!\[\[([^\]]+)\]\]/g)) add(match[1]);
  return [...found];
}

/**
 * Fetch a kiln into the store: every note, and — in `everything` mode — every
 * attachment its notes point at.
 *
 * One note that fails does not stop the run: a kiln with one unreadable file
 * should still be readable offline.
 */
export async function mirrorKiln(
  store: OfflineStore,
  source: MirrorSource,
  kiln: string,
  mode: OfflineMode,
  onProgress?: (progress: MirrorProgress) => void,
): Promise<{ notes: number; attachments: number; failed: string[] }> {
  const notes = await source.listNotes(kiln);
  await store.put<MirroredIndex>('index', kiln, { kiln, notes, indexedAt: Date.now() });

  const failed: string[] = [];
  let done = 0;
  let attachments = 0;
  const wanted = new Set<string>();

  for (const note of notes) {
    try {
      const { content, content_hash } = await source.readNote(note.path);
      await store.put<MirroredNote>('mirror', note.path, {
        body: content,
        hash: content_hash,
        kiln,
        mirroredAt: Date.now(),
      });
      if (mode === 'everything') for (const ref of attachmentsIn(content, note.path)) wanted.add(ref);
    } catch {
      failed.push(note.path);
    }
    done += 1;
    onProgress?.({ done, total: notes.length });
  }

  for (const path of wanted) {
    try {
      await store.put<Blob>('blobs', path, await source.readAttachment(path));
      attachments += 1;
    } catch {
      failed.push(path);
    }
  }

  return { notes: notes.length - failed.length, attachments, failed };
}

/** Drop everything kept for one kiln. */
export async function forgetKiln(store: OfflineStore, kiln: string): Promise<void> {
  await store.remove('index', kiln);
  for (const { key, value } of await store.list<MirroredNote>('mirror')) {
    if (value.kiln === kiln) await store.remove('mirror', key);
  }
  // A blob is keyed by its own path, which lies under the kiln's.
  for (const { key } of await store.list('blobs', `${kiln}/`)) {
    await store.remove('blobs', key);
  }
}

/** Read a note the mirror holds, or null. */
export function readMirrored(store: OfflineStore, path: string): Promise<MirroredNote | null> {
  return store.get<MirroredNote>('mirror', path);
}
