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

export interface OutboxEntry {
  path: string;
  /** The whole note as this device last had it. */
  body: string;
  /** The disk hash it was edited FROM. Never updated once queued. */
  base: string;
  kiln: string;
  daemon: string;
  queuedAt: number;
  /** Ordering, so two writes to one note replay as they were made. */
  sequence: number;
}

/** What a drain needs from the network. Injected, so a test supplies it. */
export interface OutboxSink {
  /** Answers the note's hash on success, or null when the note moved on. */
  write(entry: OutboxEntry): Promise<{ ok: true; hash: string } | { ok: false; current: string }>;
  /** Write the losing copy beside the note. Answers its path. */
  writeConflictCopy(entry: OutboxEntry): Promise<string>;
}

export interface DrainResult {
  sent: number;
  conflicted: string[];
  /** Entries left alone because they belong to a different daemon. */
  foreign: number;
  failed: number;
}

let nextSequence = 0;

/** Queue a note's new text. Replaces an earlier queued write to the same note. */
export async function queueWrite(
  store: OfflineStore,
  entry: Omit<OutboxEntry, 'queuedAt' | 'sequence'>,
): Promise<void> {
  const existing = await store.get<OutboxEntry>('outbox', entry.path);
  await store.put<OutboxEntry>('outbox', entry.path, {
    ...entry,
    // The base is what the user edited FROM, so the FIRST queue wins it.
    base: existing?.base ?? entry.base,
    queuedAt: Date.now(),
    sequence: existing?.sequence ?? (nextSequence += 1),
  });
}

/** Whether a path has writing the daemon has not received. */
export async function isQueued(store: OfflineStore, path: string): Promise<boolean> {
  return (await store.get<OutboxEntry>('outbox', path)) !== null;
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
  const entries = (await store.list<OutboxEntry>('outbox')).map((e) => e.value);
  entries.sort((a, b) => a.sequence - b.sequence);

  const result: DrainResult = { sent: 0, conflicted: [], foreign: 0, failed: 0 };

  for (const entry of entries) {
    if (!sameDaemon(entry.daemon, daemon)) {
      result.foreign += 1;
      continue;
    }
    try {
      const answer = await sink.write(entry);
      if (answer.ok) {
        await store.put<MirroredNote>('mirror', entry.path, {
          body: entry.body,
          hash: answer.hash,
          kiln: entry.kiln,
          mirroredAt: Date.now(),
        });
        await store.remove('outbox', entry.path);
        result.sent += 1;
      } else {
        const copy = await sink.writeConflictCopy(entry);
        // Only now: if the copy failed, the writing is still queued.
        await store.remove('outbox', entry.path);
        result.conflicted.push(copy);
      }
    } catch {
      // Still offline, or the write failed. It stays queued.
      result.failed += 1;
    }
  }
  return result;
}

/** The name a losing copy takes, beside the note it lost to. */
export function conflictCopyPath(path: string, when: Date, device = 'phone'): string {
  const stamp = when.toISOString().slice(0, 10);
  const dot = path.lastIndexOf('.');
  const stem = dot > path.lastIndexOf('/') ? path.slice(0, dot) : path;
  const extension = dot > path.lastIndexOf('/') ? path.slice(dot) : '';
  return `${stem} (conflict, ${device}, ${stamp})${extension}`;
}
