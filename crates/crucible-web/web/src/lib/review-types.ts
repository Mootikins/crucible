/**
 * Wire types for the attributed-diff review surface.
 *
 * Every shape here is an alias into the generated contract. They live in this
 * file rather than in `lib/types.ts` so the review surface can ship as one
 * self-contained slice, and the two helper functions below travel with them.
 *
 * One deliberate absence: there is no `external` field. Rust exposes it as
 * `ComposedHunk::is_external()`, a method, so it never crosses the wire —
 * `tool_call_ids.length === 0` IS the definition. `isExternal()` below is the
 * single place that knowledge lives on this side.
 */
import type { components } from './api-schema';

type Schemas = components['schemas'];

/** Per-composed-hunk review state. */
export type ReviewState = Schemas['ReviewStateRow'];

/**
 * Which hunks a review lists: the session's, or the current turn's. The daemon
 * decides membership; the browser only names the scope it wants.
 */
export type ReviewScope = Schemas['ReviewScopeRow'];

/**
 * A hunk of the composed diff (`session_base` → current worktree).
 *
 * The action unit of review. `before_content`/`after_content` are whole
 * newline-terminated blocks (empty for a pure insertion / deletion). They are
 * the original and the document of the merge view `HunkMergeView` mounts.
 * `base_range` and `current_range` are 1-based and half-open: `start` is the
 * first line and `end` is one PAST the last.
 *
 * `reapplied` says the agent re-applied a change the user rejected. It is
 * derived by the daemon, never client-settable, and such a hunk still reports
 * `state: 'unreviewed'` — it adds no decision, only the history that makes the
 * grind visible instead of showing a fresh-looking hunk each round.
 */
export type ComposedHunk = Schemas['ReviewHunkRow'];

export type ReviewComment = Schemas['ReviewCommentRow'];

/**
 * Effective review policy for the session's current mode.
 *
 * The daemon degrades this per agent capability before sending it
 * (`ModeDescriptor::degraded_for`), so what arrives here is what will actually
 * happen — an ACP session in `normal` reports `post_turn`, not `pre_write`.
 * Rendering the configured policy instead would be a lie about a safety
 * property, so nothing on this side re-derives it from the mode id.
 */
export type ReviewPolicy = Schemas['ReviewPolicyRow'];

/**
 * A mode, named from the review surface.
 *
 * `review_policy` is now a required member of `ModeDescriptor` itself, because
 * the route declares it: the daemon always sends it. The alias stays so the
 * review components keep reading one name, and so the two files do not import
 * each other in a cycle.
 */
export type ReviewAwareMode = Schemas['ModeRow'];

/**
 * A hunk nobody's ledger claims: the user's own editor, an async formatter, a
 * plugin writing directly. Shown for context so the composed diff stays
 * honest, never rejectable — reverting one destroys the user's concurrent work
 * while reporting that an agent edit was undone.
 */
export function isExternal(hunk: ComposedHunk): boolean {
  return hunk.tool_call_ids.length === 0;
}

/** Absolute path of a hunk: its root joined with its root-relative path. */
export function hunkPath(hunk: ComposedHunk): string {
  return `${hunk.root.replace(/\/$/, '')}/${hunk.path}`;
}

/**
 * The hunk's lines as a human reads them: `L4–5`, or `L4` for one line.
 *
 * `current_range` is half-open, so `end` is one PAST the last line and an
 * empty range is a pure deletion with no line left to name.
 */
export function hunkRangeLabel(hunk: ComposedHunk): string {
  const r = hunk.current_range;
  return r.end > r.start + 1 ? `L${r.start}–${r.end - 1}` : `L${r.start}`;
}
