/**
 * Why a turn ended, as the daemon spells it on `message_complete`.
 *
 * The daemon owns the set (`crucible_core::turn::StopReason`). A reason this
 * client does not know is a newer daemon talking to an older page, so the type
 * keeps a string fallback and `stopReasonNotice` answers null for it rather
 * than drawing a word nobody wrote.
 */
export type StopReason =
  | 'end_turn'
  | 'cancelled'
  | 'empty'
  | 'max_tokens'
  | 'refusal'
  | (string & {});

/**
 * The line the transcript draws beside a reply, or null when the reason needs
 * no note.
 *
 * It mirrors `StopReason::user_notice` on the Rust side. `end_turn` says
 * nothing because a completed answer explains itself, and `cancelled` and
 * `empty` already have their own paths here.
 */
export function stopReasonNotice(reason: StopReason | undefined): string | null {
  switch (reason) {
    case 'max_tokens':
      return 'The model reached its output limit, so the reply stops here.';
    case 'refusal':
      return 'The model declined to answer.';
    default:
      return null;
  }
}
