/**
 * Why a turn ended, as the daemon spells it on `message_complete`.
 *
 * The daemon owns the set (`crucible_core::turn::StopReason`). A reason this
 * client does not know is a newer daemon talking to an older page, so the type
 * keeps a string fallback.
 *
 * **This page words no note about a reason.** It used to hold a
 * `stopReasonNotice` table that restated `StopReason::user_notice`, and the two
 * drifted — a capital letter and a trailing full stop. The daemon now sends the
 * words as `stop_notice` on the same event, and the reducer draws that string.
 * `crucible-web`'s `the_frontend_words_no_stop_reason_notice` refuses a second
 * wording here.
 */
export type StopReason =
  | 'end_turn'
  | 'cancelled'
  | 'empty'
  | 'max_tokens'
  | 'refusal'
  | (string & {});
