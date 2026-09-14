/**
 * The gate in front of a revert, the receipt after one, and the way back.
 *
 * Rejecting a hunk is the most destructive thing either review surface does:
 * the daemon rewrites the file on disk from `before_content` before it records
 * the state (`ReviewLedger::revert_hunk`). It was also the CHEAPEST — a ~22px
 * glyph, one click, no confirmation, no feedback — while deleting a session,
 * which loses nothing on disk, already raised a `confirm`. That gradient ran
 * backwards.
 *
 * The undo is the daemon's, not the browser's. After a revert the hunk is gone
 * from the composed diff along with its content-derived id, so a client has
 * nothing left to address, and holding `after_content` here to write it back
 * would be business logic on the render layer. Instead every reject, single
 * or bulk, pushes one batch on a journaled stack the daemon keeps per session,
 * and `review.undo_reject` pops the top batch (`review/undo.rs`). The receipt
 * each reject posts carries that undo as its one action. It is multi-level:
 * three rejects undo as three toasts, newest first.
 *
 * The attention is still spent BEFORE the write. A confirm stays in front of
 * every reject, because the undo can be refused: a hunk whose lines moved on
 * since the revert is `Stale`, and the batch stays on the stack for a retry
 * that may never succeed. Accept stays one click: this gate is on the
 * destructive half of the pair only, which is what keeps it from becoming the
 * fatigue the `re-applied` flag exists to make visible.
 */
import { notificationActions } from '@/stores/notificationStore';
import type { BulkOutcome } from './review-api';
import { reviewActions } from './review-store';
import { hunkRangeLabel, type ComposedHunk } from './review-types';

/** `src/a.rs L4–5` — what the user is about to lose, as they saw it. */
function label(hunk: ComposedHunk): string {
  return `${hunk.path} ${hunkRangeLabel(hunk)}`;
}

const plural = (count: number, noun: string) => `${count} ${noun}${count === 1 ? '' : 's'}`;

const UNDO_HINT = 'Undo from the notification that follows puts them back.';

/**
 * Ask before reverting. `false` means the caller must not touch the disk.
 *
 * Deliberately `window.confirm`, the same primitive `deleteSession` uses: one
 * spelling for "this rewrites something", identical on both review surfaces,
 * and it cannot be dismissed by a stray click the way an inline two-step can.
 */
export function confirmReject(hunk: ComposedHunk): boolean {
  return window.confirm(
    `Reject the change to ${label(hunk)}?\n\n` +
      `This reverts the lines on disk now and tells the agent. ${UNDO_HINT}`,
  );
}

/**
 * Ask once for a whole file or the whole review. `label` names the scope the
 * way the user reads it: a path, or `every file in this review`.
 */
export function confirmRejectAll(count: number, label: string): boolean {
  return window.confirm(
    `Reject ${plural(count, 'change')} to ${label}?\n\n` +
      `This reverts every one of them on disk now and tells the agent. ${UNDO_HINT}`,
  );
}

/**
 * Say what happened, and offer the way back.
 *
 * The revert was once silent on success, so a click that did rewrite a file
 * and a click the daemon refused looked the same until the row happened to
 * re-render. The `Undo` action is what keeps this toast on screen: an
 * actionable notification never auto-dismisses.
 */
export function announceReject(hunk: ComposedHunk, undo: () => void): void {
  notificationActions.addNotification('info', `Reverted ${label(hunk)} on disk.`, {
    label: 'Undo',
    run: undo,
  });
}

/** The bulk receipt: how many landed, and one `Undo` for the whole batch. */
export function announceRejectAll(count: number, undo: () => void): void {
  notificationActions.addNotification('info', `Reverted ${plural(count, 'change')} on disk.`, {
    label: 'Undo',
    run: undo,
  });
}

/**
 * Name what the daemon refused, once.
 *
 * A bulk call applies what it can and reports the rest per hunk. The names
 * come from the hunks the caller can still see; a hunk that already left the
 * composed diff falls back to its id, which is all the daemon has for it.
 */
export function announceRefused(failed: BulkOutcome['failed'], hunks: ComposedHunk[]): void {
  if (failed.length === 0) return;
  const named = failed.map((f) => {
    const hunk = hunks.find((h) => h.id === f.hunk_id);
    return `${hunk ? label(hunk) : f.hunk_id} (${f.reason})`;
  });
  notificationActions.addNotification(
    'warning',
    `The daemon refused ${plural(failed.length, 'hunk')}: ${named.join('; ')}`,
  );
}

/**
 * The undo every receipt offers: pop the daemon's top batch of rejects and
 * report what came back.
 *
 * `rejected` is the batch as the caller saw it before the revert; it only
 * serves to name a refused hunk, which has left the composed diff and cannot
 * be looked up any more. An empty answer means the stack was empty — another
 * client, or another toast, got there first.
 */
export async function undoLastReject(sessionId: string, rejected: ComposedHunk[]): Promise<void> {
  try {
    const outcome = await reviewActions.undoReject(sessionId);
    if (outcome.applied.length > 0) {
      notificationActions.addNotification(
        'info',
        `Restored ${plural(outcome.applied.length, 'change')} on disk.`,
      );
    } else if (outcome.failed.length === 0) {
      notificationActions.addNotification('info', 'Nothing left to undo.');
    }
    announceRefused(outcome.failed, rejected);
  } catch (e) {
    notificationActions.addNotification('error', (e as Error).message);
  }
}
