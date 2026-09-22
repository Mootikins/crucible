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
 * The chip and the stored comment are one thing. The `×` of a chip deletes
 * the comment, and a stored comment with no chip can attach itself again. A
 * comment that a message already took is the exception: the agent has it, so
 * `take` remembers the id and a later `×` only drops the chip.
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
/** The comments that a sent message of each session already carried. */
const [sent, setSent] = createStore<Record<string, string[]>>({});

export const composerComments = {
  /** The comments that the next message of `sessionId` attaches. */
  of(sessionId: string | null | undefined): AttachedComment[] {
    return sessionId ? (attached[sessionId] ?? []) : [];
  },

  /** True while the comment has a chip in the composer of `sessionId`. */
  has(sessionId: string | null | undefined, commentId: string): boolean {
    return this.of(sessionId).some((c) => c.id === commentId);
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
  detach(sessionId: string, commentId: string): void {
    setAttached(sessionId, (list) => (list ?? []).filter((c) => c.id !== commentId));
  },

  /**
   * Remove the chips that a sent message took, and remember their ids.
   *
   * The agent now has each of these comments, so a later `×` must not delete
   * one. See `wasSent`.
   */
  take(sessionId: string, commentIds: readonly string[]): void {
    setAttached(sessionId, (list) => (list ?? []).filter((c) => !commentIds.includes(c.id)));
    setSent(
      produce((all) => {
        const known = all[sessionId] ?? [];
        all[sessionId] = [...known, ...commentIds.filter((id) => !known.includes(id))];
      }),
    );
  },

  /** True when a message of `sessionId` already carried this comment. */
  wasSent(sessionId: string | null | undefined, commentId: string): boolean {
    return sessionId ? (sent[sessionId] ?? []).includes(commentId) : false;
  },

  /** Forget every draft. Tests only. */
  resetForTests(): void {
    setAttached(produce((all) => Object.keys(all).forEach((key) => delete all[key])));
    setSent(produce((all) => Object.keys(all).forEach((key) => delete all[key])));
  },
};
