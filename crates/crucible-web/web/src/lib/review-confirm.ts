/**
 * The gate in front of a revert, and the receipt after one.
 *
 * Rejecting a hunk is the most destructive thing either review surface does:
 * the daemon rewrites the file on disk from `before_content` before it records
 * the state (`ReviewLedger::revert_hunk`). It was also the CHEAPEST — a ~22px
 * glyph, one click, no confirmation, no feedback — while deleting a session,
 * which loses nothing on disk, already raised a `confirm`. That gradient ran
 * backwards.
 *
 * There is no undo to offer instead. The daemon's whole review surface is five
 * methods — `review.list_hunks`, `review.set_state`, `review.comment`,
 * `review.resolve_comment`, `review.rebase` — and `set_state` only ever
 * reverts: `Rejected` routes to `revert_hunk`, while `Accepted`/`Unreviewed`
 * record a state and touch no bytes. After the revert the hunk is gone from
 * the composed diff along with its content-derived id, so a client has nothing
 * left to address. Re-applying would need a daemon method that does not exist
 * (see the note in the module docs of `review-api.ts` for the shape such a
 * method would take). Faking one client-side would mean holding
 * `after_content` in the browser and writing it back through a tool call —
 * inventing business logic on the render layer.
 *
 * So the attention is spent BEFORE the write, where it can still change the
 * outcome. Accept stays one click: this gate is on the destructive half of the
 * pair only, which is what keeps it from becoming the fatigue the `re-applied`
 * flag exists to make visible.
 */
import { notificationActions } from '@/stores/notificationStore';
import { hunkRangeLabel, type ComposedHunk } from './review-types';

/** `src/a.rs L4–5` — what the user is about to lose, as they saw it. */
function label(hunk: ComposedHunk): string {
  return `${hunk.path} ${hunkRangeLabel(hunk)}`;
}

/**
 * Ask before reverting. `false` means the caller must not touch the disk.
 *
 * Deliberately `window.confirm`, the same primitive `deleteSession` uses: one
 * spelling for "this is not undoable", identical on both review surfaces, and
 * it cannot be dismissed by a stray click the way an inline two-step can.
 */
export function confirmReject(hunk: ComposedHunk): boolean {
  return window.confirm(
    `Reject the change to ${label(hunk)}?\n\n` +
      'This reverts the lines on disk now and tells the agent. ' +
      'It cannot be undone from here.',
  );
}

/**
 * Say what happened. The revert was silent on success, so a click that did
 * rewrite a file and a click the daemon refused looked the same until the row
 * happened to re-render.
 */
export function announceReject(hunk: ComposedHunk): void {
  notificationActions.addNotification('info', `Reverted ${label(hunk)} on disk.`);
}
