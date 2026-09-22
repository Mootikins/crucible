import { createStore, produce } from 'solid-js/store';
import type { DiffsetSource } from '@/lib/diffset';

/**
 * The stored review comments that the composer of each session attaches to
 * its next message.
 *
 * The diff pane adds an entry when it stores a comment. The composer of the
 * session shows each entry as a chip, and sends the references with the next
 * message. The daemon builds the context of each comment, so an entry holds
 * only the reference and its label.
 *
 * The entries are display state of this client: a draft, not a record. The
 * comment itself stays in the daemon store when the user removes the chip.
 */
export interface AttachedComment {
  /** The id of the stored comment. */
  id: string;
  /** The source of the diffset that owns the comment. */
  source: DiffsetSource;
  /** The chip text: "server.rs L17–19". */
  label: string;
  /** The chip tooltip: the path and the text of the comment. */
  title: string;
}

const [attached, setAttached] = createStore<Record<string, AttachedComment[]>>({});

export const composerComments = {
  /** The comments that the next message of `sessionId` attaches. */
  of(sessionId: string | null | undefined): AttachedComment[] {
    return sessionId ? (attached[sessionId] ?? []) : [];
  },

  /** Attach a comment to the draft of `sessionId`. A second attach does nothing. */
  attach(sessionId: string, comment: AttachedComment): void {
    setAttached(
      produce((all) => {
        const list = all[sessionId] ?? [];
        if (!list.some((c) => c.id === comment.id)) all[sessionId] = [...list, comment];
      }),
    );
  },

  /** Remove the chip of one comment. The stored comment does not change. */
  remove(sessionId: string, commentId: string): void {
    setAttached(sessionId, (list) => (list ?? []).filter((c) => c.id !== commentId));
  },

  /** Remove the chips that a sent message took. */
  take(sessionId: string, commentIds: readonly string[]): void {
    setAttached(sessionId, (list) => (list ?? []).filter((c) => !commentIds.includes(c.id)));
  },

  /** Forget every draft. Tests only. */
  resetForTests(): void {
    setAttached(produce((all) => Object.keys(all).forEach((key) => delete all[key])));
  },
};
