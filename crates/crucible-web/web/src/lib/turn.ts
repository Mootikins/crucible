/**
 * Transcript ids, derived one way.
 *
 * The backend keys a whole turn by one `message_id`; every bubble the
 * transcript draws beside it — the answer, a frozen pre-tool segment, a
 * reasoning-only opener — derives its id FROM that key with the functions
 * here, so live streaming, late-attaching viewers, and history
 * reconstruction all converge on identical ids without asking the daemon
 * for any of them.
 *
 * Pure helpers, deliberately outside `lib/api.ts`: they make no request,
 * and keeping them here is what lets the transcript layer import nothing
 * from the api module.
 */

/** Transcript id for the assistant response of a turn. The backend keys a
 * whole turn by one message_id (send response, user_message echo, and
 * message_complete all carry it); the user message takes the id itself and
 * the response takes this derived form, so live streaming, late-attaching
 * viewers, and history reconstruction all converge on identical ids.
 */
export function turnResponseId(messageId: string): string {
  return `${messageId}-response`;
}

/**
 * Transcript id for a pre-tool narration segment of a turn. A segmented turn
 * (text → tool → text) freezes each pre-tool text run into its own bubble;
 * the daemon's `segment_complete` event carries the turn's message_id and the
 * segment's 0-based index, and both live streaming and history reconstruction
 * derive the same id from them — so segmented turns converge on identical
 * bubbles across viewers and reload (mirrors `turnResponseId`).
 */
export function turnSegmentId(messageId: string, index: number): string {
  return `${messageId}-seg-${index}`;
}

/**
 * Transcript id for a turn's reasoning-only opening segment.
 *
 * A turn can reason and go straight to a tool, which closes that bubble with
 * no text in it. The send POST canonicalizes the optimistic placeholder to
 * `turnResponseId` the moment it returns, so the retired bubble can be left
 * holding the id the turn's ANSWER must carry. The answer claims it back and
 * the reasoning moves here — a live-only id, because history reconstruction
 * rebuilds no bubble for reasoning.
 */
export function turnThinkingId(messageId: string): string {
  return `${messageId}-thinking`;
}

/**
 * Strip the frozen-segment prefix off a turn's accumulated text so the final
 * bubble carries only the trailing (post-last-tool) narration. The daemon's
 * `message_complete` deliberately carries the WHOLE turn's text; segments
 * render as their own bubbles, so the final bubble must drop the
 * already-rendered prefix. Shared by the live reducer and history
 * reconstruction so both produce identical final-bubble content.
 *
 * Each segment is consumed in order. A segment whose trailing whitespace the
 * accumulated copy does not repeat still matches: the two texts are collected
 * by different accumulators, and a lost space at the seam must not make the
 * whole narration render a second time inside the final bubble. Nothing but
 * that trailing whitespace is forgiven — on any other mismatch the text is
 * returned verbatim, because a wrong slice loses words.
 */
export function stripFrozenPrefix(fullText: string, frozenSegments: string[]): string {
  let rest = fullText;
  for (const segment of frozenSegments) {
    if (rest.startsWith(segment)) {
      rest = rest.slice(segment.length);
      continue;
    }
    const withoutTrailingSpace = segment.replace(/\s+$/, '');
    if (withoutTrailingSpace !== '' && rest.startsWith(withoutTrailingSpace)) {
      rest = rest.slice(withoutTrailingSpace.length);
      continue;
    }
    return fullText;
  }
  return rest;
}

/** A client-minted id for an optimistic message, unique within a page. */
export function generateMessageId(): string {
  return `msg_${Date.now()}_${Math.random().toString(36).substring(2, 9)}`;
}
